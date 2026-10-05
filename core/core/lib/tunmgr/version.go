package tunmgr

import (
	"context"
	"fmt"
	"os/exec"
	"regexp"
	"strings"
	"time"
)

// PinnedVersion specifies the exact version of tun2socks pinned to this client release.
const PinnedVersion = "v2.7.0"

var versionRegex = regexp.MustCompile(`(?i)tun2socks(?:[-_a-z0-9/ ]*?)(?:v|\s|-)([0-9]+\.[0-9]+(?:\.[0-9]+)?)`)

// NormalizeVersion trims spaces and removes leading "v" or "V" for canonical version comparison.
func NormalizeVersion(v string) string {
	v = strings.TrimSpace(v)
	v = strings.TrimPrefix(v, "v")
	v = strings.TrimPrefix(v, "V")
	return v
}

// ParseTun2socksVersion extracts the semver string from the output of `tun2socks --version`.
func ParseTun2socksVersion(output string) (string, error) {
	lines := strings.Split(output, "\n")
	for _, line := range lines {
		matches := versionRegex.FindStringSubmatch(line)
		if len(matches) >= 2 {
			return matches[1], nil
		}
	}
	return "", fmt.Errorf("could not parse tun2socks version from output: %q", strings.TrimSpace(output))
}

// CheckBinaryVersion executes the binary with the "--version" flag (with fallback to "-v" and "-version"),
// parses its version, and checks if it matches the expected version.
func CheckBinaryVersion(binPath string, expectedVersion string) (bool, string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()

	cmd := exec.CommandContext(ctx, binPath, "--version")
	out, err := cmd.CombinedOutput()
	if err != nil {
		// Fallback to -v if --version returned non-zero
		ctx2, cancel2 := context.WithTimeout(context.Background(), 3*time.Second)
		defer cancel2()
		cmd2 := exec.CommandContext(ctx2, binPath, "-v")
		out2, err2 := cmd2.CombinedOutput()
		if err2 == nil {
			out = out2
			err = nil
		} else {
			// Fallback to -version (used by Go standard flag package)
			ctx3, cancel3 := context.WithTimeout(context.Background(), 3*time.Second)
			defer cancel3()
			cmd3 := exec.CommandContext(ctx3, binPath, "-version")
			out3, err3 := cmd3.CombinedOutput()
			if err3 == nil {
				out = out3
				err = nil
			}
		}
	}
	if err != nil {
		return false, "", fmt.Errorf("failed to run %s --version: %w (output: %s)", binPath, err, string(out))
	}

	actual, err := ParseTun2socksVersion(string(out))
	if err != nil {
		return false, "", err
	}

	matches := NormalizeVersion(actual) == NormalizeVersion(expectedVersion)
	return matches, actual, nil
}
