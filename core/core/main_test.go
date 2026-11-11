package main

import (
	"os"
	"path/filepath"
	"regexp"
	"testing"
)

func TestAcquireInstanceLock(t *testing.T) {
	t.Setenv("XDG_RUNTIME_DIR", t.TempDir())

	ok, dir, err := acquireInstanceLock()
	if err != nil || !ok {
		t.Fatalf("first lock: ok=%v err=%v", ok, err)
	}
	if want := filepath.Join(os.Getenv("XDG_RUNTIME_DIR"), "whoisthat"); dir != want {
		t.Fatalf("lock dir = %q, want %q", dir, want)
	}

	ok, dir, err = acquireInstanceLock()
	if err != nil {
		t.Fatalf("second lock returned an I/O error: %v", err)
	}
	if ok {
		t.Fatalf("second lock acquired in %q; expected (false, _, nil)", dir)
	}

	instanceLock.Close()
	instanceLock = nil

	ok, _, err = acquireInstanceLock()
	if err != nil || !ok {
		t.Fatalf("lock after release: ok=%v err=%v", ok, err)
	}
	instanceLock.Close()
	instanceLock = nil
}

func TestHandleArgs(t *testing.T) {
	tests := []struct {
		name string
		args []string
		code int
		done bool
	}{
		{"no arguments starts the daemon", nil, 0, false},
		{"--version", []string{"--version"}, 0, true},
		{"-v", []string{"-v"}, 0, true},
		{"version", []string{"version"}, 0, true},
		{"--help", []string{"--help"}, 0, true},
		{"-h", []string{"-h"}, 0, true},
		{"unknown argument", []string{"--foo"}, 2, true},
		{"only the first argument counts", []string{"--foo", "--help"}, 2, true},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			code, done := handleArgs(tc.args)
			if code != tc.code || done != tc.done {
				t.Fatalf("handleArgs(%q) = (%d, %v), want (%d, %v)", tc.args, code, done, tc.code, tc.done)
			}
		})
	}
}

func TestVersionLineFormat(t *testing.T) {
	// install.sh and release.yml parse this exact shape.
	pattern := regexp.MustCompile(`^whoisthat-core v\d+\.\d+\.\d+ \(protocol \d+\)\n$`)
	if !pattern.MatchString(versionLine()) {
		t.Fatalf("versionLine() = %q does not match %s", versionLine(), pattern)
	}
}
