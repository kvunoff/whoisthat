package mainproxy

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"syscall"
	"time"

	"whoisthat-core/lib"
	"whoisthat-core/lib/geo"
	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/xraymgr"
	"whoisthat-core/structs"
	"whoisthat-core/utils"

	"golang.org/x/sys/unix"
)

const (
	// xrayPreflightTimeout bounds `xray run -test`. Geo/DNS-heavy configs need
	// more than the previous 2s, but a hung validation must not block connect.
	xrayPreflightTimeout = 5 * time.Second
	// xrayPreflightMaxOutput caps the error text surfaced in the TUI.
	xrayPreflightMaxOutput = 600
)

// buildPreflightConfig renders the config that `xray run -test` should inspect.
//
// The TUN inbound is deliberately omitted: the test validates routing/DNS
// syntax without opening /dev/net/tun or contending with a live core. Every
// other injection (stats, routing) mirrors what the real config will contain.
func buildPreflightConfig(
	profile structs.Profile,
	socksPort, httpPort, apiPort int,
	selection structs.RoutingSelection,
	withRouting bool,
	tunActive bool,
) ([]byte, error) {
	cfg, err := lib.ParseUriWithTun(profile.Uri, socksPort, httpPort, "")
	if err != nil {
		return nil, fmt.Errorf("failed to parse profile uri: %w", err)
	}

	cfg, err = injectStatsConfig(cfg, apiPort)
	if err != nil {
		return nil, fmt.Errorf("failed to inject stats config: %w", err)
	}

	if withRouting {
		cfg, err = injectRoutingConfig(cfg, selection, tunActive)
		if err != nil {
			return nil, fmt.Errorf("failed to inject routing config: %w", err)
		}
	}

	return cfg, nil
}

// xrayTestBinary resolves the xray binary used for validation. Returns "" when
// no binary is available yet (e.g. the managed runtime is still downloading).
func xrayTestBinary() string {
	if bin, err := xraymgr.GetManager().GetXrayBin(); err == nil && bin != "" {
		return bin
	}
	bin, _ := utils.GetXrayBin()
	return bin
}

// validateXrayConfig runs `xray run -test` against cfg and returns a compact,
// user-facing error when the config is rejected. Missing binary is not an
// error: validation is best-effort and must not block a connect on systems
// that have not downloaded the runtime yet.
func validateXrayConfig(cfg []byte, assetDir string) error {
	bin := xrayTestBinary()
	if bin == "" {
		logger.Warn("preflight: xray binary not available, skipping config validation")
		return nil
	}

	ctx, cancel := context.WithTimeout(context.Background(), xrayPreflightTimeout)
	defer cancel()

	cmd := exec.CommandContext(ctx, bin, "run", "-test")
	if caps := xrayTestAmbientCaps(); len(caps) > 0 {
		cmd.SysProcAttr = &syscall.SysProcAttr{AmbientCaps: caps}
	}

	ad := strings.TrimSpace(assetDir)
	if ad == "" {
		ad = geo.AssetDir()
	}
	if ad != "" {
		cmd.Env = append(os.Environ(), "XRAY_LOCATION_ASSET="+ad)
	}
	cmd.Stdin = bytes.NewReader(cfg)

	out, err := cmd.CombinedOutput()
	if err == nil {
		return nil
	}
	if ctx.Err() == context.DeadlineExceeded {
		return fmt.Errorf("xray config test timed out after %s", xrayPreflightTimeout)
	}
	return fmt.Errorf("xray config test failed: %s", summarizeXrayTestOutput(out))
}

// xrayTestAmbientCaps returns the capabilities to propagate to `xray run -test`.
// `-test` only parses the config, but keeping the same ambient caps as the real
// runtime preserves validation fidelity. Caps are only propagated when the
// caller already holds them, so unit tests / CI without capabilities can still
// run the pre-flight.
func xrayTestAmbientCaps() []uintptr {
	if !hasEffectiveCapability(unix.CAP_NET_ADMIN) {
		return nil
	}
	return []uintptr{unix.CAP_NET_ADMIN, unix.CAP_NET_RAW}
}

// hasEffectiveCapability reports whether the current process holds cap in its
// effective set (Linux capabilities are bit-indexed in CapEff).
func hasEffectiveCapability(cap uintptr) bool {
	data, err := os.ReadFile("/proc/self/status")
	if err != nil {
		return false
	}
	for _, line := range strings.Split(string(data), "\n") {
		if !strings.HasPrefix(line, "CapEff:") {
			continue
		}
		raw := strings.TrimSpace(strings.TrimPrefix(line, "CapEff:"))
		v, err := strconv.ParseUint(raw, 16, 64)
		if err != nil {
			return false
		}
		return v&(uint64(1)<<cap) != 0
	}
	return false
}

// summarizeXrayTestOutput keeps the actionable tail of xray's stderr and
// collapses it into a single line suitable for the TUI footer.
func summarizeXrayTestOutput(out []byte) string {
	s := strings.TrimSpace(string(out))
	if s == "" {
		return "(no output)"
	}

	lines := strings.Split(s, "\n")
	if len(lines) > 6 {
		lines = lines[len(lines)-6:]
	}
	s = strings.Join(lines, " | ")

	if len(s) > xrayPreflightMaxOutput {
		s = "…" + s[len(s)-xrayPreflightMaxOutput:]
	}
	return s
}
