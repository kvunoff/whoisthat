package xraymgr

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
	"regexp"
	"runtime"
	"strings"
	"time"
	"whoisthat-core/lib/logger"
)

var dgstSha256Regex = regexp.MustCompile(`(?i)SHA2-256=\s*([a-f0-9]{64})`)

// Mirrors to try in order when downloading from GitHub Releases.
var downloadMirrors = []string{
	"https://github.com/XTLS/Xray-core/releases/download",
	"https://ghfast.top/https://github.com/XTLS/Xray-core/releases/download",
}

// GetTargetArch maps runtime.GOARCH to Xray release asset architecture naming.
func GetTargetArch() (string, error) {
	switch runtime.GOARCH {
	case "amd64":
		return "64", nil
	case "arm64":
		return "arm64-v8a", nil
	case "arm":
		return "arm32-v7a", nil
	default:
		return "", fmt.Errorf("unsupported system architecture for Xray-core: %s", runtime.GOARCH)
	}
}

// ParseDgstSHA256 extracts the SHA2-256 hex string from the content of a .dgst file.
func ParseDgstSHA256(dgstContent string) (string, error) {
	matches := dgstSha256Regex.FindStringSubmatch(dgstContent)
	if len(matches) >= 2 {
		return strings.ToLower(matches[1]), nil
	}
	return "", fmt.Errorf("no SHA2-256 entry found in .dgst content")
}

// fetchDigest downloads the .dgst file for the given version and arch and extracts the expected SHA2-256 checksum.
func fetchDigest(client *http.Client, version, arch string) (string, error) {
	dgstFileName := fmt.Sprintf("Xray-linux-%s.zip.dgst", arch)
	var lastErr error

	for _, mirror := range downloadMirrors {
		url := fmt.Sprintf("%s/%s/%s", mirror, version, dgstFileName)
		logger.Infof("xraymgr: fetching digest from %s", url)

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

		bodyBytes, err := io.ReadAll(resp.Body)
		resp.Body.Close()
		if err != nil {
			lastErr = err
			continue
		}

		sha, err := ParseDgstSHA256(string(bodyBytes))
		if err == nil {
			return sha, nil
		}
		lastErr = err
	}

	return "", fmt.Errorf("failed to fetch digest: %w", lastErr)
}

// downloadArchive downloads the Xray zip file, reporting progress via callback.
func downloadArchive(client *http.Client, version, arch, destPath string, progressCb func(current, total int64)) error {
	zipFileName := fmt.Sprintf("Xray-linux-%s.zip", arch)
	var lastErr error

	for _, mirror := range downloadMirrors {
		url := fmt.Sprintf("%s/%s/%s", mirror, version, zipFileName)
		logger.Infof("xraymgr: downloading archive from %s", url)

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

	return fmt.Errorf("failed to download archive from all mirrors: %w", lastErr)
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

// unzipArchive extracts the archive into destDir and ensures files have appropriate permissions.
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
		if strings.HasPrefix(cleanName, "..") || filepath.IsAbs(cleanName) {
			continue // Prevent path traversal
		}

		targetPath := filepath.Join(destDir, cleanName)
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
		if filepath.Base(targetPath) == "xray" {
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

		if filepath.Base(targetPath) == "xray" {
			_ = os.Chmod(targetPath, 0755)
		}
	}

	return nil
}
