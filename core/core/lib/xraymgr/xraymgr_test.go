package xraymgr

import (
	"archive/zip"
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"os"
	"path/filepath"
	"testing"
)

func TestNormalizeVersion(t *testing.T) {
	cases := []struct {
		input    string
		expected string
	}{
		{"v26.3.27", "26.3.27"},
		{"V26.3.27", "26.3.27"},
		{"26.3.27", "26.3.27"},
		{"  v1.8.23\n", "1.8.23"},
	}

	for _, c := range cases {
		got := NormalizeVersion(c.input)
		if got != c.expected {
			t.Errorf("NormalizeVersion(%q) = %q, want %q", c.input, got, c.expected)
		}
	}
}

func TestParseXrayVersion(t *testing.T) {
	out1 := `Xray 26.3.27 (Xray, Penetrates Everything.) cc66b68-dirty (go1.27.1-X:nodwarf5 linux/amd64)
A unified platform for anti-censorship.`
	v1, err := ParseXrayVersion(out1)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if v1 != "26.3.27" {
		t.Errorf("got %q, want %q", v1, "26.3.27")
	}

	out2 := `Xray v1.8.23 (Xray, Penetrates Everything.) Custom
A unified platform for anti-censorship.`
	v2, err := ParseXrayVersion(out2)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if v2 != "1.8.23" {
		t.Errorf("got %q, want %q", v2, "1.8.23")
	}

	_, err = ParseXrayVersion("command not found")
	if err == nil {
		t.Error("expected error for invalid output, got nil")
	}
}

func TestParseDgstSHA256(t *testing.T) {
	dgst := `MD5= ee4e2ff74948a9b464624b1cabc44409
SHA1= b55b06e74e89083b9cedfdecf0d68b579cd2af72
SHA2-256= 23cd9af937744d97776ee35ecad4972cf4b2109d1e0fe6be9930467608f7c8ae
SHA2-512= e8bc40a0687cac184bbe4b5c1f047e69064ccedc489fb25e208889ae287bbf8736dff16b108d68fc00dc33edc8bb53502e47a9698a277f4f51b67b83d899e518`

	sha, err := ParseDgstSHA256(dgst)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	expected := "23cd9af937744d97776ee35ecad4972cf4b2109d1e0fe6be9930467608f7c8ae"
	if sha != expected {
		t.Errorf("got %s, want %s", sha, expected)
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
	content := []byte("hello xray core")
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

func TestUnzipArchive(t *testing.T) {
	tmpDir := t.TempDir()
	zipPath := filepath.Join(tmpDir, "test.zip")
	destDir := filepath.Join(tmpDir, "dest")

	buf := new(bytes.Buffer)
	zw := zip.NewWriter(buf)

	f, err := zw.Create("xray")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := f.Write([]byte("fake-xray-binary")); err != nil {
		t.Fatal(err)
	}

	f2, err := zw.Create("geoip.dat")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := f2.Write([]byte("fake-geo-data")); err != nil {
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

	xrayBin := filepath.Join(destDir, "xray")
	info, err := os.Stat(xrayBin)
	if err != nil {
		t.Fatalf("xray was not extracted: %v", err)
	}
	if info.Mode()&0111 == 0 {
		t.Errorf("expected xray to be executable, mode is %v", info.Mode())
	}
}
