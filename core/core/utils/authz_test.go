package utils

import (
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

func setupAuth(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	old := coreAuthDir
	coreAuthDir = dir
	authFileUid = os.Getuid()
	netAdminFunc = func() bool { return true }
	t.Cleanup(func() {
		coreAuthDir = old
		authFileUid = 0
		netAdminFunc = capNetAdmin
	})
	return dir
}

func authPath(dir string) string {
	return filepath.Join(dir, strconv.Itoa(os.Getuid()))
}

func TestCheckAuthorizationWithoutCaps(t *testing.T) {
	coreAuthDir = t.TempDir()
	netAdminFunc = func() bool { return false }
	t.Cleanup(func() { netAdminFunc = capNetAdmin })

	if err := CheckAuthorization(); err != nil {
		t.Fatalf("capless process must not need authorization: %v", err)
	}
}

func TestCheckAuthorizationAllowlisted(t *testing.T) {
	dir := setupAuth(t)
	if err := os.WriteFile(authPath(dir), nil, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := CheckAuthorization(); err != nil {
		t.Fatalf("allowlisted uid rejected: %v", err)
	}
}

func TestCheckAuthorizationMissingFile(t *testing.T) {
	setupAuth(t)
	err := CheckAuthorization()
	if err == nil || !strings.Contains(err.Error(), "not authorized") {
		t.Fatalf("want not-authorized error, got %v", err)
	}
}

func TestCheckAuthorizationSymlink(t *testing.T) {
	dir := setupAuth(t)
	target := filepath.Join(dir, "target")
	if err := os.WriteFile(target, nil, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(target, authPath(dir)); err != nil {
		t.Fatal(err)
	}
	if err := CheckAuthorization(); err == nil {
		t.Fatal("symlink must be refused")
	}
}

func TestCheckAuthorizationWrongOwner(t *testing.T) {
	dir := setupAuth(t)
	if err := os.WriteFile(authPath(dir), nil, 0o644); err != nil {
		t.Fatal(err)
	}
	authFileUid = os.Getuid() + 1
	if err := CheckAuthorization(); err == nil || !strings.Contains(err.Error(), "owned by uid") {
		t.Fatalf("want owner error, got %v", err)
	}
}

func TestCheckAuthorizationGroupWritable(t *testing.T) {
	dir := setupAuth(t)
	path := authPath(dir)
	if err := os.WriteFile(path, nil, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(path, 0o664); err != nil {
		t.Fatal(err)
	}
	if err := CheckAuthorization(); err == nil || !strings.Contains(err.Error(), "writable") {
		t.Fatalf("want writable error, got %v", err)
	}
}

func TestCheckAuthorizationNotRegular(t *testing.T) {
	dir := setupAuth(t)
	if err := os.Mkdir(authPath(dir), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := CheckAuthorization(); err == nil || !strings.Contains(err.Error(), "regular file") {
		t.Fatalf("want regular-file error, got %v", err)
	}
}

func setupLock(t *testing.T) (string, func(bool)) {
	t.Helper()
	dir := t.TempDir()
	path := filepath.Join(dir, "run.lock")
	oldPath, oldUid := coreRunLock, runLockUid
	coreRunLock = path
	runLockUid = os.Getuid()
	t.Cleanup(func() {
		coreRunLock = oldPath
		runLockUid = oldUid
		netAdminFunc = capNetAdmin
	})
	return path, func(on bool) {
		netAdminFunc = func() bool { return on }
	}
}

func TestAcquireGlobalLockMissingWithoutCaps(t *testing.T) {
	_, setCaps := setupLock(t)
	setCaps(false)
	lock, err := AcquireGlobalLock()
	if err != nil || lock != nil {
		t.Fatalf("dev core without lock file: lock=%v err=%v", lock, err)
	}
}

func TestAcquireGlobalLockMissingWithCaps(t *testing.T) {
	_, setCaps := setupLock(t)
	setCaps(true)
	if _, err := AcquireGlobalLock(); err == nil {
		t.Fatal("capped core must refuse a missing global lock")
	}
}

func TestAcquireGlobalLockAndContention(t *testing.T) {
	path, setCaps := setupLock(t)
	setCaps(true)
	if err := os.WriteFile(path, nil, 0o644); err != nil {
		t.Fatal(err)
	}
	lock, err := AcquireGlobalLock()
	if err != nil {
		t.Fatalf("first lock failed: %v", err)
	}
	defer lock.Close()

	if _, err := AcquireGlobalLock(); err == nil || !strings.Contains(err.Error(), "already running") {
		t.Fatalf("second lock must fail with already-running, got %v", err)
	}
}

func TestAcquireGlobalLockWrongOwner(t *testing.T) {
	path, setCaps := setupLock(t)
	setCaps(true)
	if err := os.WriteFile(path, nil, 0o644); err != nil {
		t.Fatal(err)
	}
	runLockUid = os.Getuid() + 1
	if _, err := AcquireGlobalLock(); err == nil || !strings.Contains(err.Error(), "owned by uid") {
		t.Fatalf("want owner error, got %v", err)
	}
}

func writeProc(t *testing.T, root, pid, comm, status string) {
	t.Helper()
	dir := filepath.Join(root, pid)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "comm"), []byte(comm+"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "status"), []byte(status), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestOtherCoreRunning(t *testing.T) {
	root := t.TempDir()
	old := procRoot
	procRoot = root
	t.Cleanup(func() { procRoot = old })

	writeProc(t, root, "4242", "whoisthat-core", "Name:\twhoisthat-core\nState:\tS (sleeping)\nUid:\t4242\t4242\t4242\t4242\n")
	writeProc(t, root, "4243", "xray", "State:\tS (sleeping)\nUid:\t4242\t4242\t4242\t4242\n")

	uid, pid := OtherCoreRunning()
	if pid != 4242 || uid != 4242 {
		t.Fatalf("want (4242,4242), got (%d,%d)", uid, pid)
	}
}

func TestOtherCoreRunningSkipsZombie(t *testing.T) {
	root := t.TempDir()
	old := procRoot
	procRoot = root
	t.Cleanup(func() { procRoot = old })

	writeProc(t, root, "4242", "whoisthat-core", "State:\tZ (zombie)\nUid:\t4242\t4242\t4242\t4242\n")
	if uid, pid := OtherCoreRunning(); pid != 0 || uid != 0 {
		t.Fatalf("zombie must be ignored, got (%d,%d)", uid, pid)
	}
}
