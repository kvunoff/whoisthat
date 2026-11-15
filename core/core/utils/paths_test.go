package utils

import (
	"net"
	"os"
	"path/filepath"
	"strconv"
	"testing"
)

func TestXDGPathsRespected(t *testing.T) {
	configHome := t.TempDir()
	dataHome := t.TempDir()
	runtimeDir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", configHome)
	t.Setenv("XDG_DATA_HOME", dataHome)
	t.Setenv("XDG_RUNTIME_DIR", runtimeDir)

	if got, want := ConfigDir(), filepath.Join(configHome, "whoisthat"); got != want {
		t.Errorf("ConfigDir() = %q, want %q", got, want)
	}
	if got, want := DataDir(), filepath.Join(dataHome, "whoisthat"); got != want {
		t.Errorf("DataDir() = %q, want %q", got, want)
	}
	if got, want := RuntimeDir(), filepath.Join(runtimeDir, "whoisthat"); got != want {
		t.Errorf("RuntimeDir() = %q, want %q", got, want)
	}
}

func TestXDGAbsolutePathsOnly(t *testing.T) {
	// Relative XDG values must be ignored per the XDG spec.
	t.Setenv("XDG_CONFIG_HOME", "relative/path")
	t.Setenv("XDG_DATA_HOME", "relative/path")
	t.Setenv("XDG_RUNTIME_DIR", "relative/path")

	for name, got := range map[string]string{
		"ConfigDir":  ConfigDir(),
		"DataDir":    DataDir(),
		"RuntimeDir": RuntimeDir(),
	} {
		if got == "relative/path" || filepath.IsAbs(got) == false {
			t.Errorf("%s() = %q, want an absolute fallback", name, got)
		}
	}
}

func TestSocketIsLive(t *testing.T) {
	dir := t.TempDir()
	sock := filepath.Join(dir, "core.sock")

	if SocketIsLive(sock) {
		t.Fatal("missing socket reported as live")
	}

	ln, err := net.Listen("unix", sock)
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	defer ln.Close()
	go func() {
		for {
			conn, err := ln.Accept()
			if err != nil {
				return
			}
			conn.Close()
		}
	}()

	if !SocketIsLive(sock) {
		t.Fatal("live socket reported as dead")
	}
}

func TestRuntimeBaseInitBackendHasNoFallback(t *testing.T) {
	configHome := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", configHome)
	t.Setenv("XDG_RUNTIME_DIR", t.TempDir())

	conf := filepath.Join(configHome, "whoisthat")
	if err := os.MkdirAll(conf, 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(conf, "autostart-backend"), []byte("init\n"), 0600); err != nil {
		t.Fatal(err)
	}

	want := filepath.Join("/run/whoisthat", strconv.Itoa(runtimeUid()))
	if got := RuntimeBase(); got != want {
		t.Errorf("RuntimeBase() = %q, want %q (init backend must not fall back)", got, want)
	}
}

func TestRuntimeBaseFallsBackToXDG(t *testing.T) {
	configHome := t.TempDir()
	runtimeDir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", configHome)
	t.Setenv("XDG_RUNTIME_DIR", runtimeDir)

	if got := RuntimeBase(); got != runtimeDir {
		t.Errorf("RuntimeBase() = %q, want %q", got, runtimeDir)
	}
}

func TestEnsurePrivateDirCreatesAndTightens(t *testing.T) {
	root := t.TempDir()
	dir := filepath.Join(root, "priv")

	if err := EnsurePrivateDir(dir, os.Getuid()); err != nil {
		t.Fatalf("EnsurePrivateDir(create) = %v", err)
	}
	if err := CheckPrivateDir(dir, os.Getuid()); err != nil {
		t.Fatalf("CheckPrivateDir(fresh) = %v", err)
	}

	if err := os.Chmod(dir, 0755); err != nil {
		t.Fatal(err)
	}
	if err := EnsurePrivateDir(dir, os.Getuid()); err != nil {
		t.Fatalf("EnsurePrivateDir(tighten) = %v", err)
	}
	info, err := os.Stat(dir)
	if err != nil {
		t.Fatal(err)
	}
	if got := info.Mode().Perm(); got != 0700 {
		t.Errorf("mode = %04o, want 0700", got)
	}

	if err := CheckPrivateDir(dir, os.Getuid()+1); err == nil {
		t.Error("a foreign owner must be rejected")
	}
}

func TestEnsurePrivateDirCreatesParents(t *testing.T) {
	root := t.TempDir()
	nested := filepath.Join(root, "a", "b", "whoisthat")
	if err := EnsurePrivateDir(nested, os.Getuid()); err != nil {
		t.Fatalf("EnsurePrivateDir(nested) = %v", err)
	}
	if err := CheckPrivateDir(nested, os.Getuid()); err != nil {
		t.Fatalf("CheckPrivateDir(nested) = %v", err)
	}
}

func TestEnsurePrivateDirRejectsSymlinkAndFile(t *testing.T) {
	root := t.TempDir()
	real := filepath.Join(root, "real")
	if err := os.Mkdir(real, 0700); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(root, "link")
	if err := os.Symlink(real, link); err != nil {
		t.Fatal(err)
	}
	if err := EnsurePrivateDir(link, os.Getuid()); err == nil {
		t.Error("a symlink must be rejected")
	}

	file := filepath.Join(root, "file")
	if err := os.WriteFile(file, nil, 0600); err != nil {
		t.Fatal(err)
	}
	if err := CheckPrivateDir(file, os.Getuid()); err == nil {
		t.Error("a regular file must be rejected")
	}
}
