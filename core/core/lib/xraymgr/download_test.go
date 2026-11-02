package xraymgr

import (
	"archive/zip"
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

// fakeXrayZip builds a zip containing a shell script that answers the
// `xray version` probe used by CheckBinaryVersion.
func fakeXrayZip(t *testing.T, version string) []byte {
	t.Helper()
	script := fmt.Sprintf("#!/bin/sh\nif [ \"$1\" = \"version\" ]; then\n  echo \"Xray %s\"\n  exit 0\nfi\nexit 0\n", version)

	var buf bytes.Buffer
	zw := zip.NewWriter(&buf)
	hdr := &zip.FileHeader{Name: "xray", Method: zip.Deflate}
	hdr.SetMode(0755)
	w, err := zw.CreateHeader(hdr)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := w.Write([]byte(script)); err != nil {
		t.Fatal(err)
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
	return buf.Bytes()
}

func writeFakeXray(t *testing.T, dir, version string) string {
	t.Helper()
	path := filepath.Join(dir, "xray")
	script := fmt.Sprintf("#!/bin/sh\nif [ \"$1\" = \"version\" ]; then\n  echo \"Xray %s\"\n  exit 0\nfi\nexit 0\n", version)
	if err := os.WriteFile(path, []byte(script), 0755); err != nil {
		t.Fatal(err)
	}
	return path
}

// newXrayTestServer serves the .dgst and zip assets for the current arch.
func newXrayTestServer(t *testing.T, digest string, zipData []byte) *httptest.Server {
	t.Helper()
	arch, err := GetTargetArch()
	if err != nil {
		t.Fatal(err)
	}
	zipName := fmt.Sprintf("Xray-linux-%s.zip", arch)
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch filepath.Base(r.URL.Path) {
		case zipName + ".dgst":
			fmt.Fprintf(w, "SHA2-256= %s\n", digest)
		case zipName:
			_, _ = w.Write(zipData)
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(srv.Close)
	return srv
}

// withDownloadBase points both the digest origin and the mirror list at base
// for the duration of the test.
func withDownloadBase(t *testing.T, base string) {
	t.Helper()
	oldDigest, oldMirrors := digestBaseURL, downloadMirrors
	digestBaseURL = base
	downloadMirrors = []string{base}
	t.Cleanup(func() {
		digestBaseURL = oldDigest
		downloadMirrors = oldMirrors
	})
}

func TestDownloadAndInstall_HappyPath(t *testing.T) {
	t.Setenv("XDG_DATA_HOME", t.TempDir())
	zipData := fakeXrayZip(t, PinnedVersion)
	sum := sha256.Sum256(zipData)
	srv := newXrayTestServer(t, hex.EncodeToString(sum[:]), zipData)
	withDownloadBase(t, srv.URL)

	m := NewManager()
	if err := m.downloadAndInstall(); err != nil {
		t.Fatalf("downloadAndInstall() = %v", err)
	}

	if m.binPath == "" {
		t.Fatal("binPath was not set")
	}
	info, err := os.Stat(m.binPath)
	if err != nil {
		t.Fatalf("installed binary: %v", err)
	}
	if info.Mode()&0111 == 0 {
		t.Fatalf("installed binary is not executable: %v", info.Mode())
	}
}

func TestDownloadAndInstall_RejectsBadChecksum(t *testing.T) {
	t.Setenv("XDG_DATA_HOME", t.TempDir())
	zipData := fakeXrayZip(t, PinnedVersion)
	srv := newXrayTestServer(t, strings.Repeat("0", 64), zipData)
	withDownloadBase(t, srv.URL)

	m := NewManager()
	err := m.downloadAndInstall()
	if err == nil || !strings.Contains(err.Error(), "checksum verification failed") {
		t.Fatalf("downloadAndInstall() = %v, want checksum error", err)
	}

	runtimeDir, err := RuntimeDir()
	if err != nil {
		t.Fatal(err)
	}
	if _, statErr := os.Stat(filepath.Join(runtimeDir, "xray")); !os.IsNotExist(statErr) {
		t.Fatalf("binary installed despite bad checksum: %v", statErr)
	}
}

func TestDownloadAndInstall_RejectsWrongVersion(t *testing.T) {
	t.Setenv("XDG_DATA_HOME", t.TempDir())
	zipData := fakeXrayZip(t, "26.3.27")
	sum := sha256.Sum256(zipData)
	srv := newXrayTestServer(t, hex.EncodeToString(sum[:]), zipData)
	withDownloadBase(t, srv.URL)

	m := NewManager()
	err := m.downloadAndInstall()
	if err == nil || !strings.Contains(err.Error(), "did not match pinned version") {
		t.Fatalf("downloadAndInstall() = %v, want version mismatch", err)
	}
}

func TestDownloadArchive_FallsBackToSecondMirror(t *testing.T) {
	body := []byte("zip-bytes")
	bad := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "nope", http.StatusInternalServerError)
	}))
	t.Cleanup(bad.Close)
	good := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write(body)
	}))
	t.Cleanup(good.Close)

	old := downloadMirrors
	downloadMirrors = []string{bad.URL, good.URL}
	t.Cleanup(func() { downloadMirrors = old })

	dest := filepath.Join(t.TempDir(), "xray.zip")
	if err := downloadArchive(http.DefaultClient, "v-test", "64", dest, nil); err != nil {
		t.Fatalf("downloadArchive() = %v, want nil", err)
	}
	got, err := os.ReadFile(dest)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(got, body) {
		t.Fatalf("downloaded %q, want %q", got, body)
	}
}

func TestUnzipArchive_RejectsPathTraversal(t *testing.T) {
	zipPath := filepath.Join(t.TempDir(), "evil.zip")
	var buf bytes.Buffer
	zw := zip.NewWriter(&buf)
	w, err := zw.Create("../evil")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := w.Write([]byte("pwned")); err != nil {
		t.Fatal(err)
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(zipPath, buf.Bytes(), 0644); err != nil {
		t.Fatal(err)
	}

	dest := filepath.Join(t.TempDir(), "dest")
	if err := unzipArchive(zipPath, dest); err != nil {
		t.Fatalf("unzipArchive() = %v", err)
	}
	if _, err := os.Stat(filepath.Join(dest, "..", "evil")); !os.IsNotExist(err) {
		t.Fatalf("path traversal escaped the destination: %v", err)
	}
}

func TestEnsureRuntimeAsync_SingleFlight(t *testing.T) {
	t.Setenv("XDG_DATA_HOME", t.TempDir())
	zipData := fakeXrayZip(t, PinnedVersion)
	sum := sha256.Sum256(zipData)
	arch, err := GetTargetArch()
	if err != nil {
		t.Fatal(err)
	}
	zipName := fmt.Sprintf("Xray-linux-%s.zip", arch)

	var requests atomic.Int32
	started := make(chan struct{}, 1)
	release := make(chan struct{})
	var releaseOnce sync.Once
	unblock := func() { releaseOnce.Do(func() { close(release) }) }
	defer unblock()

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch filepath.Base(r.URL.Path) {
		case zipName + ".dgst":
			fmt.Fprintf(w, "SHA2-256= %s\n", hex.EncodeToString(sum[:]))
		case zipName:
			requests.Add(1)
			select {
			case started <- struct{}{}:
			default:
			}
			<-release
			_, _ = w.Write(zipData)
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(srv.Close)
	withDownloadBase(t, srv.URL)

	m := NewManager()
	m.EnsureRuntimeAsync()
	select {
	case <-started:
	case <-time.After(2 * time.Second):
		t.Fatal("download did not start")
	}

	m.EnsureRuntimeAsync() // must be a no-op while the first one runs
	unblock()

	deadline := time.Now().Add(3 * time.Second)
	for m.GetStatus().Status == "downloading" {
		if time.Now().After(deadline) {
			t.Fatal("download did not finish")
		}
		time.Sleep(10 * time.Millisecond)
	}
	if got := m.GetStatus().Status; got != "ready" {
		t.Fatalf("status = %q, want ready", got)
	}
	if got := requests.Load(); got != 1 {
		t.Fatalf("zip requests = %d, want 1", got)
	}
}

func TestEnsureRuntimeAsync_ErrorStatus(t *testing.T) {
	t.Setenv("XDG_DATA_HOME", t.TempDir())
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "missing", http.StatusInternalServerError)
	}))
	t.Cleanup(srv.Close)
	withDownloadBase(t, srv.URL)

	m := NewManager()
	m.EnsureRuntimeAsync()

	deadline := time.Now().Add(3 * time.Second)
	for m.GetStatus().Status == "downloading" {
		if time.Now().After(deadline) {
			t.Fatal("download did not finish")
		}
		time.Sleep(10 * time.Millisecond)
	}
	st := m.GetStatus()
	if st.Status != "error" {
		t.Fatalf("status = %q, want error", st.Status)
	}
	if st.Error == "" {
		t.Fatal("error status carries no message")
	}
}

func TestInit_AdoptsManagedBinary(t *testing.T) {
	t.Setenv("XDG_DATA_HOME", t.TempDir())
	runtimeDir, err := RuntimeDir()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(runtimeDir, 0755); err != nil {
		t.Fatal(err)
	}
	writeFakeXray(t, runtimeDir, PinnedVersion)

	m := NewManager()
	if err := m.Init(); err != nil {
		t.Fatal(err)
	}
	bin, err := m.GetXrayBin()
	if err != nil {
		t.Fatalf("GetXrayBin() = %v", err)
	}
	if want := filepath.Join(runtimeDir, "xray"); bin != want {
		t.Fatalf("bin = %q, want %q", bin, want)
	}
}

func TestInit_AdoptsSystemBinary(t *testing.T) {
	t.Setenv("XDG_DATA_HOME", t.TempDir())
	dir := t.TempDir()
	writeFakeXray(t, dir, PinnedVersion)
	t.Setenv("PATH", dir)

	m := NewManager()
	if err := m.Init(); err != nil {
		t.Fatal(err)
	}
	runtimeDir, err := RuntimeDir()
	if err != nil {
		t.Fatal(err)
	}
	want := filepath.Join(runtimeDir, "xray")
	bin, err := m.GetXrayBin()
	if err != nil {
		t.Fatalf("GetXrayBin() = %v", err)
	}
	if bin != want {
		t.Fatalf("bin = %q, want %q", bin, want)
	}
	if _, err := os.Stat(want); err != nil {
		t.Fatalf("system binary was not adopted: %v", err)
	}
}

func TestInit_WrongManagedFallsBackToSystem(t *testing.T) {
	t.Setenv("XDG_DATA_HOME", t.TempDir())
	runtimeDir, err := RuntimeDir()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(runtimeDir, 0755); err != nil {
		t.Fatal(err)
	}
	writeFakeXray(t, runtimeDir, "26.3.27")

	dir := t.TempDir()
	writeFakeXray(t, dir, PinnedVersion)
	t.Setenv("PATH", dir)

	m := NewManager()
	if err := m.Init(); err != nil {
		t.Fatal(err)
	}
	if _, err := m.GetXrayBin(); err != nil {
		t.Fatalf("GetXrayBin() = %v, want the system binary", err)
	}
}

func TestGetXrayBin_States(t *testing.T) {
	m := NewManager()
	if _, err := m.GetXrayBin(); err == nil {
		t.Fatal("expected an error while missing")
	}

	m.status.Status = "downloading"
	m.status.Progress = 42
	if _, err := m.GetXrayBin(); err == nil || !strings.Contains(err.Error(), "42%") {
		t.Fatalf("downloading error = %v, want progress", err)
	}

	m.status.Status = "ready"
	m.binPath = "/tmp/xray"
	bin, err := m.GetXrayBin()
	if err != nil || bin != "/tmp/xray" {
		t.Fatalf("GetXrayBin() = %q, %v", bin, err)
	}
}

func TestFetchDigest_Non200(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "missing", http.StatusNotFound)
	}))
	t.Cleanup(srv.Close)

	old := digestBaseURL
	digestBaseURL = srv.URL
	t.Cleanup(func() { digestBaseURL = old })

	if _, err := fetchDigest(srv.Client(), PinnedVersion, "64"); err == nil {
		t.Fatal("expected an error for HTTP 404")
	}
}
