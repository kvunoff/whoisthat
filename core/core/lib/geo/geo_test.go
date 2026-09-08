package geo

import (
	"os"
	"path/filepath"
	"testing"
)

func TestVerifyGeoIP(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil {
		t.Skip("cannot determine home dir")
	}
	geoPath := filepath.Join(home, ".config", "whoisthat", "geo", "geoip.dat")
	if !validFile(geoPath, 1) {
		t.Skip("geoip.dat not found at ~/.config/whoisthat/geo/geoip.dat")
	}

	ok := verifyGeoIP(geoPath)
	if !ok {
		t.Errorf("verifyGeoIP(%q) = false, want true", geoPath)
	}
}
