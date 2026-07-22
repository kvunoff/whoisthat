package tunmode

import (
	"fmt"
	"strings"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	"whoisthat-core/utils"
)

// Split tunnelling routes a chosen set of apps differently from the rest of the
// system. Apps are launched via `whoisthat run <app>`, which drops them into a
// dedicated systemd --user scope under whoisthat-split.slice; nftables then
// matches their sockets by cgroup v2 membership and applies an fwmark, and
// policy routing sends the marked packets down the right path.
//
// Two modes:
//
//	exclude — the split apps BYPASS the tunnel. They get mark 1, which routes via
//	          table 100 (the same table addFwmarkRouting already points at the
//	          physical gateway for xray). Everything else goes through the TUN.
//
//	include — ONLY the split apps use the tunnel. They get mark 2, which routes
//	          via table 200 (default dev tun). Everything else uses the main
//	          table (physical) — so in include mode NO system-wide default TUN
//	          route is installed (see TunModeManager.Start).
const (
	splitSliceName    = "whoisthat-split.slice"
	splitExcludeMark  = 1
	splitExcludeTable = 100
	splitIncludeMark  = 2
	splitIncludeTable = 200
)

// splitCgroupRelPath is the cgroup v2 path (relative to the cgroup root — the
// form nftables `socket cgroupv2` matches on) of the slice that `whoisthat run`
// launches apps into. systemd nests a --user manager's slices under
// user.slice/user-<uid>.slice/user@<uid>.service, and our transient scopes sit
// one level below the slice, so a match on the slice catches every launched app.
func splitCgroupRelPath(uid int) string {
	return fmt.Sprintf("user.slice/user-%d.slice/user@%d.service/%s", uid, uid, splitSliceName)
}

func splitCgroupAbsPath(uid int) string {
	return "/sys/fs/cgroup/" + splitCgroupRelPath(uid)
}

// splitCgroupLevel is the nftables cgroupv2 "level" of relPath: the depth of the
// slice directory below the cgroup root. Derived from the path so it stays
// correct if the layout ever gains or loses a component.
func splitCgroupLevel(relPath string) int {
	return strings.Count(relPath, "/") + 1
}

// buildSplitSetupScript returns the shell script that installs split-tunnel
// marking + routing for the given mode. Returns "" for "off"/unknown modes.
// Pure text so it is unit-testable without root or a live cgroup tree.
func buildSplitSetupScript(mode string, uid int, tunName string, hasV6 bool) string {
	rel := splitCgroupRelPath(uid)
	abs := splitCgroupAbsPath(uid)
	level := splitCgroupLevel(rel)

	var mark int
	switch mode {
	case "exclude":
		mark = splitExcludeMark
	case "include":
		mark = splitIncludeMark
	default:
		return ""
	}

	var b strings.Builder
	b.WriteString("set -e\n")
	// Pre-create the slice cgroup so the nft rule can reference it before any app
	// has been launched into it. systemd-run --user --slice adopts it later.
	fmt.Fprintf(&b, "mkdir -p %q 2>/dev/null || true\n", abs)

	// Mark packets from sockets in the split slice (and its descendant scopes).
	// A `route` hook is required so setting the mark triggers a re-route of
	// locally-generated traffic — mirrors the conntrack-bypass mangle chain.
	b.WriteString("nft add table inet whoisthat_split 2>/dev/null || true\n")
	b.WriteString("nft 'add chain inet whoisthat_split output { type route hook output priority -150; policy accept; }' 2>/dev/null || true\n")
	b.WriteString("nft flush chain inet whoisthat_split output\n")
	fmt.Fprintf(&b, "nft add rule inet whoisthat_split output socket cgroupv2 level %d %q meta mark set %d\n", level, rel, mark)

	if mode == "include" {
		// Marked traffic -> table 200 -> default dev tun. Unmarked traffic falls
		// through to the main table (physical), so it bypasses the tunnel.
		fmt.Fprintf(&b, "ip rule add fwmark %d table %d 2>/dev/null || true\n", mark, splitIncludeTable)
		fmt.Fprintf(&b, "ip route replace default dev %s table %d\n", tunName, splitIncludeTable)
		if hasV6 {
			fmt.Fprintf(&b, "ip -6 rule add fwmark %d table %d 2>/dev/null || true\n", mark, splitIncludeTable)
			fmt.Fprintf(&b, "ip -6 route replace default dev %s table %d\n", tunName, splitIncludeTable)
		}
	}
	// exclude mode needs no extra routing: mark 1 already resolves to table 100
	// (addFwmarkRouting -> physical gateway), installed during TUN start.

	return b.String()
}

// buildSplitTeardownScript removes everything buildSplitSetupScript can create.
// Idempotent: a clean no-op when nothing exists. The whoisthat_split table drop
// covers both modes' nft state; the table-200 cleanup covers include mode.
func buildSplitTeardownScript() string {
	return fmt.Sprintf(`nft delete table inet whoisthat_split 2>/dev/null || true
ip rule del fwmark %d table %d 2>/dev/null || true
ip route flush table %d 2>/dev/null || true
ip -6 rule del fwmark %d table %d 2>/dev/null || true
ip -6 route flush table %d 2>/dev/null || true
`, splitIncludeMark, splitIncludeTable, splitIncludeTable, splitIncludeMark, splitIncludeTable, splitIncludeTable)
}

func (t *TunModeManager) applySplitRules() error {
	cfg := appconfig.GetConfig().SplitTunnel
	if cfg.Mode == "" || cfg.Mode == "off" {
		return nil
	}
	uid := utils.RealUserUid()
	if uid <= 0 {
		logger.Warn("split: no real login user detected; split tunnel needs a user session (skipping)")
		return nil
	}
	script := buildSplitSetupScript(cfg.Mode, uid, t.tun_name, t.tun_ipv6 != "")
	if script == "" {
		return nil
	}
	logger.Infof("split: applying %q mode rules (uid=%d)", cfg.Mode, uid)
	_, err := runScriptWithSh(script)
	return err
}

func (t *TunModeManager) removeSplitRules() error {
	_, err := runScriptWithSh(buildSplitTeardownScript())
	return err
}

// ReapplySplit reconciles live routing with the current split config while TUN
// is running. It is a no-op when TUN is down (the new config applies on the next
// Start). Because include mode removes the system-wide default TUN route while
// exclude/off keep it, this also toggles that route to match the new mode.
func (t *TunModeManager) ReapplySplit() error {
	t.mu.Lock()
	defer t.mu.Unlock()
	if !t.IsEnabled {
		return nil
	}
	t.removeSplitRules()

	mode := appconfig.GetConfig().SplitTunnel.Mode
	if mode == "include" {
		deleteTunIpRoute(t.tun_name, t.tun_ip)
		if t.tun_ipv6 != "" {
			deleteTunIpRoute6(t.tun_name, t.tun_ipv6)
		}
	} else {
		// Delete-then-add so this is idempotent when the route already exists
		// (e.g. exclude -> exclude reapply); setupTunIpRoute uses `ip route add`
		// which would otherwise fail with "file exists".
		deleteTunIpRoute(t.tun_name, t.tun_ip)
		if err := setupTunIpRoute(t.tun_name, t.tun_ip); err != nil {
			return fmt.Errorf("split: failed to restore tun default route: %w", err)
		}
		if t.tun_ipv6 != "" {
			deleteTunIpRoute6(t.tun_name, t.tun_ipv6)
			setupTunIpRoute6(t.tun_name, t.tun_ipv6)
		}
	}
	return t.applySplitRules()
}
