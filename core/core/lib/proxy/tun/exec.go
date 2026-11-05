package tunmode

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
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
		if _, err := exec.LookPath("nft"); err == nil {
			fwBackend = firewallNftables
		} else {
			fwBackend = firewallIptables
		}
	})
	return fwBackend
}

func runScriptWithSh(script string) (string, error) {
	cmd := exec.Command("sh")
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
	cmd := exec.Command(name, args...)
	cmd.Env = append(os.Environ(), "LC_ALL=C")
	cmd.SysProcAttr = &syscall.SysProcAttr{
		AmbientCaps: []uintptr{unix.CAP_NET_ADMIN, unix.CAP_NET_RAW},
	}
	out, err := cmd.CombinedOutput()
	return string(out), err
}
