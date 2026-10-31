package geo

import (
	"io"
	"os"
	"path/filepath"

	"whoisthat-core/lib/logger"
)

const (
	globalGeoIPName   = "geoip-global.dat"
	globalGeoSiteName = "geosite-global.dat"
)

// EnsureGlobalCopies exposes the whoisthat-wide geo assets inside a
// provider-owned asset directory. Xray reads a single XRAY_LOCATION_ASSET
// directory, so provider rules (geosite:win-spy and friends) and manual rules
// (geoip:ru, geosite:category-ru) can only coexist when both datasets live
// side by side. Symlinks are preferred; a copy is used when symlinks are not
// permitted.
func EnsureGlobalCopies(dir string) bool {
	if dir == "" {
		return false
	}
	srcDir := AssetDir()
	if srcDir == "" {
		return false
	}
	if err := os.MkdirAll(dir, 0700); err != nil {
		logger.Warnf("geo: cannot create asset dir %s: %v", dir, err)
		return false
	}

	ok := true
	for _, asset := range [][2]string{
		{"geoip.dat", globalGeoIPName},
		{"geosite.dat", globalGeoSiteName},
	} {
		if !ensureGlobalCopy(filepath.Join(srcDir, asset[0]), filepath.Join(dir, asset[1])) {
			ok = false
		}
	}
	return ok
}

func ensureGlobalCopy(src, dst string) bool {
	info, err := os.Stat(src)
	if err != nil || info.Size() == 0 {
		return false
	}
	if existing, err := os.Stat(dst); err == nil && existing.Size() == info.Size() {
		return true
	}
	_ = os.Remove(dst)
	if err := os.Symlink(src, dst); err == nil {
		return true
	}
	if err := copyAsset(src, dst); err != nil {
		logger.Warnf("geo: failed to expose global asset %s in %s: %v",
			filepath.Base(src), filepath.Dir(dst), err)
		return false
	}
	return true
}

func copyAsset(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()

	out, err := os.OpenFile(dst, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0600)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		_ = os.Remove(dst)
		return err
	}
	if err := out.Close(); err != nil {
		_ = os.Remove(dst)
		return err
	}
	return nil
}
