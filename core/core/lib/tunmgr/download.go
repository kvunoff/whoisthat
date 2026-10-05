package tunmgr

import (
	"archive/zip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"time"
	"whoisthat-core/lib/logger"
)

// Mirrors to try in order when downloading from GitHub Releases.
var downloadMirrors = []string{
	"https://github.com/xjasonlyu/tun2socks/releases/download",
	"https://ghfast.top/https://github.com/xjasonlyu/tun2socks/releases/download",
}

// PinnedSHA256 maps architecture to verified SHA256 hashes of the release zip archives for PinnedVersion (v2.7.0).
var PinnedSHA256 = map[string]string{
	"amd64": "a612baa287a3b6de6221f74fd02b442a50888508227ecf51e1288a5ccbb77381",
	"arm64": "3931476c9cfa8fa236d23aeaf36767df0eb27cc11ecaab699faba57744450f49",
	"386":   "d5ebf3c1840c0ec121bfd577bd5edf751b8714eb16a19e9667ea5e2267108606",
	"armv7": "90875d7c09af28ed9b0dc51a2d17b317af832553e2e3de1d3365351d14bd7762",
}

// GetTargetArch maps runtime.GOARCH to tun2socks release asset architecture naming.
func GetTargetArch() (string, error) {
	switch runtime.GOARCH {
	case "amd64":
		return "amd64", nil
	case "arm64":
		return "arm64", nil
	case "386":
		return "386", nil
	case "arm":
		return "armv7", nil
	default:
		return "", fmt.Errorf("unsupported system architecture for tun2socks: %s", runtime.GOARCH)
	}
}

// downloadArchive downloads the tun2socks zip file, reporting progress via callback.
func downloadArchive(client *http.Client, version, arch, destPath string, progressCb func(current, total int64)) error {
	zipFileName := fmt.Sprintf("tun2socks-linux-%s.zip", arch)
	var lastErr error

	for _, mirror := range downloadMirrors {
		url := fmt.Sprintf("%s/%s/%s", mirror, version, zipFileName)
		logger.Infof("tunmgr: downloading archive from %s", url)

		req, err := http.NewRequestWithContext(context.Background(), "GET", url, nil)
		if err != nil {
			lastErr = err
			continue
		}

		resp, err := client.Do(req)
		if err != nil {
			lastErr = err
			continue
		}

		if resp.StatusCode != http.StatusOK {
			resp.Body.Close()
			lastErr = fmt.Errorf("HTTP %d from %s", resp.StatusCode, url)
			continue
		}

		total := resp.ContentLength
		out, err := os.Create(destPath)
		if err != nil {
			resp.Body.Close()
			return fmt.Errorf("failed to create destination file %s: %w", destPath, err)
		}

		var current int64
		buf := make([]byte, 64*1024)
		lastReport := time.Now()

		for {
			n, rErr := resp.Body.Read(buf)
			if n > 0 {
				if _, wErr := out.Write(buf[:n]); wErr != nil {
					out.Close()
					resp.Body.Close()
					return fmt.Errorf("failed to write archive: %w", wErr)
				}
				current += int64(n)
				if progressCb != nil && (time.Since(lastReport) > 100*time.Millisecond || current == total) {
					progressCb(current, total)
					lastReport = time.Now()
				}
			}
			if rErr != nil {
				if rErr == io.EOF {
					break
				}
				out.Close()
				resp.Body.Close()
				lastErr = rErr
				break
			}
		}

		out.Close()
		resp.Body.Close()

		if lastErr == nil {
			return nil
		}
	}

	return fmt.Errorf("failed to download tun2socks archive from all mirrors: %w", lastErr)
}

// verifySHA256 computes the SHA2-256 hash of the file and compares it with expected.
func verifySHA256(filePath, expectedSHA string) error {
	f, err := os.Open(filePath)
	if err != nil {
		return err
	}
	defer f.Close()

	hasher := sha256.New()
	if _, err := io.Copy(hasher, f); err != nil {
		return err
	}

	actualSHA := hex.EncodeToString(hasher.Sum(nil))
	if !strings.EqualFold(actualSHA, expectedSHA) {
		return fmt.Errorf("checksum mismatch: expected %s, got %s", expectedSHA, actualSHA)
	}
	return nil
}

// unzipArchive extracts the archive into destDir and ensures the binary is named 'tun2socks' and executable.
func unzipArchive(zipPath, destDir string) error {
	r, err := zip.OpenReader(zipPath)
	if err != nil {
		return fmt.Errorf("failed to open zip %s: %w", zipPath, err)
	}
	defer r.Close()

	if err := os.MkdirAll(destDir, 0755); err != nil {
		return fmt.Errorf("failed to create target dir %s: %w", destDir, err)
	}

	for _, f := range r.File {
		cleanName := filepath.Clean(f.Name)
		targetPath := filepath.Join(destDir, cleanName)
		rel, relErr := filepath.Rel(destDir, targetPath)
		if relErr != nil || strings.HasPrefix(rel, "..") || filepath.IsAbs(rel) {
			continue // Prevent path traversal (Zip-Slip)
		}

		if f.FileInfo().IsDir() {
			if err := os.MkdirAll(targetPath, 0755); err != nil {
				return err
			}
			continue
		}

		if err := os.MkdirAll(filepath.Dir(targetPath), 0755); err != nil {
			return err
		}

		rc, err := f.Open()
		if err != nil {
			return fmt.Errorf("failed to open zipped file %s: %w", f.Name, err)
		}

		mode := f.Mode()
		if strings.HasPrefix(filepath.Base(targetPath), "tun2socks") {
			mode = 0755
		}

		out, err := os.OpenFile(targetPath, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, mode)
		if err != nil {
			rc.Close()
			return fmt.Errorf("failed to open target file %s: %w", targetPath, err)
		}

		_, err = io.Copy(out, rc)
		out.Close()
		rc.Close()
		if err != nil {
			return fmt.Errorf("failed to extract file %s: %w", f.Name, err)
		}

		if strings.HasPrefix(filepath.Base(targetPath), "tun2socks") {
			_ = os.Chmod(targetPath, 0755)
		}
	}

	// Locate the extracted binary and ensure it is named 'tun2socks'
	destBin := filepath.Join(destDir, "tun2socks")
	if _, err := os.Stat(destBin); err != nil {
		var foundPath string
		err := filepath.WalkDir(destDir, func(path string, d os.DirEntry, walkErr error) error {
			if walkErr != nil || d.IsDir() {
				return walkErr
			}
			base := filepath.Base(path)
			ext := filepath.Ext(base)
			// Skip documentation, licenses, and compressed metadata
			if ext == ".txt" || ext == ".md" || ext == ".zip" || ext == ".json" {
				return nil
			}
			if strings.HasPrefix(base, "tun2socks") {
				foundPath = path
				return filepath.SkipAll
			}
			return nil
		})
		if err != nil {
			return fmt.Errorf("failed to scan destDir: %w", err)
		}
		if foundPath == "" {
			return fmt.Errorf("extracted archive did not contain tun2socks binary")
		}
		// Move/rename it to destBin
		if err := os.Rename(foundPath, destBin); err != nil {
			return fmt.Errorf("failed to rename %s to tun2socks: %w", foundPath, err)
		}
	}

	_ = os.Chmod(destBin, 0755)
	return nil
}
