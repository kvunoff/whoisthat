package utils

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

func GetBinPath(name string) (string, error) {
	if p, err := exec.LookPath(name); err == nil {
		return filepath.Abs(p)
	}

	paths := []string{
		"./" + name,
		filepath.Join(".", "bin", name),
		filepath.Join("parser", "target", "release", name),
		filepath.Join("..", "..", "parser", "target", "release", name),
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
