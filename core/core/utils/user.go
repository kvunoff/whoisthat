package utils

import (
	"os"
	"os/exec"
	"os/user"
	"strconv"
	"strings"
	"sync"
)

func RealUserUid() int {
	if s := os.Getenv("SUDO_UID"); s != "" {
		if uid, err := strconv.Atoi(s); err == nil && uid > 0 {
			return uid
		}
	}
	if s := os.Getenv("PKEXEC_UID"); s != "" {
		if uid, err := strconv.Atoi(s); err == nil && uid > 0 {
			return uid
		}
	}
	if out, err := exec.Command("logname").Output(); err == nil {
		username := strings.TrimSpace(string(out))
		if u, err := user.Lookup(username); err == nil {
			if uid, err := strconv.Atoi(u.Uid); err == nil && uid > 0 {
				return uid
			}
		}
	}
	if s := os.Getenv("SUDO_USER"); s != "" {
		if u, err := user.Lookup(s); err == nil {
			if uid, err := strconv.Atoi(u.Uid); err == nil && uid > 0 {
				return uid
			}
		}
	}
	return 0
}

func RealUserGid() int {
	if s := os.Getenv("SUDO_GID"); s != "" {
		if gid, err := strconv.Atoi(s); err == nil && gid > 0 {
			return gid
		}
	}
	if uid := RealUserUid(); uid > 0 {
		if u, err := user.LookupId(strconv.Itoa(uid)); err == nil {
			if gid, err := strconv.Atoi(u.Gid); err == nil && gid > 0 {
				return gid
			}
		}
	}
	return 0
}

var (
	dedicatedUid  int
	dedicatedOnce sync.Once
)

func DedicatedUid() int {
	dedicatedOnce.Do(func() {
		if os.Getuid() != 0 {
			return
		}
		for uid := 61000; uid < 62000; uid++ {
			if _, err := user.LookupId(strconv.Itoa(uid)); err != nil {
				dedicatedUid = uid
				return
			}
		}
	})
	return dedicatedUid
}

func DedicatedGid() int {
	if uid := DedicatedUid(); uid > 0 {
		return uid
	}
	return 0
}
