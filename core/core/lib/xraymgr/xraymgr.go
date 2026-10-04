package xraymgr

import (
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"time"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

type Manager struct {
	mu            sync.RWMutex
	status        structs.XrayStatusInfo
	binPath       string
	onProgress    func(structs.XrayStatusInfo)
	isDownloading bool
	httpClient    *http.Client
}

var (
	defaultManager *Manager
	once           sync.Once
)

// GetManager returns the singleton Xray runtime manager instance.
func GetManager() *Manager {
	once.Do(func() {
		defaultManager = NewManager()
	})
	return defaultManager
}

// NewManager creates a new instance of Xray manager.
func NewManager() *Manager {
	return &Manager{
		status: structs.XrayStatusInfo{
			TargetVersion: PinnedVersion,
			Status:        "missing",
			Progress:      0,
		},
		httpClient: &http.Client{
			Timeout: 120 * time.Second,
		},
	}
}

// SetProgressCallback registers a callback for live status and progress updates.
func (m *Manager) SetProgressCallback(fn func(structs.XrayStatusInfo)) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.onProgress = fn
}

func (m *Manager) notify(st structs.XrayStatusInfo) {
	m.mu.RLock()
	cb := m.onProgress
	m.mu.RUnlock()
	if cb != nil {
		cb(st)
	}
}

// GetStatus returns the current Xray status info.
func (m *Manager) GetStatus() structs.XrayStatusInfo {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.status
}

// GetXrayBin returns the path to the verified Xray binary if ready.
func (m *Manager) GetXrayBin() (string, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	if m.status.Status == "ready" && m.binPath != "" {
		return m.binPath, nil
	}
	if m.status.Status == "downloading" {
		return "", fmt.Errorf("Xray-core %s is downloading (%d%%)", PinnedVersion, m.status.Progress)
	}
	return "", fmt.Errorf("Xray-core %s is not ready (status: %s)", PinnedVersion, m.status.Status)
}

// RuntimeDir returns the directory path for the pinned Xray version.
func RuntimeDir() (string, error) {
	homeDir, err := utils.GetHomeDir()
	if err != nil {
		return "", fmt.Errorf("failed to get user home directory: %w", err)
	}
	return filepath.Join(homeDir, ".local", "share", "whoisthat", "runtimes", "xray", PinnedVersion), nil
}

// Init inspects existing managed runtimes and system binaries to resolve the pinned Xray binary.
func (m *Manager) Init() error {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.status.TargetVersion = PinnedVersion

	runtimeDir, err := RuntimeDir()
	if err == nil {
		managedBin := filepath.Join(runtimeDir, "xray")
		if info, err := os.Stat(managedBin); err == nil && !info.IsDir() {
			if matches, ver, err := CheckBinaryVersion(managedBin, PinnedVersion); err == nil && matches {
				m.binPath = managedBin
				m.status.Status = "ready"
				m.status.Version = ver
				m.status.Progress = 100
				logger.Infof("xraymgr: managed Xray %s verified ready at %s", ver, managedBin)
				return nil
			} else {
				logger.Warnf("xraymgr: existing managed binary %s did not pass check: matches=%v, ver=%s, err=%v", managedBin, matches, ver, err)
			}
		}
	}

	// Fallback check: look for system binary matching the exact pinned version
	sysCandidates := []string{
		"/usr/local/bin/xray",
		"/usr/bin/xray",
	}
	if p, err := exec.LookPath("xray"); err == nil {
		if abs, err := filepath.Abs(p); err == nil {
			sysCandidates = append([]string{abs}, sysCandidates...)
		}
	}

	for _, sysPath := range utils.RemoveDuplicates(sysCandidates) {
		if info, err := os.Stat(sysPath); err == nil && !info.IsDir() {
			if matches, ver, err := CheckBinaryVersion(sysPath, PinnedVersion); err == nil && matches {
				logger.Infof("xraymgr: system xray at %s matches pinned version %s, adopting into managed runtime", sysPath, PinnedVersion)
				if err := m.adoptSystemBinary(sysPath, runtimeDir); err == nil {
					m.binPath = filepath.Join(runtimeDir, "xray")
					m.status.Status = "ready"
					m.status.Version = ver
					m.status.Progress = 100
					return nil
				}
				// If copying failed, we can still use the system binary directly
				m.binPath = sysPath
				m.status.Status = "ready"
				m.status.Version = ver
				m.status.Progress = 100
				return nil
			} else {
				logger.Warnf("xraymgr: system binary at %s has version %s (pinned is %s, incompatible)", sysPath, ver, PinnedVersion)
			}
		}
	}

	m.status.Status = "missing"
	m.status.Progress = 0
	return nil
}

// adoptSystemBinary copies a matching system binary into the managed runtime directory.
func (m *Manager) adoptSystemBinary(srcPath, destDir string) error {
	if err := os.MkdirAll(destDir, 0755); err != nil {
		return err
	}
	destPath := filepath.Join(destDir, "xray")

	srcFile, err := os.Open(srcPath)
	if err != nil {
		return err
	}
	defer srcFile.Close()

	destFile, err := os.OpenFile(destPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0755)
	if err != nil {
		return err
	}
	defer destFile.Close()

	if _, err := io.Copy(destFile, srcFile); err != nil {
		return err
	}
	return os.Chmod(destPath, 0755)
}

// EnsureRuntimeAsync starts a background download if the binary is missing or not ready.
func (m *Manager) EnsureRuntimeAsync() {
	m.mu.Lock()
	if m.status.Status == "ready" || m.isDownloading {
		m.mu.Unlock()
		return
	}
	m.isDownloading = true
	m.status.Status = "downloading"
	m.status.Progress = 0
	m.status.Error = ""
	stCopy := m.status
	m.mu.Unlock()

	m.notify(stCopy)

	go func() {
		err := m.downloadAndInstall()
		m.mu.Lock()
		m.isDownloading = false
		if err != nil {
			m.status.Status = "error"
			m.status.Error = err.Error()
			logger.Errorf("xraymgr: download and installation failed: %v", err)
		} else {
			m.status.Status = "ready"
			m.status.Version = PinnedVersion
			m.status.Progress = 100
			m.status.Error = ""
			logger.Infof("xraymgr: Xray %s ready at %s", PinnedVersion, m.binPath)
		}
		st := m.status
		m.mu.Unlock()
		m.notify(st)
	}()
}

// downloadAndInstall performs the complete download, verification and installation flow.
func (m *Manager) downloadAndInstall() error {
	arch, err := GetTargetArch()
	if err != nil {
		return err
	}

	runtimeDir, err := RuntimeDir()
	if err != nil {
		return err
	}
	baseDir := filepath.Dir(runtimeDir)
	if err := os.MkdirAll(baseDir, 0755); err != nil {
		return fmt.Errorf("failed to create runtimes directory: %w", err)
	}

	tempDir := filepath.Join(baseDir, fmt.Sprintf("%s.tmp-%d", PinnedVersion, time.Now().UnixNano()))
	if err := os.MkdirAll(tempDir, 0755); err != nil {
		return fmt.Errorf("failed to create temporary directory: %w", err)
	}
	defer os.RemoveAll(tempDir)

	zipDest := filepath.Join(tempDir, "xray.zip")

	// 1. Fetch expected digest
	expectedSHA, err := fetchDigest(m.httpClient, PinnedVersion, arch)
	if err != nil {
		logger.Warnf("xraymgr: failed to fetch digest: %v (will proceed with archive download and binary test)", err)
	}

	// 2. Download zip with progress reporting
	err = downloadArchive(m.httpClient, PinnedVersion, arch, zipDest, func(current, total int64) {
		m.mu.Lock()
		m.status.BytesDownloaded = current
		m.status.TotalBytes = total
		if total > 0 {
			m.status.Progress = int((current * 100) / total)
		}
		st := m.status
		m.mu.Unlock()
		m.notify(st)
	})
	if err != nil {
		return fmt.Errorf("failed to download Xray archive: %w", err)
	}

	// 3. Verify SHA256 if digest was obtained
	if expectedSHA != "" {
		if err := verifySHA256(zipDest, expectedSHA); err != nil {
			return fmt.Errorf("checksum verification failed: %w", err)
		}
		logger.Infof("xraymgr: checksum verified OK: %s", expectedSHA)
	}

	// 4. Extract archive
	extractedDir := filepath.Join(tempDir, "extracted")
	if err := unzipArchive(zipDest, extractedDir); err != nil {
		return fmt.Errorf("failed to extract Xray archive: %w", err)
	}

	testBin := filepath.Join(extractedDir, "xray")
	if _, err := os.Stat(testBin); err != nil {
		return fmt.Errorf("extracted archive did not contain xray binary: %w", err)
	}
	_ = os.Chmod(testBin, 0755)

	// 5. Test run the extracted binary
	matches, actualVer, err := CheckBinaryVersion(testBin, PinnedVersion)
	if err != nil {
		return fmt.Errorf("downloaded binary failed verification: %w", err)
	}
	if !matches {
		return fmt.Errorf("downloaded binary version %q did not match pinned version %q", actualVer, PinnedVersion)
	}

	// 6. Move to final runtime directory
	_ = os.RemoveAll(runtimeDir)
	if err := os.Rename(extractedDir, runtimeDir); err != nil {
		return fmt.Errorf("failed to move extracted runtime to %s: %w", runtimeDir, err)
	}

	finalBin := filepath.Join(runtimeDir, "xray")
	m.mu.Lock()
	m.binPath = finalBin
	m.mu.Unlock()

	return nil
}
