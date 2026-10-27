package utils

import (
	"fmt"
	"os"
	osuser "os/user"
	"path/filepath"
	"strconv"
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

// RuntimeDir returns the XDG runtime directory for WhoisThat, used for the
// IPC socket, token and ephemeral state.
func RuntimeDir() string {
	if dir := os.Getenv("XDG_RUNTIME_DIR"); dir != "" && filepath.IsAbs(dir) {
		return filepath.Join(dir, "whoisthat")
	}
	return filepath.Join("/tmp", fmt.Sprintf("whoisthat-%d", os.Getuid()))
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
