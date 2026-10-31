package xray

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"
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
