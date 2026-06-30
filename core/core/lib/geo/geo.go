package geo

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"time"
	"whoisthat-core/lib/logger"
)

const (
	minGeoIPSize    = 10 * 1024 * 1024
	minGeoSiteSize  = 1 * 1024 * 1024
	downloadTimeout = 60 * time.Second
	verifyTimeout   = 5 * time.Second
)

// geoIPURLs / geoSiteURLs are pinned to specific release tags rather than
// /latest/download/ so a compromise of the v2fly release page can't silently
// swap a malicious geoip.dat onto every install. Bump the pinned tag when
// upgrading; the xray-test verification in verifyGeoIP is a second line of
// defense, not a primary one.
var geoIPURLs = []string{
	"https://github.com/v2fly/geoip/releases/download/v20240922/geoip.dat",
}

var geoSiteURLs = []string{
	"https://github.com/v2fly/domain-list-community/releases/download/v20240922/dlc.dat",
}

var systemAssetDirs = []string{
	"/usr/share/xray",
	"/usr/local/share/xray",
	filepath.Join(os.Getenv("HOME"), ".config", "koala-clash", "work"),
	filepath.Join(os.Getenv("HOME"), ".config", "clash-meta", "work"),
}

type Status int

const (
	StatusPending Status = iota
	StatusReady
	StatusFailed
)

var (
	assetDir string
	status   Status
	mu       sync.Mutex
	readyCh  = make(chan struct{})
)

func AssetDir() string {
	return assetDir
}

func IsReady() bool {
	mu.Lock()
	defer mu.Unlock()
	return status == StatusReady
}

func WaitReady(timeout time.Duration) bool {
	mu.Lock()
	s := status
	mu.Unlock()
	if s != StatusPending {
		return s == StatusReady
	}
	select {
	case <-readyCh:
		return IsReady()
	case <-time.After(timeout):
		return IsReady()
	}
}

func EnsureAssets(dir string) (string, error) {
	if err := os.MkdirAll(dir, 0755); err != nil {
		setStatus(StatusFailed)
		return "", fmt.Errorf("geo: cannot create asset dir %s: %w", dir, err)
	}
	assetDir = dir

	geoIPPath := filepath.Join(dir, "geoip.dat")
	geoSitePath := filepath.Join(dir, "geosite.dat")

	if validFile(geoIPPath, minGeoIPSize) {
		logger.Info("geo: geoip.dat already exists and looks valid")
	} else if found := findAndCopy(geoIPPath, "geoip.dat"); found {
		logger.Info("geo: geoip.dat found on system")
	} else {
		logger.Info("geo: downloading geoip.dat...")
		if err := downloadWithRetry(geoIPURLs, geoIPPath, minGeoIPSize); err != nil {
			logger.Warnf("geo: failed to download geoip.dat: %v", err)
		}
	}

	if validFile(geoSitePath, minGeoSiteSize) {
		logger.Info("geo: geosite.dat already exists and looks valid")
	} else if found := findAndCopy(geoSitePath, "geosite.dat"); found {
		logger.Info("geo: geosite.dat found on system")
	} else {
		logger.Info("geo: downloading geosite.dat...")
		if err := downloadWithRetry(geoSiteURLs, geoSitePath, minGeoSiteSize); err != nil {
			logger.Warnf("geo: failed to download geosite.dat: %v", err)
		}
	}

	if validFile(geoIPPath, minGeoIPSize) && verifyGeoIP(geoIPPath) {
		logger.Info("geo: geoip.dat verified OK")
		setStatus(StatusReady)
	} else if validFile(geoIPPath, minGeoIPSize) {
		logger.Warn("geo: geoip.dat failed xray verification, geo rules will not work")
		setStatus(StatusFailed)
	} else {
		logger.Warn("geo: no valid geoip.dat available, geo rules will not work")
		setStatus(StatusFailed)
	}

	return dir, nil
}

func setStatus(s Status) {
	mu.Lock()
	defer mu.Unlock()
	status = s
	select {
	case <-readyCh:
	default:
		close(readyCh)
	}
}

func validFile(path string, minSize int64) bool {
	fi, err := os.Stat(path)
	if err != nil {
		return false
	}
	return fi.Size() >= minSize
}

func findAndCopy(dest, name string) bool {
	for _, d := range systemAssetDirs {
		src := filepath.Join(d, name)
		if !validFile(src, 1) {
			continue
		}
		logger.Infof("geo: found %s at %s", name, src)
		in, err := os.Open(src)
		if err != nil {
			continue
		}
		out, err := os.Create(dest)
		if err != nil {
			in.Close()
			continue
		}
		_, err = io.Copy(out, in)
		in.Close()
		out.Close()
		if err != nil {
			os.Remove(dest)
			continue
		}
		return true
	}
	return false
}

func downloadWithRetry(urls []string, dest string, minSize int64) error {
	var lastErr error
	for _, url := range urls {
		for attempt := 0; attempt < 3; attempt++ {
			if attempt > 0 {
				time.Sleep(time.Duration(1<<attempt) * time.Second)
			}
			logger.Infof("geo: downloading %s (attempt %d)", url, attempt+1)
			if err := download(url, dest, minSize); err != nil {
				lastErr = err
				logger.Warnf("geo: download failed: %v", err)
				continue
			}
			if verifyGeoIP(dest) {
				return nil
			}
			logger.Warnf("geo: %s failed xray verification, retrying", dest)
			os.Remove(dest)
			lastErr = fmt.Errorf("verification failed")
		}
	}
	return fmt.Errorf("all download attempts failed: %w", lastErr)
}

func download(url, dest string, minSize int64) error {
	client := &http.Client{Timeout: downloadTimeout}
	resp, err := client.Get(url)
	if err != nil {
		return fmt.Errorf("http get: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("http status %d", resp.StatusCode)
	}

	tmp := dest + ".tmp"
	out, err := os.Create(tmp)
	if err != nil {
		return fmt.Errorf("create tmp: %w", err)
	}

	written, err := io.Copy(out, resp.Body)
	if err != nil {
		out.Close()
		os.Remove(tmp)
		return fmt.Errorf("write: %w", err)
	}
	out.Close()

	if written < minSize {
		os.Remove(tmp)
		return fmt.Errorf("too small: %d bytes (min %d)", written, minSize)
	}

	if err := os.Rename(tmp, dest); err != nil {
		os.Remove(tmp)
		return fmt.Errorf("rename: %w", err)
	}

	logger.Infof("geo: downloaded %d bytes to %s", written, dest)
	return nil
}

func verifyGeoIP(path string) bool {
	xrayBin, err := exec.LookPath("xray")
	if err != nil {
		// Without xray we cannot run the verification test at all. Returning
		// true here would silently bless any file (including a malicious one).
		// Fail closed: the caller falls back to "no geo rules" mode.
		logger.Warnf("geo: cannot verify geoip.dat: xray not found: %v", err)
		return false
	}

	cfg := map[string]interface{}{
		"inbounds": []map[string]interface{}{
			{"port": 0, "protocol": "socks", "listen": "127.0.0.1", "settings": map[string]interface{}{"udp": false}},
		},
		"outbounds": []map[string]interface{}{
			{"protocol": "freedom", "tag": "proxy"},
		},
		"routing": map[string]interface{}{
			"domainStrategy": "IPIfNonMatch",
			"rules": []map[string]interface{}{
				{"type": "field", "ip": []string{"geoip:private"}, "outboundTag": "direct"},
			},
		},
	}
	raw, _ := json.Marshal(cfg)

	ctx, cancel := context.WithTimeout(context.Background(), verifyTimeout)
	defer cancel()

	cmd := exec.CommandContext(ctx, xrayBin, "run")
	cmd.Stdin = bytes.NewReader(raw)
	cmd.Stdout = nil
	cmd.Stderr = nil
	cmd.Env = append(os.Environ(), "XRAY_LOCATION_ASSET="+filepath.Dir(path))

	if err := cmd.Run(); err != nil {
		if ctx.Err() != nil {
			// Verification timed out — inconclusive, NOT a pass. A slow-to-fail
			// malformed file shouldn't slip through because of a 5s budget.
			logger.Warnf("geo: xray verification timed out after %s — treating as failure", verifyTimeout)
			return false
		}
		logger.Warnf("geo: xray verification failed: %v", err)
		return false
	}
	return true
}
