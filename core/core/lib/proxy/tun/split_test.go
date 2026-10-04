package tunmode

import (
	"strings"
	"testing"
)

// The cgroup paths and their nftables "level" must stay in sync: level is the depth
// of the slice dir below the cgroup root, so it equals the number of path
// components. If the layout changes, level must track it automatically.
func TestSplitCgroupLevelMatchesPathDepth(t *testing.T) {
	paths := splitCgroupRelPaths(1000)
	if len(paths) != 2 {
		t.Fatalf("expected 2 split cgroup candidate paths, got %d", len(paths))
	}

	// 1. whoisthat_split.slice (level 4, no hyphens, top-level user slice)
	relUnderscore := paths[0]
	wantComponentsUnderscore := strings.Count(relUnderscore, "/") + 1
	if got := splitCgroupLevel(relUnderscore); got != wantComponentsUnderscore || got != 4 {
		t.Errorf("splitCgroupLevel(%q) = %d, want %d (level 4)", relUnderscore, got, wantComponentsUnderscore)
	}

	// 2. whoisthat.slice/whoisthat-split.slice (level 5, subslice)
	relHyphen := paths[1]
	wantComponentsHyphen := strings.Count(relHyphen, "/") + 1
	if got := splitCgroupLevel(relHyphen); got != wantComponentsHyphen || got != 5 {
		t.Errorf("splitCgroupLevel(%q) = %d, want %d (level 5)", relHyphen, got, wantComponentsHyphen)
	}
}

// Exclude mode marks split-app sockets with mark 1, which reuses the existing
// table-100 fwmark routing (physical gateway). It must NOT install its own
// routing table or touch the system default route.
func TestSplitExcludeModeReusesTable100(t *testing.T) {
	script := buildSplitSetupScript("exclude", 1000, "whoisthattun", true)

	if !strings.Contains(script, "nft add table inet whoisthat_split") {
		t.Error("exclude script must create the whoisthat_split nft table")
	}
	// The marking rule must target the cgroup and set the exclude mark.
	if !strings.Contains(script, "socket cgroupv2 level") {
		t.Error("exclude script must match sockets by cgroupv2 membership")
	}
	if !strings.Contains(script, "meta mark set 1") {
		t.Errorf("exclude script must set mark %d, script: %s", splitExcludeMark, script)
	}
	if !strings.Contains(script, "nft 'add rule inet whoisthat_split output socket cgroupv2 level 4 \"user.slice/user-1000.slice/user@1000.service/whoisthat_split.slice\" meta mark set 1'") {
		t.Errorf("exclude script missing level 4 rule: %s", script)
	}
	if !strings.Contains(script, "nft 'add rule inet whoisthat_split output socket cgroupv2 level 5 \"user.slice/user-1000.slice/user@1000.service/whoisthat.slice/whoisthat-split.slice\" meta mark set 1'") {
		t.Errorf("exclude script missing level 5 rule: %s", script)
	}
	if !strings.Contains(script, "nft 'add rule inet whoisthat_split postrouting meta mark 1 oifname != \"whoisthattun\" oifname != \"lo\" masquerade'") {
		t.Errorf("exclude script missing masquerade rule: %s", script)
	}
	// Exclude mode must not create its own routing table — it piggybacks on 100.
	if strings.Contains(script, "table 200") {
		t.Error("exclude script must not reference the include-mode table 200")
	}
	if strings.Contains(script, "ip rule add fwmark") {
		t.Error("exclude script must not add its own fwmark rule (reuses table 100)")
	}
}

// When physical interface lacks IPv6, exclude mode must blackhole mark 1 IPv6
// so dual-stack apps immediately fall back to physical IPv4 instead of leaking to TUN.
func TestSplitExcludeModeFallbackV6WhenAbsent(t *testing.T) {
	script := buildSplitSetupScript("exclude", 1000, "whoisthattun", false)
	if !strings.Contains(script, "ip -6 rule add fwmark 1 table 100") {
		t.Errorf("exclude script must add fwmark 1 v6 rule when hasV6=false, script: %s", script)
	}
	if !strings.Contains(script, "ip -6 route replace unreachable default table 100") {
		t.Errorf("exclude script must add unreachable v6 route when hasV6=false, script: %s", script)
	}
}

// Include mode is the inverted model: only split apps use the tunnel. It marks
// with mark 2 and installs a dedicated table whose default route is the TUN dev.
func TestSplitIncludeModeInstallsTunTable(t *testing.T) {
	script := buildSplitSetupScript("include", 1000, "whoisthattun", true)

	if !strings.Contains(script, "meta mark set 2") {
		t.Errorf("include script must set mark %d", splitIncludeMark)
	}
	if !strings.Contains(script, "nft 'add rule inet whoisthat_split postrouting meta mark 2 oifname \"whoisthattun\" masquerade'") {
		t.Errorf("include script missing include masquerade rule: %s", script)
	}
	if !strings.Contains(script, "ip rule add fwmark 2 table 200") {
		t.Error("include script must route mark 2 via table 200")
	}
	if !strings.Contains(script, "ip route replace default dev whoisthattun table 200") {
		t.Errorf("include script must point table 200 default at the tun dev, script: %s", script)
	}
	// IPv6 branch must be present when hasV6 is true.
	if !strings.Contains(script, "ip -6 rule add fwmark 2 table 200") {
		t.Error("include script must install the IPv6 fwmark rule when v6 is available")
	}
}

// When there is no IPv6 default, include mode must not emit v6 routing commands
// (they would fail on a v4-only host).
func TestSplitIncludeModeSkipsV6WhenAbsent(t *testing.T) {
	script := buildSplitSetupScript("include", 1000, "whoisthattun", false)
	if strings.Contains(script, "ip -6 ") {
		t.Errorf("include script must omit IPv6 commands when hasV6=false, script: %s", script)
	}
}

// Unknown / off modes must produce no script at all (no rules installed).
func TestSplitOffModeProducesNoScript(t *testing.T) {
	for _, mode := range []string{"off", "", "bogus"} {
		if s := buildSplitSetupScript(mode, 1000, "whoisthattun", true); s != "" {
			t.Errorf("mode %q must yield an empty script, got: %s", mode, s)
		}
	}
}

// Teardown must remove everything setup can create and be a clean no-op when
// nothing exists (every command guarded).
func TestSplitTeardownIsIdempotentAndComplete(t *testing.T) {
	script := buildSplitTeardownScript()

	if !strings.Contains(script, "nft delete table inet whoisthat_split") {
		t.Error("teardown must drop the whoisthat_split nft table")
	}
	if !strings.Contains(script, "ip rule del fwmark 2 table 200") {
		t.Error("teardown must remove the include-mode fwmark rule")
	}
	if !strings.Contains(script, "ip route flush table 200") {
		t.Error("teardown must flush the include-mode routing table")
	}
	for _, line := range strings.Split(script, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" {
			continue
		}
		if !strings.Contains(trimmed, "|| true") {
			t.Errorf("unguarded command in split teardown (not idempotent): %q", trimmed)
		}
	}
}
