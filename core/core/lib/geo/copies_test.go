package geo

import (
	"os"
	"path/filepath"
	"testing"
)

func TestEnsureGlobalCopies(t *testing.T) {
	globalDir := t.TempDir()
	for name, body := range map[string]string{
		"geoip.dat":   "global geoip",
		"geosite.dat": "global geosite",
	} {
		if err := os.WriteFile(filepath.Join(globalDir, name), []byte(body), 0600); err != nil {
			t.Fatalf("write %s: %v", name, err)
		}
	}

	oldAssetDir := assetDir
	assetDir = globalDir
	t.Cleanup(func() { assetDir = oldAssetDir })

	providerDir := t.TempDir()
	if !EnsureGlobalCopies(providerDir) {
		t.Fatal("EnsureGlobalCopies returned false")
	}
	for name, want := range map[string]string{
		globalGeoIPName:   "global geoip",
		globalGeoSiteName: "global geosite",
	} {
		got, err := os.ReadFile(filepath.Join(providerDir, name))
		if err != nil {
			t.Fatalf("read %s: %v", name, err)
		}
		if string(got) != want {
			t.Errorf("%s = %q, want %q", name, got, want)
		}
	}

	// Idempotent: a second call keeps the same links/files and succeeds.
	if !EnsureGlobalCopies(providerDir) {
		t.Fatal("second EnsureGlobalCopies returned false")
	}

	if EnsureGlobalCopies("") {
		t.Error("empty asset dir must not report success")
	}
}

func TestEnsureGlobalCopiesWithoutAssets(t *testing.T) {
	oldAssetDir := assetDir
	assetDir = t.TempDir()
	t.Cleanup(func() { assetDir = oldAssetDir })

	if EnsureGlobalCopies(t.TempDir()) {
		t.Error("missing global assets must not report success")
	}
}
