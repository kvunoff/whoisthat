package tunmgr

import (
	"archive/zip"
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"whoisthat-core/structs"
)

func TestNormalizeVersion(t *testing.T) {
	cases := []struct {
		input    string
		expected string
	}{
		{"v2.7.0", "2.7.0"},
		{"V2.7.0", "2.7.0"},
		{"2.7.0", "2.7.0"},
		{"  v2.7.0\n", "2.7.0"},
	}

	for _, c := range cases {
		got := NormalizeVersion(c.input)
		if got != c.expected {
			t.Errorf("NormalizeVersion(%q) = %q, want %q", c.input, got, c.expected)
		}
	}
}

func TestParseTun2socksVersion(t *testing.T) {
	out1 := "tun2socks-2.7.0\nlinux/amd64, go1.26.3, 8dda19e"
	v1, err := ParseTun2socksVersion(out1)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if v1 != "2.7.0" {
		t.Errorf("got %q, want %q", v1, "2.7.0")
	}

	out2 := "tun2socks v2.7.0 (built with go1.26.0)"
	v2, err := ParseTun2socksVersion(out2)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if v2 != "2.7.0" {
		t.Errorf("got %q, want %q", v2, "2.7.0")
	}

	out3 := "tun2socks-v2.7.0\n"
	v3, err := ParseTun2socksVersion(out3)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if v3 != "2.7.0" {
		t.Errorf("got %q, want %q", v3, "2.7.0")
	}

	out4 := "tun2socks 2.7.0"
	v4, err := ParseTun2socksVersion(out4)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if v4 != "2.7.0" {
		t.Errorf("got %q, want %q", v4, "2.7.0")
	}

	// Arch Linux / unversioned build output: "tun2socks-"
	_, err = ParseTun2socksVersion("tun2socks-\nlinux/amd64")
	if err == nil {
		t.Error("expected error for unversioned tun2socks- output, got nil")
	}

	_, err = ParseTun2socksVersion("command not found")
	if err == nil {
		t.Error("expected error for invalid output, got nil")
	}
}

func TestGetTargetArch(t *testing.T) {
	arch, err := GetTargetArch()
	if err != nil {
		t.Fatalf("GetTargetArch failed: %v", err)
	}
	if arch == "" {
		t.Error("expected non-empty arch")
	}
}

func TestVerifySHA256(t *testing.T) {
	tmpDir := t.TempDir()
	filePath := filepath.Join(tmpDir, "sample.txt")
	content := []byte("hello tun2socks")
	if err := os.WriteFile(filePath, content, 0644); err != nil {
		t.Fatal(err)
	}

	hasher := sha256.New()
	hasher.Write(content)
	expectedSHA := hex.EncodeToString(hasher.Sum(nil))

	if err := verifySHA256(filePath, expectedSHA); err != nil {
		t.Errorf("verifySHA256 failed: %v", err)
	}

	if err := verifySHA256(filePath, "0000000000000000000000000000000000000000000000000000000000000000"); err == nil {
		t.Error("expected error for mismatched hash, got nil")
	}
}

func TestUnzipArchive_WithRenaming(t *testing.T) {
	tmpDir := t.TempDir()
	zipPath := filepath.Join(tmpDir, "test.zip")
	destDir := filepath.Join(tmpDir, "dest")

	buf := new(bytes.Buffer)
	zw := zip.NewWriter(buf)

	// In official releases, the binary is named e.g. tun2socks-linux-amd64
	f, err := zw.Create("tun2socks-linux-amd64")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := f.Write([]byte("fake-tun2socks-binary")); err != nil {
		t.Fatal(err)
	}

	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}

	if err := os.WriteFile(zipPath, buf.Bytes(), 0644); err != nil {
		t.Fatal(err)
	}

	if err := unzipArchive(zipPath, destDir); err != nil {
		t.Fatalf("unzipArchive failed: %v", err)
	}

	tun2socksBin := filepath.Join(destDir, "tun2socks")
	info, err := os.Stat(tun2socksBin)
	if err != nil {
		t.Fatalf("tun2socks was not extracted/renamed to tun2socks: %v", err)
	}
	if info.Mode()&0111 == 0 {
		t.Errorf("expected tun2socks to be executable, mode is %v", info.Mode())
	}
}

func TestUnzipArchive_WithExactName(t *testing.T) {
	tmpDir := t.TempDir()
	zipPath := filepath.Join(tmpDir, "test.zip")
	destDir := filepath.Join(tmpDir, "dest")

	buf := new(bytes.Buffer)
	zw := zip.NewWriter(buf)

	f, err := zw.Create("tun2socks")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := f.Write([]byte("fake-tun2socks-binary")); err != nil {
		t.Fatal(err)
	}

	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}

	if err := os.WriteFile(zipPath, buf.Bytes(), 0644); err != nil {
		t.Fatal(err)
	}

	if err := unzipArchive(zipPath, destDir); err != nil {
		t.Fatalf("unzipArchive failed: %v", err)
	}

	tun2socksBin := filepath.Join(destDir, "tun2socks")
	info, err := os.Stat(tun2socksBin)
	if err != nil {
		t.Fatalf("tun2socks was not extracted: %v", err)
	}
	if info.Mode()&0111 == 0 {
		t.Errorf("expected tun2socks to be executable, mode is %v", info.Mode())
	}
}

func TestRuntimeDir(t *testing.T) {
	dir, err := RuntimeDir()
	if err != nil {
		t.Fatalf("RuntimeDir failed: %v", err)
	}
	expectedSuffix := filepath.Join(".local", "share", "whoisthat", "runtimes", "tun2socks", PinnedVersion)
	if !strings.HasSuffix(dir, expectedSuffix) {
		t.Errorf("RuntimeDir() = %s, expected suffix %s", dir, expectedSuffix)
	}
}

func TestManager_StatusAndCallbacks(t *testing.T) {
	mgr := NewManager()
	st := mgr.GetStatus()
	if st.Status != "missing" {
		t.Errorf("expected initial status 'missing', got %s", st.Status)
	}
	if st.TargetVersion != PinnedVersion {
		t.Errorf("expected TargetVersion %s, got %s", PinnedVersion, st.TargetVersion)
	}

	called := false
	mgr.SetProgressCallback(func(s structs.Tun2socksStatusInfo) {
		called = true
	})
	mgr.notify(st)
	if !called {
		t.Error("expected callback to be called")
	}
}

func TestManager_AdoptSystemBinary(t *testing.T) {
	tmpDir := t.TempDir()
	srcBin := filepath.Join(tmpDir, "source-tun2socks")
	if err := os.WriteFile(srcBin, []byte("#!/bin/sh\necho tun2socks-2.7.0\n"), 0755); err != nil {
		t.Fatal(err)
	}

	destDir := filepath.Join(tmpDir, "dest-runtime")
	mgr := NewManager()
	if err := mgr.adoptSystemBinary(srcBin, destDir); err != nil {
		t.Fatalf("adoptSystemBinary failed: %v", err)
	}

	adoptedBin := filepath.Join(destDir, "tun2socks")
	info, err := os.Stat(adoptedBin)
	if err != nil {
		t.Fatalf("adopted binary not found: %v", err)
	}
	if info.Mode()&0111 == 0 {
		t.Errorf("expected adopted binary to be executable, mode: %v", info.Mode())
	}
}

func TestParseTun2socksVersion_Extended(t *testing.T) {
	cases := []struct {
		input    string
		expected string
	}{
		{"tun2socks version 2.7.0", "2.7.0"},
		{"tun2socks-linux-amd64 v2.7.0", "2.7.0"},
		{"tun2socks-linux-arm64-2.7.0", "2.7.0"},
		{"tun2socks 2.7.0-beta1", "2.7.0"},
	}

	for _, c := range cases {
		v, err := ParseTun2socksVersion(c.input)
		if err != nil {
			t.Errorf("ParseTun2socksVersion(%q) returned error: %v", c.input, err)
			continue
		}
		if v != c.expected {
			t.Errorf("ParseTun2socksVersion(%q) = %q, want %q", c.input, v, c.expected)
		}
	}
}

func TestUnzipArchive_NestedAndSkipNonBinary(t *testing.T) {
	tmpDir := t.TempDir()
	zipPath := filepath.Join(tmpDir, "test.zip")
	destDir := filepath.Join(tmpDir, "dest")

	buf := new(bytes.Buffer)
	zw := zip.NewWriter(buf)

	// Add a non-binary text file at root that starts with tun2socks
	f1, err := zw.Create("tun2socks.txt")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := f1.Write([]byte("README text")); err != nil {
		t.Fatal(err)
	}

	// Add README
	f2, err := zw.Create("README.md")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := f2.Write([]byte("# tun2socks")); err != nil {
		t.Fatal(err)
	}

	// Add actual binary inside a nested folder
	f3, err := zw.Create("nested-dir/tun2socks-linux-amd64")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := f3.Write([]byte("actual-binary-content")); err != nil {
		t.Fatal(err)
	}

	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}

	if err := os.WriteFile(zipPath, buf.Bytes(), 0644); err != nil {
		t.Fatal(err)
	}

	if err := unzipArchive(zipPath, destDir); err != nil {
		t.Fatalf("unzipArchive failed: %v", err)
	}

	tun2socksBin := filepath.Join(destDir, "tun2socks")
	data, err := os.ReadFile(tun2socksBin)
	if err != nil {
		t.Fatalf("tun2socks binary not found at destination: %v", err)
	}
	if string(data) != "actual-binary-content" {
		t.Errorf("expected actual-binary-content, got %q", string(data))
	}
}

func TestManager_EnsureReady_Concurrency(t *testing.T) {
	mgr := NewManager()
	// Fake ready state
	mgr.mu.Lock()
	mgr.status.Status = "ready"
	mgr.binPath = "/bin/sh"
	mgr.mu.Unlock()

	var wg sync.WaitGroup
	for i := 0; i < 20; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			bin, err := mgr.GetTun2socksBin()
			if err != nil {
				t.Errorf("GetTun2socksBin failed: %v", err)
			}
			if bin != "/bin/sh" {
				t.Errorf("expected /bin/sh, got %q", bin)
			}
		}()
	}
	wg.Wait()
}

