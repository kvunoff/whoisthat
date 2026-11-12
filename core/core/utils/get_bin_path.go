package utils

import (
	"fmt"
	"os"
	"path/filepath"
)

func GetBinPath(name string) (string, error) {
	// Pinned directories only: never the caller's PATH or the cwd (F-03).
	if p, err := SafeLookPath(name); err == nil {
		return p, nil
	}

	paths := []string{
		filepath.Join("/usr/local/bin", name),
		filepath.Join("/usr/bin", name),
	}

	for _, p := range paths {
		if info, err := os.Stat(p); err == nil && !info.IsDir() {
			return filepath.Abs(p)
		}
	}

	return "", fmt.Errorf("binary %q not found", name)
}
