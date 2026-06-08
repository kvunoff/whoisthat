package utils

import (
	"bytes"
	"log"
	"os"
	"os/exec"
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

func CanTun() bool {
	if os.Getuid() == 0 {
		return true
	}

	check := exec.Command("sh")
	check.SysProcAttr = &syscall.SysProcAttr{
		AmbientCaps: []uintptr{unix.CAP_NET_ADMIN},
	}
	stdin, err := check.StdinPipe()
	if err != nil {
		return false
	}

	script := `
TUN_NAME="wt-capcheck"
ip tuntap add mode tun dev "$TUN_NAME" || exit 1
ip tuntap del mode tun dev "$TUN_NAME" 2>/dev/null
`
	go func() {
		defer stdin.Close()
		stdin.Write([]byte(script))
	}()

	stderr := bytes.Buffer{}
	check.Stderr = &stderr
	return check.Run() == nil
}
