package utils

import (
	"bytes"
	"log"
	"os"
	"syscall"

	"golang.org/x/sys/unix"
)

func RaiseAmbientCaps() {
	var hdr unix.CapUserHeader
	hdr.Version = unix.LINUX_CAPABILITY_VERSION_3
	hdr.Pid = 0
	var data [2]unix.CapUserData
	if err := unix.Capget(&hdr, &data[0]); err != nil {
		log.Printf("[WARN] RaiseAmbientCaps: capget failed: %v", err)
		return
	}

	caps := []uintptr{unix.CAP_NET_ADMIN, unix.CAP_NET_RAW}
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

// HeldCapabilities returns the subset of caps present in both the effective and
// permitted sets. Callers must only request ambient capabilities for caps they
// actually hold: Go's forkExec raises each one with PR_CAP_AMBIENT_RAISE and
// fails exec with EPERM when the cap is missing — which is exactly what happens
// to an unprivileged test run or a plain-user CLI invocation.
func HeldCapabilities(caps ...uintptr) []uintptr {
	var hdr unix.CapUserHeader
	hdr.Version = unix.LINUX_CAPABILITY_VERSION_3
	var data [2]unix.CapUserData
	if err := unix.Capget(&hdr, &data[0]); err != nil {
		return nil
	}
	held := make([]uintptr, 0, len(caps))
	for _, c := range caps {
		bit := uint32(1) << (c % 32)
		if data[c/32].Effective&bit != 0 && data[c/32].Permitted&bit != 0 {
			held = append(held, c)
		}
	}
	return held
}

func CanTun() bool {
	if os.Getuid() == 0 {
		return true
	}

	check := SafeCommand("sh")
	if caps := HeldCapabilities(unix.CAP_NET_ADMIN); len(caps) > 0 {
		check.SysProcAttr = &syscall.SysProcAttr{AmbientCaps: caps}
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

// DropSetpcap removes CAP_SETPCAP from the bounding, permitted, effective and
// inheritable sets (F-01). It must run right after RaiseAmbientCaps: the
// ambient set survives the drop, so children keep net_admin/net_raw, but a
// compromised core can no longer rewrite capability sets or hand out more.
// Failures are logged, not fatal: the core is still usable, just less
// hardened, and the doctor reports the caps.
func DropSetpcap() {
	// The bounding-set drop itself needs CAP_SETPCAP in effect, so do it first.
	if err := unix.Prctl(unix.PR_CAPBSET_DROP, unix.CAP_SETPCAP, 0, 0, 0); err != nil {
		log.Printf("[WARN] DropSetpcap: PR_CAPBSET_DROP failed: %v", err)
	}

	var hdr unix.CapUserHeader
	hdr.Version = unix.LINUX_CAPABILITY_VERSION_3
	var data [2]unix.CapUserData
	if err := unix.Capget(&hdr, &data[0]); err != nil {
		log.Printf("[WARN] DropSetpcap: capget failed: %v", err)
		return
	}
	mask := ^uint32(1 << (unix.CAP_SETPCAP % 32))
	i := unix.CAP_SETPCAP / 32
	data[i].Effective &= mask
	data[i].Permitted &= mask
	data[i].Inheritable &= mask
	if err := unix.Capset(&hdr, &data[0]); err != nil {
		log.Printf("[WARN] DropSetpcap: capset failed: %v", err)
	}
}
