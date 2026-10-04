package xraymgr

import (
	"context"
	"fmt"
	"os/exec"
	"regexp"
	"strings"
	"time"
)

// PinnedVersion specifies the exact version of Xray-core pinned to this client release.
const PinnedVersion = "v26.3.27"

var versionRegex = regexp.MustCompile(`(?i)Xray\s+v?([0-9]+\.[0-9]+(?:\.[0-9]+)?)`)

// NormalizeVersion trims spaces and removes leading "v" or "V" for canonical version comparison.
func NormalizeVersion(v string) string {
	v = strings.TrimSpace(v)
	v = strings.TrimPrefix(v, "v")
	v = strings.TrimPrefix(v, "V")
	return v
}

// ParseXrayVersion extracts the semver string from the output of `xray version`.
func ParseXrayVersion(output string) (string, error) {
	lines := strings.Split(output, "\n")
	for _, line := range lines {
		matches := versionRegex.FindStringSubmatch(line)
		if len(matches) >= 2 {
			return matches[1], nil
		}
	}
	return "", fmt.Errorf("could not parse xray version from output: %q", strings.TrimSpace(output))
}

// CheckBinaryVersion executes the binary with the "version" subcommand,
// parses its version, and checks if it matches the expected version.
func CheckBinaryVersion(binPath string, expectedVersion string) (bool, string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()

	cmd := exec.CommandContext(ctx, binPath, "version")
	out, err := cmd.CombinedOutput()
	if err != nil {
		return false, "", fmt.Errorf("failed to run %s version: %w (output: %s)", binPath, err, string(out))
	}

	actual, err := ParseXrayVersion(string(out))
	if err != nil {
		return false, "", err
	}

	matches := NormalizeVersion(actual) == NormalizeVersion(expectedVersion)
	return matches, actual, nil
}
