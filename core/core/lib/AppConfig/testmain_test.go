package appconfig

import (
	"os"
	"path/filepath"
	"testing"
)

// TestMain redirects the XDG directories into a throwaway tree so no test can
// touch the real ~/.config/whoisthat.
func TestMain(m *testing.M) {
	root, err := os.MkdirTemp("", "whoisthat-test-*")
	if err != nil {
		panic(err)
	}
	for _, key := range []string{"XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_RUNTIME_DIR"} {
		dir := filepath.Join(root, key)
		_ = os.MkdirAll(dir, 0700)
		_ = os.Setenv(key, dir)
	}
	code := m.Run()
	_ = os.RemoveAll(root)
	os.Exit(code)
}
