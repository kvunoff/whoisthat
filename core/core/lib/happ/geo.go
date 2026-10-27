package happ

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"
	"whoisthat-core/db"
	"whoisthat-core/lib/geo"
	"whoisthat-core/structs"
)

const (
	MinGeoFileSize     = 1024              // 1 KB min (allows compact custom geo assets)
	MaxGeoFileSize     = 100 * 1024 * 1024 // 100 MB max
	GeoDownloadTimeout = 3 * time.Minute   // 3 minutes per file
	WeeklyUpdateGate   = 7 * 24 * 3600     // 7 days in seconds
)

// EnsureProfileGeo downloads or updates custom Geoipurl and Geositeurl assets for a Happ profile.
// It downloads into a versioned temporary directory, validates files, atomically moves them,
// and updates the profile's GeoDir and GeoUpdatedAt in the database.
func EnsureProfileGeo(ctx context.Context, database *db.DB, profile *structs.RoutingProfile) error {
	if profile == nil {
		return errors.New("nil profile")
	}

	var hp HappProfile
	if err := json.Unmarshal(profile.RawJSON, &hp); err != nil {
		return fmt.Errorf("failed to unmarshal profile raw json: %w", err)
	}

	geoipURL := strings.TrimSpace(hp.Geoipurl)
	geositeURL := strings.TrimSpace(hp.Geositeurl)

	// If no custom geo URLs are specified, profile uses global assets
	if geoipURL == "" && geositeURL == "" {
		profile.GeoDir = ""
		profile.State = "ready"
		profile.LastError = ""
		return database.SaveRoutingProfile(*profile)
	}

	now := time.Now().Unix()

	// Check weekly gate if files already exist
	if profile.GeoUpdatedAt > 0 && (now-profile.GeoUpdatedAt) < WeeklyUpdateGate && profile.GeoDir != "" {
		absDir := filepath.Join(database.Path, profile.GeoDir)
		_, err1 := os.Stat(filepath.Join(absDir, "geoip.dat"))
		_, err2 := os.Stat(filepath.Join(absDir, "geosite.dat"))
		if err1 == nil && err2 == nil {
			profile.State = "ready"
			profile.LastError = ""
			return database.SaveRoutingProfile(*profile)
		}
	}

	// Prepare directories
	geoParentDir := database.GetRoutingGeoDir(profile.GroupId, profile.Id)
	versionDir := strconv.FormatInt(now, 10)
	destDirRel := filepath.Join("groups", strconv.Itoa(profile.GroupId), "routing", "geo", profile.Id, versionDir)
	destDirAbs := filepath.Join(database.Path, destDirRel)
	tmpDirAbs := destDirAbs + ".tmp"

	if err := os.MkdirAll(tmpDirAbs, 0700); err != nil {
		return fmt.Errorf("failed to create tmp geo directory: %w", err)
	}

	var downloadErr error
	defer func() {
		if downloadErr != nil {
			_ = os.RemoveAll(tmpDirAbs)
			profile.State = "error"
			profile.LastError = sanitizeError(downloadErr)
			_ = database.SaveRoutingProfile(*profile)
		}
	}()

	// 1. Download or copy geoip.dat
	destGeoip := filepath.Join(tmpDirAbs, "geoip.dat")
	if geoipURL != "" {
		if err := downloadAsset(ctx, geoipURL, destGeoip); err != nil {
			downloadErr = fmt.Errorf("download geoip from %s: %w", maskURL(geoipURL), err)
			return downloadErr
		}
	} else {
		if err := copyGlobalAsset("geoip.dat", destGeoip); err != nil {
			downloadErr = fmt.Errorf("copy global geoip.dat: %w", err)
			return downloadErr
		}
	}

	// 2. Download or copy geosite.dat
	destGeosite := filepath.Join(tmpDirAbs, "geosite.dat")
	if geositeURL != "" {
		if err := downloadAsset(ctx, geositeURL, destGeosite); err != nil {
			downloadErr = fmt.Errorf("download geosite from %s: %w", maskURL(geositeURL), err)
			return downloadErr
		}
	} else {
		if err := copyGlobalAsset("geosite.dat", destGeosite); err != nil {
			downloadErr = fmt.Errorf("copy global geosite.dat: %w", err)
			return downloadErr
		}
	}

	// Check if group and profile still exist before committing
	latest, err := database.LoadRoutingProfile(profile.GroupId, profile.Id)
	if err != nil {
		_ = os.RemoveAll(tmpDirAbs)
		return nil // Profile was deleted while downloading
	}

	// Atomically move tmp dir to final destination
	if err := os.Rename(tmpDirAbs, destDirAbs); err != nil {
		downloadErr = fmt.Errorf("failed to commit geo dir: %w", err)
		return downloadErr
	}

	latest.GeoDir = destDirRel
	latest.GeoUpdatedAt = now
	latest.State = "ready"
	latest.LastError = ""
	*profile = latest

	if err := database.SaveRoutingProfile(latest); err != nil {
		return fmt.Errorf("failed to save profile after geo update: %w", err)
	}

	// Prune older geo versions (keep current and 1 previous)
	pruneOldGeoDirs(geoParentDir, versionDir)

	return nil
}

func downloadAsset(ctx context.Context, downloadURL string, destPath string) error {
	u, err := url.Parse(downloadURL)
	if err != nil || u.Scheme != "https" {
		return fmt.Errorf("insecure or invalid URL (must be https): %s", maskURL(downloadURL))
	}

	req, err := http.NewRequestWithContext(ctx, "GET", downloadURL, nil)
	if err != nil {
		return err
	}

	client := &http.Client{
		Timeout: GeoDownloadTimeout,
		CheckRedirect: func(req *http.Request, via []*http.Request) error {
			if len(via) >= 10 {
				return errors.New("too many redirects")
			}
			if req.URL.Scheme != "https" {
				return errors.New("redirect to non-https disallowed")
			}
			return nil
		},
	}

	resp, err := client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("HTTP status %d", resp.StatusCode)
	}

	out, err := os.OpenFile(destPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0600)
	if err != nil {
		return err
	}
	defer out.Close()

	// Limit reader to MaxGeoFileSize + 1 byte to detect oversized files
	limitedReader := io.LimitReader(resp.Body, MaxGeoFileSize+1)
	written, err := io.Copy(out, limitedReader)
	if err != nil {
		return err
	}

	_ = out.Sync()
	out.Close()

	if written > MaxGeoFileSize {
		_ = os.Remove(destPath)
		return fmt.Errorf("geo asset exceeds maximum size of 100 MB")
	}
	if written < MinGeoFileSize {
		_ = os.Remove(destPath)
		return fmt.Errorf("geo asset smaller than minimum size of 1 KB (%d bytes)", written)
	}

	// Validate that downloaded file is binary and not an HTML/JSON error response
	buf := make([]byte, 16)
	f, err := os.Open(destPath)
	if err == nil {
		n, _ := io.ReadFull(f, buf)
		f.Close()
		head := strings.ToLower(string(buf[:n]))
		if strings.HasPrefix(head, "<!doctype") || strings.HasPrefix(head, "<html") || strings.HasPrefix(head, "{\n") || strings.HasPrefix(head, "{\"") {
			_ = os.Remove(destPath)
			return fmt.Errorf("downloaded file is HTML/JSON, not a binary .dat asset")
		}
	}

	return nil
}

func copyGlobalAsset(filename string, destPath string) error {
	globalDir := geo.AssetDir()
	if globalDir == "" {
		return fmt.Errorf("global asset dir not configured")
	}

	srcPath := filepath.Join(globalDir, filename)
	src, err := os.Open(srcPath)
	if err != nil {
		return err
	}
	defer src.Close()

	dst, err := os.OpenFile(destPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0600)
	if err != nil {
		return err
	}
	defer dst.Close()

	_, err = io.Copy(dst, src)
	return err
}

func pruneOldGeoDirs(parentDir string, currentVersion string) {
	entries, err := os.ReadDir(parentDir)
	if err != nil {
		return
	}

	var versions []string
	for _, entry := range entries {
		if entry.IsDir() && entry.Name() != currentVersion && !strings.HasSuffix(entry.Name(), ".tmp") {
			versions = append(versions, entry.Name())
		}
	}

	sort.Strings(versions)
	// Keep 1 previous version for safety, delete older
	if len(versions) > 1 {
		toDelete := versions[:len(versions)-1]
		for _, v := range toDelete {
			_ = os.RemoveAll(filepath.Join(parentDir, v))
		}
	}
}

func maskURL(raw string) string {
	u, err := url.Parse(raw)
	if err != nil {
		return raw
	}
	if u.RawQuery != "" {
		u.RawQuery = "..."
	}
	return u.String()
}

func sanitizeError(err error) string {
	if err == nil {
		return ""
	}
	s := err.Error()
	// Strip sensitive query params from error string if present
	if idx := strings.Index(s, "?"); idx != -1 {
		endIdx := strings.IndexAny(s[idx:], " \t\r\n:\"'")
		if endIdx != -1 {
			s = s[:idx] + "?..." + s[idx+endIdx:]
		} else {
			s = s[:idx] + "?..."
		}
	}
	return s
}
