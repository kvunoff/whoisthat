package xraymgr

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"syscall"

	"whoisthat-core/utils"
)

const digestName = "xray.sha256"

// trustedSystemOwner is 0 in production. Tests run unprivileged and override
// it to their own uid to exercise the adoption path.
var trustedSystemOwner = 0

// fileSHA256 returns the hex SHA-256 of path.
func fileSHA256(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()
	hasher := sha256.New()
	if _, err := io.Copy(hasher, f); err != nil {
		return "", err
	}
	return hex.EncodeToString(hasher.Sum(nil)), nil
}

// writeDigest records the digest of bin next to it. It is written before the
// runtime directory is moved into place so a crash cannot leave a binary
// without its checksum.
func writeDigest(dir, bin string) error {
	sum, err := fileSHA256(bin)
	if err != nil {
		return err
	}
	return utils.AtomicWriteFile(filepath.Join(dir, digestName), []byte(sum+"\n"), 0600)
}

// verifyDigest rejects a managed binary that changed after installation. A
// missing digest means an install from before this check: treat it as
// untrusted and let the caller download a fresh copy.
func verifyDigest(dir, bin string) error {
	raw, err := os.ReadFile(filepath.Join(dir, digestName))
	if err != nil {
		return fmt.Errorf("missing %s: %w", digestName, err)
	}
	want := strings.TrimSpace(string(raw))
	got, err := fileSHA256(bin)
	if err != nil {
		return err
	}
	if want == "" || want != got {
		return fmt.Errorf("binary digest mismatch")
	}
	return nil
}

// validateSystemBinary accepts a root-owned regular file that is not writable
// by group or others. Symlinks are resolved first so packaging alternatives
// still work.
func validateSystemBinary(path string) error {
	resolved, err := filepath.EvalSymlinks(path)
	if err != nil {
		return err
	}
	info, err := os.Stat(resolved)
	if err != nil {
		return err
	}
	if !info.Mode().IsRegular() {
		return fmt.Errorf("%s is not a regular file", resolved)
	}
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !ok || int(stat.Uid) != trustedSystemOwner {
		return fmt.Errorf("%s must be owned by uid %d", resolved, trustedSystemOwner)
	}
	if info.Mode().Perm()&0o022 != 0 {
		return fmt.Errorf("%s must not be group/other writable", resolved)
	}
	return nil
}
