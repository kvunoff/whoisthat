package utils

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"

	"golang.org/x/sys/unix"
)

// Authorization is a root-owned allowlist: one empty file per uid under
// coreAuthDir, and one host-wide lock so only a single core owns the global
// netns state at a time. Both are created by scripts/authorize-core.
var (
	coreAuthDir  = "/etc/whoisthat/authorized.d"
	coreRunLock  = "/var/lib/whoisthat/run.lock"
	procRoot     = "/proc"
	authFileUid  = 0
	runLockUid   = 0
	netAdminFunc = capNetAdmin
)

const authorizeHint = "sudo /usr/lib/whoisthat/authorize-core --add $USER"

// HasNetAdmin reports whether CAP_NET_ADMIN is in the effective set.
func HasNetAdmin() bool { return netAdminFunc() }

func capNetAdmin() bool {
	var hdr unix.CapUserHeader
	hdr.Version = unix.LINUX_CAPABILITY_VERSION_3
	var data [2]unix.CapUserData
	if err := unix.Capget(&hdr, &data[0]); err != nil {
		return false
	}
	cap := uintptr(unix.CAP_NET_ADMIN)
	return data[cap/32].Effective&(1<<(cap%32)) != 0
}

// CheckAuthorization fails when this process holds CAP_NET_ADMIN but its uid is
// not present in the root-owned allowlist. Processes without the capability
// cannot touch global network state, so proxy mode and dev builds need no
// authorization.
func CheckAuthorization() error {
	if !HasNetAdmin() {
		return nil
	}
	uid := os.Getuid()
	path := filepath.Join(coreAuthDir, strconv.Itoa(uid))
	f, err := os.OpenFile(path, os.O_RDONLY|syscall.O_NOFOLLOW|syscall.O_NONBLOCK, 0)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return fmt.Errorf("uid %d is not authorized to run whoisthat-core (no %s); run: %s", uid, path, authorizeHint)
		}
		return fmt.Errorf("cannot verify core authorization at %s: %w", path, err)
	}
	defer f.Close()

	st, err := f.Stat()
	if err != nil {
		return fmt.Errorf("cannot stat core authorization %s: %w", path, err)
	}
	return checkAuthFile(path, st)
}

func checkAuthFile(path string, st os.FileInfo) error {
	if !st.Mode().IsRegular() {
		return fmt.Errorf("core authorization %s is not a regular file; refusing to start", path)
	}
	sys, ok := st.Sys().(*syscall.Stat_t)
	if !ok {
		return fmt.Errorf("cannot stat core authorization %s; refusing to start", path)
	}
	if int(sys.Uid) != authFileUid {
		return fmt.Errorf("core authorization %s must be owned by uid %d (found %d); refusing to start", path, authFileUid, sys.Uid)
	}
	if st.Mode().Perm()&0o022 != 0 {
		return fmt.Errorf("core authorization %s must not be writable by group or others; refusing to start", path)
	}
	return nil
}

// AcquireGlobalLock takes the host-wide lock that keeps exactly one core alive.
// When the lock file is missing and this process has no capabilities (dev or
// proxy build), it returns (nil, nil): such a core cannot change global state
// anyway. With capabilities present a missing lock means the install is broken,
// so it fails closed.
func AcquireGlobalLock() (*os.File, error) {
	_, statErr := os.Stat(coreRunLock)
	if statErr != nil {
		if errors.Is(statErr, os.ErrNotExist) {
			if !HasNetAdmin() {
				return nil, nil
			}
			return nil, fmt.Errorf("global lock %s is missing; run: %s", coreRunLock, authorizeHint)
		}
		return nil, fmt.Errorf("cannot check global lock %s: %w", coreRunLock, statErr)
	}

	f, err := os.OpenFile(coreRunLock, os.O_RDONLY, 0)
	if err != nil {
		return nil, fmt.Errorf("cannot open global lock %s: %w", coreRunLock, err)
	}
	st, err := f.Stat()
	if err != nil {
		f.Close()
		return nil, fmt.Errorf("cannot stat global lock %s: %w", coreRunLock, err)
	}
	if err := checkLockFile(coreRunLock, st); err != nil {
		f.Close()
		return nil, err
	}
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		f.Close()
		if errors.Is(err, syscall.EWOULDBLOCK) {
			return nil, errors.New("another whoisthat core is already running; stop it first")
		}
		return nil, fmt.Errorf("cannot lock %s: %w", coreRunLock, err)
	}
	return f, nil
}

func checkLockFile(path string, st os.FileInfo) error {
	if !st.Mode().IsRegular() {
		return fmt.Errorf("global lock %s is not a regular file; refusing to start", path)
	}
	sys, ok := st.Sys().(*syscall.Stat_t)
	if !ok {
		return fmt.Errorf("cannot stat global lock %s; refusing to start", path)
	}
	if int(sys.Uid) != runLockUid {
		return fmt.Errorf("global lock %s must be owned by uid %d (found %d); refusing to start", path, runLockUid, sys.Uid)
	}
	if st.Mode().Perm()&0o022 != 0 {
		return fmt.Errorf("global lock %s must not be writable by group or others; refusing to start", path)
	}
	return nil
}

// OtherCoreRunning finds a whoisthat-core process other than this one. It
// exists only for the upgrade window: cores from older builds do not take the
// global lock, so without this check they could be reconciled away by a new
// core of a different uid. Best effort by design.
func OtherCoreRunning() (uid, pid int) {
	entries, err := os.ReadDir(procRoot)
	if err != nil {
		return 0, 0
	}
	self := os.Getpid()
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		candidate, err := strconv.Atoi(entry.Name())
		if err != nil || candidate == self {
			continue
		}
		dir := filepath.Join(procRoot, entry.Name())
		comm, err := os.ReadFile(filepath.Join(dir, "comm"))
		if err != nil || strings.TrimSpace(string(comm)) != "whoisthat-core" {
			continue
		}
		status, err := os.ReadFile(filepath.Join(dir, "status"))
		if err != nil {
			continue
		}
		state, owner := parseProcStatus(string(status))
		if state == "Z" {
			continue
		}
		return owner, candidate
	}
	return 0, 0
}

func parseProcStatus(status string) (state string, uid int) {
	for _, line := range strings.Split(status, "\n") {
		switch {
		case strings.HasPrefix(line, "State:"):
			fields := strings.Fields(strings.TrimPrefix(line, "State:"))
			if len(fields) > 0 {
				state = fields[0]
			}
		case strings.HasPrefix(line, "Uid:"):
			fields := strings.Fields(strings.TrimPrefix(line, "Uid:"))
			if len(fields) > 0 {
				uid, _ = strconv.Atoi(fields[0])
			}
		}
	}
	return state, uid
}
