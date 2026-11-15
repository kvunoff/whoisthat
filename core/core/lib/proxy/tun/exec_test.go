package tunmode

import (
	"os"
	"path/filepath"
	"testing"
)

func TestMain(m *testing.M) {
	if os.Getuid() != 0 {
		privilegedOwner = os.Getuid()
	}
	os.Exit(m.Run())
}

func TestValidatePrivilegedPath(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "tool")
	if err := os.WriteFile(bin, []byte("#!/bin/sh\n"), 0755); err != nil {
		t.Fatal(err)
	}

	self := os.Getuid()
	old := privilegedOwner
	t.Cleanup(func() { privilegedOwner = old })

	privilegedOwner = self
	if _, err := validatePrivilegedPath(bin); err != nil {
		t.Fatalf("a user-owned tool must be accepted when it is the expected owner: %v", err)
	}

	privilegedOwner = self + 1
	if _, err := validatePrivilegedPath(bin); err == nil {
		t.Error("a tool owned by another uid must be rejected")
	}

	privilegedOwner = self
	if err := os.Chmod(bin, 0775); err != nil {
		t.Fatal(err)
	}
	if _, err := validatePrivilegedPath(bin); err == nil {
		t.Error("a group-writable tool must be rejected")
	}

	if err := os.Chmod(bin, 0755); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(dir, "link")
	if err := os.Symlink(bin, link); err != nil {
		t.Fatal(err)
	}
	if _, err := validatePrivilegedPath(link); err != nil {
		t.Fatalf("a symlink to a valid tool must be accepted: %v", err)
	}
}
