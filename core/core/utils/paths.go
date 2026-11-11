package utils

import (
	"fmt"
	"net"
	"os"
	osuser "os/user"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"time"
)

// ConfigDir returns the XDG config directory for WhoisThat.
//
// Resolution mirrors the Rust client (which uses the `directories` crate):
// $XDG_CONFIG_HOME/whoisthat when XDG_CONFIG_HOME is an absolute path,
// otherwise ~/.config/whoisthat. Keeping both sides on the same algorithm is
// essential: the TUI and the daemon must read the same config.toml/config.json.
func ConfigDir() string {
	if dir := os.Getenv("XDG_CONFIG_HOME"); dir != "" && filepath.IsAbs(dir) {
		return filepath.Join(dir, "whoisthat")
	}
	home, err := GetHomeDir()
	if err != nil || home == "" {
		home = os.Getenv("HOME")
	}
	if home == "" {
		home = "/tmp"
	}
	return filepath.Join(home, ".config", "whoisthat")
}

// DataDir returns the XDG data directory for WhoisThat.
func DataDir() string {
	if dir := os.Getenv("XDG_DATA_HOME"); dir != "" && filepath.IsAbs(dir) {
		return filepath.Join(dir, "whoisthat")
	}
	home, err := GetHomeDir()
	if err != nil || home == "" {
		home = os.Getenv("HOME")
	}
	if home == "" {
		home = "/tmp"
	}
	return filepath.Join(home, ".local", "share", "whoisthat")
}

// RuntimeBase returns the XDG runtime base directory (the parent of the
// whoisthat directory). Mirrors the Rust client's config::runtime_base():
// $XDG_RUNTIME_DIR/whoisthat is derived from it on both sides.
func RuntimeBase() string {
	uid := runtimeUid()
	canonical := filepath.Join("/run/whoisthat", strconv.Itoa(uid))
	// The init backend has no fallback: the boot wrapper and the client must
	// agree on one socket path.
	if initBackendSelected() {
		return canonical
	}
	if validPrivateDir(canonical, uid) {
		return canonical
	}
	if dir := os.Getenv("XDG_RUNTIME_DIR"); dir != "" && filepath.IsAbs(dir) {
		return dir
	}
	return filepath.Join("/tmp", fmt.Sprintf("whoisthat-%d", uid))
}

// RuntimeDir returns the XDG runtime directory for WhoisThat, used for the
// IPC socket, token and ephemeral state.
func RuntimeDir() string {
	return filepath.Join(RuntimeBase(), "whoisthat")
}

func runtimeUid() int {
	if uid := RealUserUid(); uid > 0 {
		return uid
	}
	// The environment is broken; fall back to the process uid so the daemon
	// still starts, but never silently keep uid 0 from a sudo shell.
	return os.Getuid()
}

func initBackendSelected() bool {
	data, err := os.ReadFile(filepath.Join(ConfigDir(), "autostart-backend"))
	return err == nil && strings.TrimSpace(string(data)) == "init"
}

func validPrivateDir(path string, uid int) bool {
	info, err := os.Lstat(path)
	if err != nil || info.Mode()&os.ModeSymlink != 0 || !info.IsDir() {
		return false
	}
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !ok || int(stat.Uid) != uid {
		return false
	}
	return info.Mode().Perm() == 0700
}

// SocketIsLive reports whether something is accepting connections on the unix
// socket at path. Used to avoid stealing the socket of a running core and to
// skip the orphaned-firewall reconcile while another core owns the rules.
func SocketIsLive(path string) bool {
	conn, err := net.DialTimeout("unix", path, 300*time.Millisecond)
	if err != nil {
		return false
	}
	conn.Close()
	return true
}

func GetXrayBin() (string, error) {
	return GetBinPath("xray")
}

func GetHomeDir() (string, error) {
	uid := os.Getuid()
	if uid == 0 {
		uidStr := os.Getenv("SUDO_UID")
		if uidStr == "" {
			uidStr = os.Getenv("PKEXEC_UID")
		}
		real_uid, err := strconv.Atoi(uidStr)
		if err != nil {
			return "", fmt.Errorf("failed to get user id outside of sudo/pkexec %w", err)
		}
		uid = real_uid
	}

	user, err := osuser.LookupId(strconv.Itoa(uid))
	if err != nil {
		// Don't Fatal: a missing/odd UID mapping shouldn't kill a running
		// daemon mid-session. Surface the error to the caller.
		return "", fmt.Errorf("failed to get user from uid %d: %w", uid, err)
	}
	return user.HomeDir, nil
}

func RemoveDuplicates(input []string) []string {
	seen := make(map[string]struct{})
	result := []string{}

	for _, v := range input {
		if _, ok := seen[v]; !ok {
			seen[v] = struct{}{}
			result = append(result, v)
		}
	}
	return result
}
