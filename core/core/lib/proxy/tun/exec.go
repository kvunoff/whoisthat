package tunmode

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"syscall"

	"golang.org/x/sys/unix"
)

type firewallBackend int

const (
	firewallIptables firewallBackend = iota
	firewallNftables
)

var (
	fwBackend firewallBackend
	fwOnce    sync.Once
	// privilegedOwner is 0 in production. Tests run unprivileged and override
	// it to their own uid so fake tools in a temp PATH are accepted.
	privilegedOwner = 0
)

func probeFirewall() firewallBackend {
	fwOnce.Do(func() {
		// Allow forcing a backend. Useful when the nftables `type route hook
		// output` reroute-on-mark misbehaves on a given kernel: set
		// WHOISTHAT_FW_BACKEND=iptables to drive the (potentially legacy)
		// iptables mangle path instead. For a genuinely different reroute
		// implementation point the PATH at iptables-legacy.
		switch strings.ToLower(strings.TrimSpace(os.Getenv("WHOISTHAT_FW_BACKEND"))) {
		case "iptables", "legacy":
			fwBackend = firewallIptables
			return
		case "nft", "nftables":
			fwBackend = firewallNftables
			return
		}
		if _, err := resolvePrivileged("nft"); err == nil {
			fwBackend = firewallNftables
		} else {
			fwBackend = firewallIptables
		}
	})
	return fwBackend
}

// resolvePrivileged finds name via PATH but only accepts a root-owned regular
// file that is not writable by group or others. This blocks a user-writable
// binary from running with ambient caps while keeping NixOS store paths and
// iptables-legacy alternatives working.
func resolvePrivileged(name string) (string, error) {
	path, err := exec.LookPath(name)
	if err != nil {
		return "", err
	}
	return validatePrivilegedPath(path)
}

func validatePrivilegedPath(path string) (string, error) {
	resolved, err := filepath.EvalSymlinks(path)
	if err != nil {
		return "", err
	}
	info, err := os.Stat(resolved)
	if err != nil {
		return "", err
	}
	if !info.Mode().IsRegular() {
		return "", fmt.Errorf("%s is not a regular file", resolved)
	}
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !ok || int(stat.Uid) != privilegedOwner {
		return "", fmt.Errorf("%s must be owned by uid %d", resolved, privilegedOwner)
	}
	if info.Mode().Perm()&0o022 != 0 {
		return "", fmt.Errorf("%s must not be group/other writable", resolved)
	}
	return resolved, nil
}

func shellPath() (string, error) {
	for _, path := range []string{"/bin/sh", "/usr/bin/sh"} {
		if _, err := os.Stat(path); err == nil {
			return validatePrivilegedPath(path)
		}
	}
	return "", errors.New("no /bin/sh on this system")
}

// scriptPath builds PATH from the directories of the tools the shell scripts
// call, after validating each tool. A user-writable PATH entry therefore never
// reaches the privileged shell.
func scriptPath() string {
	dirs := []string{}
	for _, name := range []string{"ip", "nft", "iptables", "ip6tables", "resolvectl", "sysctl"} {
		path, err := resolvePrivileged(name)
		if err != nil {
			continue
		}
		dir := filepath.Dir(path)
		seen := false
		for _, existing := range dirs {
			if existing == dir {
				seen = true
				break
			}
		}
		if !seen {
			dirs = append(dirs, dir)
		}
	}
	dirs = append(dirs, "/usr/bin", "/bin")
	return strings.Join(dirs, ":")
}

func runScriptWithSh(script string) (string, error) {
	shell, err := shellPath()
	if err != nil {
		return "", err
	}
	cmd := exec.Command(shell)
	cmd.Env = []string{"PATH=" + scriptPath(), "LC_ALL=C"}
	cmd.SysProcAttr = &syscall.SysProcAttr{
		AmbientCaps: []uintptr{unix.CAP_NET_ADMIN, unix.CAP_NET_RAW},
	}
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return "", fmt.Errorf("failed to get stdin of sh %w", err)
	}
	_, err = stdin.Write([]byte(script))

	if err != nil {
		return "", fmt.Errorf("failed to write to stdin of sh %w", err)
	}
	err = stdin.Close()
	if err != nil {
		return "", fmt.Errorf("failed to close stdin of sh %w", err)
	}
	stderr := bytes.Buffer{}
	cmd.Stderr = &stderr
	output, err := cmd.Output()

	if err != nil {
		return "", fmt.Errorf("%w: %s", err, string(stderr.String()))
	}
	return string(output), nil
}

// runCommandWithCaps runs a privileged inspection command and returns its
// combined output, even on failure. Callers that need to tell "not there" from
// "could not look" parse the output/exit code themselves.
func runCommandWithCaps(name string, args ...string) (string, error) {
	resolved, err := resolvePrivileged(name)
	if err != nil {
		return "", err
	}
	cmd := exec.Command(resolved, args...)
	cmd.Env = []string{"PATH=" + scriptPath(), "LC_ALL=C"}
	cmd.SysProcAttr = &syscall.SysProcAttr{
		AmbientCaps: []uintptr{unix.CAP_NET_ADMIN, unix.CAP_NET_RAW},
	}
	out, err := cmd.CombinedOutput()
	return string(out), err
}
