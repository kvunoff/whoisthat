package utils

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// F-03: a PATH entry the user (or a planted file) controls must never decide
// which `nft`/`ip`/`xray` runs with net_admin.
func TestSafeLookPathIgnoresPoisonedPATH(t *testing.T) {
	evil := t.TempDir()
	if err := os.WriteFile(filepath.Join(evil, "nft"), []byte("#!/bin/sh\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", evil)
	if p, err := SafeLookPath("nft"); err == nil && strings.HasPrefix(p, evil) {
		t.Fatalf("SafeLookPath resolved %q from the caller's PATH", p)
	}
	if p, err := SafeLookPath("sh"); err != nil || !filepath.IsAbs(p) || strings.HasPrefix(p, evil) {
		t.Fatalf("SafeLookPath(sh) = %q, %v; want an absolute system path", p, err)
	}
}

func TestSafeEnvPinsPATH(t *testing.T) {
	env := SafeEnv([]string{"A=1", "PATH=/home/u/.local/bin:/tmp/x", "B=2"})
	var paths []string
	for _, e := range env {
		if strings.HasPrefix(e, "PATH=") {
			paths = append(paths, e)
		}
	}
	if len(paths) != 1 || paths[0] != "PATH="+SafePath {
		t.Fatalf("PATH entries = %v, want exactly PATH=%s", paths, SafePath)
	}
}

// Grep guard from the ticket: no bare-name exec.Command left in production
// code. Everything goes through SafeCommand/SafeLookPath, resolved against the
// pinned directories.
func TestNoBareCommandNamesInProductionCode(t *testing.T) {
	re := regexp.MustCompile(`exec\.Command(?:Context)?\(\s*(?:[A-Za-z_.]+,\s*)?"[^"/]`)
	var offenders []string
	err := filepath.Walk("..", func(path string, info os.FileInfo, err error) error {
		if err != nil || info.IsDir() || !strings.HasSuffix(path, ".go") ||
			strings.HasSuffix(path, "_test.go") || strings.HasSuffix(path, "safepath.go") {
			return err
		}
		b, _ := os.ReadFile(path)
		for i, line := range strings.Split(string(b), "\n") {
			if re.MatchString(line) {
				offenders = append(offenders, path+":"+itoa(i+1)+": "+strings.TrimSpace(line))
			}
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	if len(offenders) > 0 {
		t.Fatalf("bare exec.Command names found:\n%s", strings.Join(offenders, "\n"))
	}
}

func itoa(i int) string {
	s := ""
	for ; i > 0; i /= 10 {
		s = string(rune('0'+i%10)) + s
	}
	return s
}
