package tunmode

import (
	"context"
	"encoding/json"
	"fmt"
	"os/exec"
	"strconv"
	"strings"
	"syscall"

	"golang.org/x/sys/unix"

	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
)

// bypassUpComment / bypassDownComment mark the accounting rules. The reader
// sums their byte counters; rules without these comments are ignored, so the
// NEW-flow tagging rule (which only sees the first packet) is never counted.
const (
	bypassUpComment   = "wt-bypass-up"
	bypassDownComment = "wt-bypass-down"
	bypassMarkBit     = "0x4"
)

// runCommandWithCapsOutput mirrors runCommandWithCaps but keeps stdout clean:
// nft -j output must not be mixed with stderr from a diagnostic.
func runCommandWithCapsOutput(name string, args ...string) (string, error) {
	resolved, err := resolvePrivileged(name)
	if err != nil {
		return "", err
	}
	cmd := exec.Command(resolved, args...)
	cmd.Env = []string{"PATH=" + scriptPath(), "LC_ALL=C"}
	cmd.SysProcAttr = &syscall.SysProcAttr{
		AmbientCaps: []uintptr{unix.CAP_NET_ADMIN, unix.CAP_NET_RAW},
	}
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("%s: %w", name, err)
	}
	return string(out), nil
}

// BypassCounters returns cumulative L3 bytes that left through the physical
// gateway without Xray. ok=false means no accounting rules are installed
// (proxy mode, include mode) or the backend could not be inspected; the caller
// then marks the traffic sample as partial instead of claiming full coverage.
func BypassCounters(ctx context.Context) (up, down int64, ok bool, err error) {
	if err := ctx.Err(); err != nil {
		return 0, 0, false, err
	}
	// In include mode the bypass class is the unmarked traffic, which no rule
	// can identify. Report degraded instead of claiming complete coverage.
	if appconfig.GetConfig().SplitTunnel.Mode == "include" {
		return 0, 0, false, nil
	}

	// Split rules always use nftables, regardless of the conntrack backend.
	found := false
	nftOut, nftErr := runCommandWithCapsOutput("nft", "-j", "list", "ruleset")
	if nftErr != nil {
		logger.Warn("bypass: cannot read nft counters:", nftErr)
	} else {
		nftUp, nftDown, nftFound, parseErr := parseNftCounters(nftOut)
		if parseErr != nil {
			return 0, 0, false, parseErr
		}
		up += nftUp
		down += nftDown
		found = found || nftFound
	}

	if probeFirewall() == firewallIptables {
		for _, bin := range []string{"iptables", "ip6tables"} {
			for _, chain := range []string{"PREROUTING", "OUTPUT"} {
				out, runErr := runCommandWithCapsOutput(bin, "-t", "mangle", "-L", chain, "-n", "-v", "-x")
				if runErr != nil {
					logger.Warn("bypass: cannot read", bin, chain, ":", runErr)
					continue
				}
				chainUp, chainDown, chainFound := parseIptablesCounters(out)
				up += chainUp
				down += chainDown
				found = found || chainFound
			}
		}
	}

	return up, down, found, nil
}

type nftCounterBytes struct {
	Name  string `json:"name"`
	Bytes int64  `json:"bytes"`
}

type nftRule struct {
	Comment string                       `json:"comment"`
	Expr    []map[string]json.RawMessage `json:"expr"`
}

// parseNftCounters sums counters referenced by wt-bypass-* rules. Both
// anonymous counters (counter.bytes in expr) and named counter objects are
// supported; the latter carry their bytes in a separate object.
func parseNftCounters(raw string) (up, down int64, found bool, err error) {
	var payload struct {
		Nftables []map[string]json.RawMessage `json:"nftables"`
	}
	if err := json.Unmarshal([]byte(raw), &payload); err != nil {
		return 0, 0, false, fmt.Errorf("parse nft json: %w", err)
	}

	named := map[string]int64{}
	for _, object := range payload.Nftables {
		value, has := object["counter"]
		if !has {
			continue
		}
		var counter nftCounterBytes
		if json.Unmarshal(value, &counter) == nil && counter.Name != "" {
			named[counter.Name] = counter.Bytes
		}
	}

	for _, object := range payload.Nftables {
		value, has := object["rule"]
		if !has {
			continue
		}
		var rule nftRule
		if json.Unmarshal(value, &rule) != nil {
			continue
		}
		var bytes int64
		for _, expression := range rule.Expr {
			counter, has := expression["counter"]
			if !has {
				continue
			}
			var anonymous nftCounterBytes
			if json.Unmarshal(counter, &anonymous) == nil {
				bytes += anonymous.Bytes
				continue
			}
			var name string
			if json.Unmarshal(counter, &name) == nil {
				bytes += named[name]
			}
		}
		switch rule.Comment {
		case bypassUpComment:
			up += bytes
			found = true
		case bypassDownComment:
			down += bytes
			found = true
		}
	}
	return up, down, found, nil
}

// parseIptablesCounters reads byte counters from iptables -L -n -v -x output.
// Only rules carrying our comments are counted.
func parseIptablesCounters(raw string) (up, down int64, found bool) {
	for _, line := range strings.Split(raw, "\n") {
		if !strings.Contains(line, "/* "+bypassUpComment+" */") &&
			!strings.Contains(line, "/* "+bypassDownComment+" */") {
			continue
		}
		fields := strings.Fields(line)
		if len(fields) < 2 {
			continue
		}
		bytes, err := strconv.ParseInt(fields[1], 10, 64)
		if err != nil {
			continue
		}
		found = true
		if strings.Contains(line, "/* "+bypassUpComment+" */") {
			up += bytes
		} else {
			down += bytes
		}
	}
	return up, down, found
}
