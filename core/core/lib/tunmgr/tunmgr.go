package tunmgr

import (
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

type Manager struct {
	mu            sync.RWMutex
	status        structs.Tun2socksStatusInfo
	binPath       string
	onProgress    func(structs.Tun2socksStatusInfo)
	isDownloading bool
	downloadDone  chan struct{}
	httpClient    *http.Client
}

var (
	defaultManager *Manager
	once           sync.Once
)

// GetManager returns the singleton tun2socks runtime manager instance.
func GetManager() *Manager {
	once.Do(func() {
		defaultManager = NewManager()
	})
	return defaultManager
}

// NewManager creates a new instance of tun2socks manager.
func NewManager() *Manager {
	return &Manager{
		status: structs.Tun2socksStatusInfo{
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
func (m *Manager) SetProgressCallback(fn func(structs.Tun2socksStatusInfo)) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.onProgress = fn
}

func (m *Manager) notify(st structs.Tun2socksStatusInfo) {
	m.mu.RLock()
	cb := m.onProgress
	m.mu.RUnlock()
	if cb != nil {
		cb(st)
	}
}

// GetStatus returns the current tun2socks status info.
func (m *Manager) GetStatus() structs.Tun2socksStatusInfo {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.status
}

// GetTun2socksBin returns the path to the verified tun2socks binary if ready,
// or ensures it is prepared if missing.
func (m *Manager) GetTun2socksBin() (string, error) {
	m.mu.RLock()
	if m.status.Status == "ready" && m.binPath != "" {
		defer m.mu.RUnlock()
		return m.binPath, nil
	}
	m.mu.RUnlock()

	if err := m.EnsureReady(); err != nil {
		m.mu.RLock()
		defer m.mu.RUnlock()
		if m.status.Status == "downloading" {
			return "", fmt.Errorf("tun2socks %s is downloading (%d%%)", PinnedVersion, m.status.Progress)
		}
		return "", fmt.Errorf("tun2socks %s is not ready (status: %s): %w", PinnedVersion, m.status.Status, err)
	}

	m.mu.RLock()
	defer m.mu.RUnlock()
	if m.status.Status == "ready" && m.binPath != "" {
		return m.binPath, nil
	}
	return "", fmt.Errorf("tun2socks %s is not ready (status: %s)", PinnedVersion, m.status.Status)
}

// RuntimeDir returns the directory path for the pinned tun2socks version.
func RuntimeDir() (string, error) {
	homeDir, err := utils.GetHomeDir()
	if err != nil {
		return "", fmt.Errorf("failed to get user home directory: %w", err)
	}
	return filepath.Join(homeDir, ".local", "share", "whoisthat", "runtimes", "tun2socks", PinnedVersion), nil
}

// Init inspects existing managed runtimes and system binaries to resolve the pinned tun2socks binary.
func (m *Manager) Init() error {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.status.TargetVersion = PinnedVersion

	runtimeDir, err := RuntimeDir()
	if err == nil {
		// Clean up any stale temp directories in baseDir
		baseDir := filepath.Dir(runtimeDir)
		if entries, readErr := os.ReadDir(baseDir); readErr == nil {
			for _, e := range entries {
				if e.IsDir() && strings.Contains(e.Name(), ".tmp-") {
					_ = os.RemoveAll(filepath.Join(baseDir, e.Name()))
				}
			}
		}

		managedBin := filepath.Join(runtimeDir, "tun2socks")
		if info, err := os.Stat(managedBin); err == nil && !info.IsDir() {
			if matches, ver, err := CheckBinaryVersion(managedBin, PinnedVersion); err == nil && matches {
				m.binPath = managedBin
				m.status.Status = "ready"
				m.status.Version = ver
				m.status.Progress = 100
				logger.Infof("tunmgr: managed tun2socks %s verified ready at %s", ver, managedBin)
				return nil
			} else {
				logger.Warnf("tunmgr: existing managed binary %s did not pass check: matches=%v, ver=%s, err=%v", managedBin, matches, ver, err)
			}
		}
	}

	// Fallback check: look for system binary matching the exact pinned version
	sysCandidates := []string{
		"/usr/local/bin/tun2socks",
		"/usr/bin/tun2socks",
	}
	if p, err := exec.LookPath("tun2socks"); err == nil {
		if abs, err := filepath.Abs(p); err == nil {
			sysCandidates = append([]string{abs}, sysCandidates...)
		}
	}

	for _, sysPath := range utils.RemoveDuplicates(sysCandidates) {
		if info, err := os.Stat(sysPath); err == nil && !info.IsDir() {
			if matches, ver, err := CheckBinaryVersion(sysPath, PinnedVersion); err == nil && matches {
				logger.Infof("tunmgr: system tun2socks at %s matches pinned version %s, adopting into managed runtime", sysPath, PinnedVersion)
				if runtimeDir != "" {
					if err := m.adoptSystemBinary(sysPath, runtimeDir); err == nil {
						m.binPath = filepath.Join(runtimeDir, "tun2socks")
						m.status.Status = "ready"
						m.status.Version = ver
						m.status.Progress = 100
						return nil
					}
				}
				// If copying failed or runtimeDir unavailable, we can still use the system binary directly
				m.binPath = sysPath
				m.status.Status = "ready"
				m.status.Version = ver
				m.status.Progress = 100
				return nil
			} else {
				logger.Warnf("tunmgr: system binary at %s has version %s (pinned is %s, incompatible)", sysPath, ver, PinnedVersion)
			}
		}
	}

	m.status.Status = "missing"
	m.status.Progress = 0
	return nil
}

// adoptSystemBinary copies a matching system binary into the managed runtime directory.
// It writes to a temporary file first and renames atomically to avoid ETXTBSY on running binaries.
func (m *Manager) adoptSystemBinary(srcPath, destDir string) error {
	if destDir == "" {
		return fmt.Errorf("destination directory cannot be empty")
	}
	destPath := filepath.Join(destDir, "tun2socks")
	if srcPath == destPath {
		return nil
	}

	if err := os.MkdirAll(destDir, 0755); err != nil {
		return err
	}

	srcFile, err := os.Open(srcPath)
	if err != nil {
		return err
	}
	defer srcFile.Close()

	tmpPath := fmt.Sprintf("%s.tmp-%d", destPath, time.Now().UnixNano())
	destFile, err := os.OpenFile(tmpPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0755)
	if err != nil {
		return err
	}

	if _, err := io.Copy(destFile, srcFile); err != nil {
		destFile.Close()
		_ = os.Remove(tmpPath)
		return err
	}
	if err := destFile.Close(); err != nil {
		_ = os.Remove(tmpPath)
		return err
	}
	if err := os.Chmod(tmpPath, 0755); err != nil {
		_ = os.Remove(tmpPath)
		return err
	}
	return os.Rename(tmpPath, destPath)
}

// EnsureReady ensures the pinned tun2socks binary is ready, downloading and installing if needed.
// It blocks until the binary is ready or an error occurs.
func (m *Manager) EnsureReady() error {
	m.mu.Lock()
	if m.status.Status == "ready" && m.binPath != "" {
		m.mu.Unlock()
		return nil
	}
	if m.isDownloading {
		done := m.downloadDone
		m.mu.Unlock()
		if done != nil {
			<-done
		}
		m.mu.RLock()
		defer m.mu.RUnlock()
		if m.status.Status == "ready" && m.binPath != "" {
			return nil
		}
		return fmt.Errorf("tun2socks preparation failed: %s", m.status.Error)
	}

	m.isDownloading = true
	m.downloadDone = make(chan struct{})
	m.status.Status = "downloading"
	m.status.Progress = 0
	m.status.Error = ""
	stCopy := m.status
	done := m.downloadDone
	m.mu.Unlock()

	m.notify(stCopy)

	var downloadErr error
	defer func() {
		m.mu.Lock()
		m.isDownloading = false
		if r := recover(); r != nil {
			m.status.Status = "error"
			m.status.Error = fmt.Sprintf("panic: %v", r)
			downloadErr = fmt.Errorf("panic during download: %v", r)
		} else if downloadErr != nil {
			m.status.Status = "error"
			m.status.Error = downloadErr.Error()
			logger.Errorf("tunmgr: download and installation failed: %v", downloadErr)
		} else {
			m.status.Status = "ready"
			m.status.Version = PinnedVersion
			m.status.Progress = 100
			m.status.Error = ""
			logger.Infof("tunmgr: tun2socks %s ready at %s", PinnedVersion, m.binPath)
		}
		st := m.status
		close(done)
		m.mu.Unlock()
		m.notify(st)
	}()

	downloadErr = m.downloadAndInstall()
	return downloadErr
}

// EnsureRuntimeAsync starts a background download if the binary is missing or not ready.
func (m *Manager) EnsureRuntimeAsync() {
	m.mu.Lock()
	if m.status.Status == "ready" || m.isDownloading {
		m.mu.Unlock()
		return
	}
	m.isDownloading = true
	m.downloadDone = make(chan struct{})
	m.status.Status = "downloading"
	m.status.Progress = 0
	m.status.Error = ""
	stCopy := m.status
	done := m.downloadDone
	m.mu.Unlock()

	m.notify(stCopy)

	go func() {
		var downloadErr error
		defer func() {
			m.mu.Lock()
			m.isDownloading = false
			if r := recover(); r != nil {
				m.status.Status = "error"
				m.status.Error = fmt.Sprintf("panic: %v", r)
			} else if downloadErr != nil {
				m.status.Status = "error"
				m.status.Error = downloadErr.Error()
				logger.Errorf("tunmgr: download and installation failed: %v", downloadErr)
			} else {
				m.status.Status = "ready"
				m.status.Version = PinnedVersion
				m.status.Progress = 100
				m.status.Error = ""
				logger.Infof("tunmgr: tun2socks %s ready at %s", PinnedVersion, m.binPath)
			}
			st := m.status
			close(done)
			m.mu.Unlock()
			m.notify(st)
		}()

		downloadErr = m.downloadAndInstall()
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

	// Clean up any stale temp directories in baseDir before downloading
	if entries, readErr := os.ReadDir(baseDir); readErr == nil {
		for _, e := range entries {
			if e.IsDir() && strings.Contains(e.Name(), ".tmp-") {
				_ = os.RemoveAll(filepath.Join(baseDir, e.Name()))
			}
		}
	}

	tempDir := filepath.Join(baseDir, fmt.Sprintf("%s.tmp-%d", PinnedVersion, time.Now().UnixNano()))
	if err := os.MkdirAll(tempDir, 0755); err != nil {
		return fmt.Errorf("failed to create temporary directory: %w", err)
	}
	defer os.RemoveAll(tempDir)

	zipDest := filepath.Join(tempDir, "tun2socks.zip")

	// 1. Download zip with progress reporting
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
		return fmt.Errorf("failed to download tun2socks archive: %w", err)
	}

	// 2. Verify SHA256 if expected hash exists
	if expectedSHA, ok := PinnedSHA256[arch]; ok && expectedSHA != "" {
		if err := verifySHA256(zipDest, expectedSHA); err != nil {
			return fmt.Errorf("checksum verification failed: %w", err)
		}
		logger.Infof("tunmgr: checksum verified OK: %s", expectedSHA)
	}

	// 3. Extract archive
	extractedDir := filepath.Join(tempDir, "extracted")
	if err := unzipArchive(zipDest, extractedDir); err != nil {
		return fmt.Errorf("failed to extract tun2socks archive: %w", err)
	}

	testBin := filepath.Join(extractedDir, "tun2socks")
	if _, err := os.Stat(testBin); err != nil {
		return fmt.Errorf("extracted archive did not contain tun2socks binary: %w", err)
	}
	_ = os.Chmod(testBin, 0755)

	// 4. Test run the extracted binary
	matches, actualVer, err := CheckBinaryVersion(testBin, PinnedVersion)
	if err != nil {
		return fmt.Errorf("downloaded binary failed verification: %w", err)
	}
	if !matches {
		return fmt.Errorf("downloaded binary version %q did not match pinned version %q", actualVer, PinnedVersion)
	}

	// 5. Move to final runtime directory
	_ = os.RemoveAll(runtimeDir)
	if err := os.Rename(extractedDir, runtimeDir); err != nil {
		return fmt.Errorf("failed to move extracted runtime to %s: %w", runtimeDir, err)
	}

	finalBin := filepath.Join(runtimeDir, "tun2socks")
	m.mu.Lock()
	m.binPath = finalBin
	m.mu.Unlock()

	return nil
}
