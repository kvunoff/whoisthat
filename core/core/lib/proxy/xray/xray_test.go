package xray

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"
)

func TestFailedUnexpectedly(t *testing.T) {
	killed := errors.New("signal: killed")
	canceled := context.Canceled

	cases := []struct {
		name    string
		ctxErr  error
		waitErr error
		want    bool
	}{
		{"clean exit", nil, nil, false},
		{"intentional kill", canceled, killed, false},
		{"context canceled is intentional", canceled, canceled, false},
		{"config rejection", nil, errors.New("exit status 23"), true},
		{"clean exit after cancel", canceled, nil, false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := failedUnexpectedly(tc.ctxErr, tc.waitErr); got != tc.want {
				t.Fatalf("failedUnexpectedly() = %v, want %v", got, tc.want)
			}
		})
	}
}

func TestCrashTailReadsOnlyTheEnd(t *testing.T) {
	path := filepath.Join(t.TempDir(), "stderr.log")
	if err := os.WriteFile(path, []byte("first line\nsecond line\nthird line\n"), 0600); err != nil {
		t.Fatal(err)
	}

	tail := crashTail(path, 11)
	if tail != "third line" {
		t.Fatalf("crashTail() = %q, want %q", tail, "third line")
	}
}

func TestCrashTailMissingFile(t *testing.T) {
	if got := crashTail(filepath.Join(t.TempDir(), "absent"), 64); got != "" {
		t.Fatalf("expected empty tail, got %q", got)
	}
}

func fakeXray(t *testing.T, body string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "xray")
	if err := os.WriteFile(path, []byte("#!/bin/sh\n"+body), 0755); err != nil {
		t.Fatal(err)
	}
	return path
}

// useFakeBinary replaces the privileged defaults with a plain script.
func useFakeBinary(t *testing.T, path string) {
	t.Helper()
	oldCaps, oldBin := ambientCaps, getXrayBin
	ambientCaps = nil
	getXrayBin = func() (string, error) { return path, nil }
	t.Cleanup(func() {
		ambientCaps = oldCaps
		getXrayBin = oldBin
	})
}

func skipIfRoot(t *testing.T) {
	t.Helper()
	if os.Getuid() == 0 {
		t.Skip("root drops to DedicatedUid; the fake binary in t.TempDir() is unreadable")
	}
}

func TestSetOrReplaceEnv(t *testing.T) {
	cases := []struct {
		name string
		env  []string
		want []string
	}{
		{"replaces existing", []string{"A=1", "B=2"}, []string{"B=2", "A=9"}},
		{"appends missing", []string{"B=2"}, []string{"B=2", "A=9"}},
		{"keeps unrelated", []string{"B=2", "C=3"}, []string{"B=2", "C=3", "A=9"}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := setOrReplaceEnv(tc.env, "A", "9"); !reflect.DeepEqual(got, tc.want) {
				t.Fatalf("setOrReplaceEnv() = %v, want %v", got, tc.want)
			}
		})
	}
}

func TestStartStop(t *testing.T) {
	skipIfRoot(t)
	useFakeBinary(t, fakeXray(t, "cat >/dev/null\nsleep 60\n"))

	x := &XrayCore{}
	if err := x.Start([]byte("{}"), t.TempDir()); err != nil {
		t.Fatalf("Start() = %v", err)
	}
	if !x.IsRunning() {
		t.Fatal("IsRunning() = false after Start")
	}

	x.Stop()
	if x.IsRunning() {
		t.Fatal("IsRunning() = true after Stop")
	}
	select {
	case _, ok := <-x.Exited:
		if ok {
			t.Fatal("Exited delivered an error on an intentional stop")
		}
	case <-time.After(2 * time.Second):
		t.Fatal("Exited was not closed by Stop")
	}
}

func TestStartTwice(t *testing.T) {
	skipIfRoot(t)
	useFakeBinary(t, fakeXray(t, "cat >/dev/null\nsleep 60\n"))

	x := &XrayCore{}
	if err := x.Start([]byte("{}"), t.TempDir()); err != nil {
		t.Fatalf("first Start() = %v", err)
	}
	defer x.Stop()

	if err := x.Start([]byte("{}"), t.TempDir()); err == nil {
		t.Fatal("second Start() succeeded, want an error")
	}
}

func TestUnexpectedExitDeliversError(t *testing.T) {
	skipIfRoot(t)
	useFakeBinary(t, fakeXray(t, "exit 1\n"))

	x := &XrayCore{Exited: make(chan error, 1)}
	if err := x.Start([]byte("{}"), t.TempDir()); err != nil {
		t.Fatalf("Start() = %v", err)
	}
	select {
	case err, ok := <-x.Exited:
		if !ok || err == nil {
			t.Fatalf("Exited = %v, open=%v; want an error", err, ok)
		}
	case <-time.After(3 * time.Second):
		t.Fatal("unexpected exit was not reported")
	}
}

func TestStartMissingBinary(t *testing.T) {
	old := getXrayBin
	getXrayBin = func() (string, error) { return "", errors.New("not ready") }
	t.Cleanup(func() { getXrayBin = old })

	x := &XrayCore{}
	err := x.Start([]byte("{}"), t.TempDir())
	if err == nil || !strings.Contains(err.Error(), "failed to start xray") {
		t.Fatalf("Start() = %v, want a wrapped binary error", err)
	}
	if x.IsRunning() {
		t.Fatal("IsRunning() = true after a failed Start")
	}
}
