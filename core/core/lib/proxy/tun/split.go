package tunmode

import (
	"fmt"
	"os"
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
	splitSliceNameUnderscore = "whoisthat_split.slice"
	splitSliceNameHyphen     = "whoisthat-split.slice"
	splitExcludeMark         = 1
	splitExcludeTable        = 100
	splitIncludeMark         = 2
	splitIncludeTable        = 200
)

// splitCgroupRelPaths returns the candidate cgroup v2 paths (relative to the
// cgroup root) where apps launched by `whoisthat run` may end up:
// 1. whoisthat_split.slice (level 4, no hyphens, directly under user@<uid>.service)
// 2. whoisthat-split.slice (level 5, systemd treats hyphens as sub-slice hierarchy: whoisthat.slice/whoisthat-split.slice)
// 3. whoisthat-<uid> (level 1, the native root-created cgroup used without systemd)
// We install rules for every path that exists so both slice naming conventions
// and the native backend work transparently.
func splitCgroupRelPaths(uid int) []string {
	return []string{
		fmt.Sprintf("user.slice/user-%d.slice/user@%d.service/%s", uid, uid, splitSliceNameUnderscore),
		fmt.Sprintf("user.slice/user-%d.slice/user@%d.service/whoisthat.slice/%s", uid, uid, splitSliceNameHyphen),
		fmt.Sprintf("whoisthat-%d", uid),
	}
}

// splitRunScript is a seam so tests can observe/inject script execution.
var splitRunScript = runScriptWithSh

// cgroupRoot returns the cgroup v2 mount point. The override is test-only: in
// normal operation an environment variable must not be able to redirect root
// file operations (mkdir) outside the real cgroup tree.
func cgroupRoot() string {
	if os.Getenv("WHOISTHAT_TEST_MODE") == "1" {
		if root := os.Getenv("WHOISTHAT_CGROUP_ROOT"); root != "" {
			return root
		}
	}
	return "/sys/fs/cgroup"
}

// splitCgroupLevel is the nftables cgroupv2 "level" of relPath: the depth of the
// slice directory below the cgroup root. Derived from the path so it stays
// correct if the layout ever gains or loses a component.
func splitCgroupLevel(relPath string) int {
	return strings.Count(relPath, "/") + 1
}

// buildSplitSetupScript returns the shell script that installs split-tunnel
// marking + routing for the given mode. Returns "" for "off"/unknown modes.
//
// physIface/physGateway identify the physical link exclude mode must route
// marked traffic through.
//
// tunV6: the TUN device carries IPv6 (include mode routes v6 into the tunnel).
// physV6: the physical link has an IPv6 default gateway (exclude mode points
// excluded apps at the physical link through table 100; without v6 they must
// be blackholed instead of leaking into the tunnel).
//
// Pure text so it is unit-testable without root or a live cgroup tree.
func buildSplitSetupScript(mode string, uid int, tunName, physIface, physGateway string, tunV6, physV6 bool) string {
	paths := splitCgroupRelPaths(uid)

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
	// Fail before touching nft when split cannot possibly work, so the TUN
	// start gets an actionable error instead of a half-installed ruleset.
	fmt.Fprintf(&b, "root=%q\n", cgroupRoot())
	b.WriteString("[ -f \"$root/cgroup.controllers\" ] || { echo \"whoisthat: cgroup v2 not mounted; split-tunnel unavailable\" >&2; exit 1; }\n")
	// Pre-create the slice cgroups so nft rules can reference them before any app
	// has been launched into them. systemd-run --user --slice adopts them later.
	b.WriteString("for rel in")
	for _, rel := range paths {
		fmt.Fprintf(&b, " %q", rel)
	}
	b.WriteString("; do mkdir -p \"$root/$rel\" 2>/dev/null || true; done\n")
	b.WriteString("ok=0\nfor rel in")
	for _, rel := range paths {
		fmt.Fprintf(&b, " %q", rel)
	}
	b.WriteString("; do [ -d \"$root/$rel\" ] && ok=1; done\n")
	b.WriteString("[ \"$ok\" = 1 ] || { echo \"whoisthat: split cgroup not available; run: sudo /usr/lib/whoisthat/cgroup-setup $USER\" >&2; exit 1; }\n")

	// Mark packets from sockets in the split slice (and its descendant scopes).
	// A `route` hook is required so setting the mark triggers a re-route of
	// locally-generated traffic — mirrors the conntrack-bypass mangle chain.
	b.WriteString("nft add table inet whoisthat_split 2>/dev/null || true\n")
	b.WriteString("nft 'add chain inet whoisthat_split output { type route hook output priority -150; policy accept; }' 2>/dev/null || true\n")
	b.WriteString("nft flush chain inet whoisthat_split output\n")
	for _, rel := range paths {
		level := splitCgroupLevel(rel)
		// nft resolves the cgroup path when the rule is added: a rule for a
		// path that does not exist aborts the whole script, so only reference
		// directories that are really there.
		fmt.Fprintf(&b, "[ -d \"$root/%s\" ] && nft 'add rule inet whoisthat_split output socket cgroupv2 level %d \"%s\" meta mark set %d'\n", rel, level, rel, mark)
	}

	// Sockets bound to the wrong source IP before output routing must be
	// masqueraded on the egress interface so upstream routers/gateways accept them.
	b.WriteString("nft 'add chain inet whoisthat_split postrouting { type nat hook postrouting priority 100; policy accept; }' 2>/dev/null || true\n")
	b.WriteString("nft flush chain inet whoisthat_split postrouting\n")
	if mode == "exclude" {
		fmt.Fprintf(&b, "nft 'add rule inet whoisthat_split postrouting meta mark %d oifname != %q oifname != \"lo\" masquerade'\n", mark, tunName)
	} else if mode == "include" {
		fmt.Fprintf(&b, "nft 'add rule inet whoisthat_split postrouting meta mark %d oifname %q masquerade'\n", mark, tunName)
	}

	if mode == "include" {
		// Marked traffic -> table 200 -> default dev tun. Unmarked traffic falls
		// through to the main table (physical), so it bypasses the tunnel.
		fmt.Fprintf(&b, "ip rule add fwmark %d table %d 2>/dev/null || true\n", mark, splitIncludeTable)
		fmt.Fprintf(&b, "ip route replace default dev %s table %d\n", tunName, splitIncludeTable)
		if tunV6 {
			fmt.Fprintf(&b, "ip -6 rule add fwmark %d table %d 2>/dev/null || true\n", mark, splitIncludeTable)
			fmt.Fprintf(&b, "ip -6 route replace default dev %s table %d\n", tunName, splitIncludeTable)
		}
	} else if mode == "exclude" {
		// Re-assert the mark-1 policy route instead of trusting the one that
		// addFwmarkRouting installed at TUN start: a startup reconcile, a
		// network reset or a reapply can drop it while the TUN stays up, and
		// then excluded apps silently go through the tunnel.
		if physIface != "" && physGateway != "" {
			fmt.Fprintf(&b, "ip rule add fwmark %d table %d 2>/dev/null || true\n", mark, splitExcludeTable)
			fmt.Fprintf(&b, "ip route replace %s/32 dev %s table %d\n", physGateway, physIface, splitExcludeTable)
			fmt.Fprintf(&b, "ip route replace default via %s table %d\n", physGateway, splitExcludeTable)
		}
		if !physV6 {
			// Physical interface has no IPv6 default gateway. Excluded apps must NOT
			// leak into the tunnel's default IPv6 route. Route mark 1 IPv6 packets
			// to unreachable so dual-stack apps fall back cleanly to IPv4 (physical).
			b.WriteString("ip -6 rule add fwmark 1 table 100 2>/dev/null || true\n")
			b.WriteString("ip -6 route replace unreachable default table 100 2>/dev/null || true\n")
		}
	}

	return b.String()
}

// buildSplitTeardownScript removes everything buildSplitSetupScript can create.
// Idempotent: a clean no-op when nothing exists. The whoisthat_split table drop
// covers both modes' nft state; the table-200 cleanup covers include mode.
//
// Table 100 is shared with Xray's fwmark routing (addFwmarkRouting6), so it is
// only flushed when setup installed the v4-only unreachable fallback there
// (physV6 == false). Flushing it unconditionally breaks Xray's direct IPv6
// traffic on a live TUN.
func buildSplitTeardownScript(physV6 bool) string {
	var b strings.Builder
	b.WriteString("nft delete table inet whoisthat_split 2>/dev/null || true\n")
	fmt.Fprintf(&b, "ip rule del fwmark %d table %d 2>/dev/null || true\n", splitIncludeMark, splitIncludeTable)
	fmt.Fprintf(&b, "ip route flush table %d 2>/dev/null || true\n", splitIncludeTable)
	fmt.Fprintf(&b, "ip -6 rule del fwmark %d table %d 2>/dev/null || true\n", splitIncludeMark, splitIncludeTable)
	fmt.Fprintf(&b, "ip -6 route flush table %d 2>/dev/null || true\n", splitIncludeTable)
	if !physV6 {
		fmt.Fprintf(&b, "ip -6 rule del fwmark %d table 100 2>/dev/null || true\n", splitExcludeMark)
		b.WriteString("ip -6 route flush table 100 2>/dev/null || true\n")
	}
	return b.String()
}

// ValidateSplitUid rejects split configs that cannot work. A non-positive uid
// used to be skipped with a warning while the TUN still came up: in include
// mode that sent every app through the tunnel and in exclude mode none, which
// is silently wrong. Callers may validate before persisting the mode.
func ValidateSplitUid(mode string, uid int) error {
	if mode == "" || mode == "off" {
		return nil
	}
	if uid <= 0 {
		return fmt.Errorf("split: no real login user detected; split tunnel needs a user session")
	}
	return nil
}

func (t *TunModeManager) applySplitRules() error {
	return t.applySplitRulesFor(appconfig.GetConfig().SplitTunnel.Mode)
}

func (t *TunModeManager) applySplitRulesFor(mode string) error {
	if mode == "" || mode == "off" {
		return nil
	}
	uid := utils.RealUserUid()
	if err := ValidateSplitUid(mode, uid); err != nil {
		logger.Warn(err.Error())
		return err
	}
	script := buildSplitSetupScript(
		mode,
		uid,
		t.tun_name,
		t.default_interface,
		t.default_interface_ip,
		t.tun_ipv6 != "",
		t.default_interface_ipv6 != "",
	)
	if script == "" {
		return nil
	}
	logger.Infof("split: applying %q mode rules (uid=%d)", mode, uid)
	_, err := splitRunScript(script)
	if err != nil {
		// The stderr text carries the actionable reason (cgroup missing, nft
		// unsupported). Log its first line: the rest is shell noise.
		reason := strings.TrimSpace(err.Error())
		if idx := strings.IndexByte(reason, '\n'); idx >= 0 {
			reason = reason[:idx]
		}
		logger.Warn("split: failed to apply rules:", reason)
	}
	return err
}

func (t *TunModeManager) removeSplitRules() error {
	_, err := splitRunScript(buildSplitTeardownScript(t.default_interface_ipv6 != ""))
	return err
}

// ReapplySplit reconciles live routing with the current split config while TUN
// is running. It is a no-op when TUN is down (the new config applies on the next
// Start). Because include mode removes the system-wide default TUN route while
// exclude/off keep it, this also toggles that route to match the new mode.
//
// oldMode is the mode that was active before the config change. If the new mode
// cannot be applied, routing is restored to oldMode; if even that fails the TUN
// is stopped, because half-applied split rules leak traffic.
func (t *TunModeManager) ReapplySplit(oldMode string) error {
	t.mu.Lock()
	defer t.mu.Unlock()
	if !t.IsEnabled {
		return nil
	}

	mode := appconfig.GetConfig().SplitTunnel.Mode
	if err := t.reapplySplitLocked(mode); err != nil {
		if restoreErr := t.reapplySplitLocked(oldMode); restoreErr != nil {
			logger.Warn("split: cannot restore previous routing, stopping TUN:", restoreErr)
			t.stopLocked()
			return fmt.Errorf("split: apply failed (%v) and restore failed (%v); tun disabled", err, restoreErr)
		}
		return err
	}
	return nil
}

func (t *TunModeManager) reapplySplitLocked(mode string) error {
	// Teardown is idempotent; its failure must not prevent the new rules from
	// being installed.
	if err := t.removeSplitRules(); err != nil {
		logger.Warn("split: failed to remove old rules:", err)
	}

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
	return t.applySplitRulesFor(mode)
}

// stopLocked is Stop() without taking t.mu (callers already hold it).
func (t *TunModeManager) stopLocked() {
	_ = t.clearNetworkRules()
	t.dnsHijack = DnsHijack{}
	t.IsEnabled = false
	t.StatusChanged <- t.IsEnabled
}
