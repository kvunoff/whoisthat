package utils

import (
	"bytes"
	"log"
	"os"
	"os/exec"
	"syscall"

	"golang.org/x/sys/unix"
)

// XrayAmbientCaps returns the network capabilities available to the daemon.
func XrayAmbientCaps() []uintptr {
	hdr := unix.CapUserHeader{Version: unix.LINUX_CAPABILITY_VERSION_3}
	var data [2]unix.CapUserData
	if unix.Capget(&hdr, &data[0]) != nil {
		return nil
	}
	var caps []uintptr
	for _, cap := range []uintptr{unix.CAP_NET_ADMIN, unix.CAP_NET_RAW} {
		if data[cap/32].Effective&(1<<(cap%32)) != 0 {
			caps = append(caps, cap)
		}
	}
	return caps
}

func RaiseAmbientCaps() {
	var hdr unix.CapUserHeader
	hdr.Version = unix.LINUX_CAPABILITY_VERSION_3
	hdr.Pid = 0
	var data [2]unix.CapUserData
	if err := unix.Capget(&hdr, &data[0]); err != nil {
		log.Printf("[WARN] RaiseAmbientCaps: capget failed: %v", err)
		return
	}

	caps := XrayAmbientCaps()
	if len(caps) == 0 {
		return
	}
	for _, cap := range caps {
		data[cap/32].Inheritable |= 1 << (cap % 32)
	}

	if err := unix.Capset(&hdr, &data[0]); err != nil {
		log.Printf("[WARN] RaiseAmbientCaps: capset failed: %v", err)
		return
	}

	for _, cap := range caps {
		if err := unix.Prctl(unix.PR_CAP_AMBIENT, unix.PR_CAP_AMBIENT_RAISE, cap, 0, 0); err != nil {
			log.Printf("[WARN] RaiseAmbientCaps: prctl PR_CAP_AMBIENT_RAISE for cap %d failed: %v", cap, err)
		}
	}
}

func CanTun() bool {
	if os.Getuid() == 0 {
		return true
	}

	shell := "/bin/sh"
	if _, err := os.Stat(shell); err != nil {
		shell = "sh"
	}
	check := exec.Command(shell)
	check.Env = []string{"PATH=/usr/sbin:/usr/bin:/sbin:/bin", "LC_ALL=C"}
	check.SysProcAttr = &syscall.SysProcAttr{
		AmbientCaps: []uintptr{unix.CAP_NET_ADMIN},
	}
	stdin, err := check.StdinPipe()
	if err != nil {
		return false
	}

	script := `
TUN_NAME="wt-capcheck"
# Self-clean any stale probe device from a previous failed run
ip tuntap del mode tun dev "$TUN_NAME" 2>/dev/null || true
# Ensure cleanup even if something fails mid-script
trap 'ip tuntap del mode tun dev "$TUN_NAME" 2>/dev/null' EXIT
ip tuntap add mode tun dev "$TUN_NAME" || exit 1
ip tuntap del mode tun dev "$TUN_NAME" 2>/dev/null
trap - EXIT
`
	go func() {
		defer stdin.Close()
		stdin.Write([]byte(script))
	}()

	stderr := bytes.Buffer{}
	check.Stderr = &stderr
	return check.Run() == nil
}
