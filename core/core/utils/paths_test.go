package utils

import (
	"path/filepath"
	"testing"
)

func TestXDGPathsRespected(t *testing.T) {
	configHome := t.TempDir()
	dataHome := t.TempDir()
	runtimeDir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", configHome)
	t.Setenv("XDG_DATA_HOME", dataHome)
	t.Setenv("XDG_RUNTIME_DIR", runtimeDir)

	if got, want := ConfigDir(), filepath.Join(configHome, "whoisthat"); got != want {
		t.Errorf("ConfigDir() = %q, want %q", got, want)
	}
	if got, want := DataDir(), filepath.Join(dataHome, "whoisthat"); got != want {
		t.Errorf("DataDir() = %q, want %q", got, want)
	}
	if got, want := RuntimeDir(), filepath.Join(runtimeDir, "whoisthat"); got != want {
		t.Errorf("RuntimeDir() = %q, want %q", got, want)
	}
}

func TestXDGAbsolutePathsOnly(t *testing.T) {
	// Relative XDG values must be ignored per the XDG spec.
	t.Setenv("XDG_CONFIG_HOME", "relative/path")
	t.Setenv("XDG_DATA_HOME", "relative/path")
	t.Setenv("XDG_RUNTIME_DIR", "relative/path")

	for name, got := range map[string]string{
		"ConfigDir":  ConfigDir(),
		"DataDir":    DataDir(),
		"RuntimeDir": RuntimeDir(),
	} {
		if got == "relative/path" || filepath.IsAbs(got) == false {
			t.Errorf("%s() = %q, want an absolute fallback", name, got)
		}
	}
}
