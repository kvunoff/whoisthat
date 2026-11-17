package tunmode

import (
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"os/exec"
	"regexp"
	"strings"
	"whoisthat-core/utils"

	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/netmark"
)

// Kill-switch: an independent filter chain that drops every locally-generated
// packet except the explicitly whitelisted classes of traffic. It is installed
// while the user is "protected" (connected, or disconnected after a drop) and
// removed only when the user turns the kill-switch off.
//
// The whitelist intentionally mirrors what the documentation promises:
//
//   - loopback (127.0.0.0/8, ::1) and the `lo` interface
//   - private / link-local / multicast LAN ranges (printers, NAS, mDNS, NDP)
//   - the configured DNS servers (resolution before/after TUN is up)
//   - sockets marked with the "direct" fwmark (Xray's freedom outbound and
//     split-exclude traffic). Setting SO_MARK requires CAP_NET_ADMIN, which in
//     the rootless setup only whoisthat-core holds.
//   - the TUN interface itself, when one is configured. Everything routed into
//     TUN is encapsulated by the tunnel; the rule stops matching as soon as the
//     interface is deleted on disconnect, so the fail-closed behaviour stands.
//   - the active VPN server IPs (handshake / reconnect)
//   - the reserved DNS hijack sink (see FAKEDNS plan, Phase 1)
//
// Everything else is dropped.

const (
	// killSwitchDirectMark is the fwmark used for "go out via the physical
	// gateway" sockets: Xray's freedom outbound and split-exclude apps.
	// Keep in sync with splitExcludeMark in split.go.
	killSwitchDirectMark = netmark.DirectMark

	// killSwitchDNSSink4/6 are the reserved addresses the TUN DNS hijack will
	// redirect port 53 to. Nothing listens on them directly; they must stay
	// reachable so DNS keeps working with the kill-switch armed.
	killSwitchDNSSink4 = DNSSink4
	killSwitchDNSSink6 = DNSSink6
)

// killSwitchV4/6 are the fixed LAN ranges always allowed through.
var (
	killSwitchV4 = []string{
		"127.0.0.0/8",
		"10.0.0.0/8",
		"172.16.0.0/12",
		"192.168.0.0/16",
		"169.254.0.0/16",
		"224.0.0.0/4",
	}
	killSwitchV6 = []string{
		"::1/128",
		"fc00::/7",
		"fe80::/10",
		"ff00::/8",
	}
)

// validTunNameRe matches the names AppConfig accepts for the TUN device. The
// name is interpolated into root shell scripts, so we re-validate it here even
// though sanitizeTunName already did.
var validTunNameRe = regexp.MustCompile(`^[A-Za-z][A-Za-z0-9_-]*$`)

// SetupKillSwitchBlock installs the drop-all-except-whitelist chain.
//
// proxyIPs are the resolved addresses of the active VPN server, tunName the
// configured TUN interface (empty when TUN is unused), dnsServers the resolver
// list from the app config.
func SetupKillSwitchBlock(proxyIPs []string, tunName string, dnsServers []string) error {
	direct := currentDirectExceptions()
	d4, d6 := splitNetsValid(direct)
	logger.Infof("kill-switch: direct-rule targets %v", direct)
	// Visible at arm time: a missing exception shows up as a low count.
	logger.Infof("kill-switch: %d direct-rule accept exceptions (v4 %d, v6 %d)", len(d4)+len(d6), len(d4), len(d6))
	// Tripwire: an empty target list usually means the extractor saw no rules
	// (or none resolved) rather than that none exist, and the arm then drops the
	// excluded hosts. Warn instead of quietly arming with nothing.
	if len(direct) == 0 {
		logger.Warn("kill-switch: no direct-rule targets resolved; arming with no exceptions")
	}
	// Excluded hosts are routed off the TUN so they keep the physical path even
	// though the TUN owns the default route. Best-effort: a missing route only
	// costs latency, the accept exception above still keeps them alive.
	if err := addDirectHostRoutes(d4, d6, tunName); err != nil {
		logger.Warn("kill-switch: could not pin direct routes off the tunnel:", err)
	}

	// K-2: refuse to arm over a DoT-only resolver we cannot neutralise.
	if err := neutraliseDoT(tunName); err != nil {
		return err
	}
	switch probeFirewall() {
	case firewallNftables:
		return setupKillSwitchBlockNftables(proxyIPs, tunName, dnsServers, direct...)
	default:
		return setupKillSwitchBlockIptables(proxyIPs, tunName, dnsServers, direct...)
	}
}

// RemoveKillSwitchBlock tears the chain down. Idempotent.
//
// Both backends are attempted: an nft-only host may still carry legacy
// iptables chains from an older build (and vice versa). The call reports an
// error unless the rules are verifiably gone, so callers never claim "off"
// while traffic is still being dropped.
func RemoveKillSwitchBlock() error {
	removeDirectHostRoutes()
	nftErr := removeNftKillSwitch()
	iptErr := removeIptablesKillSwitch()

	armed, probeErr := killSwitchInstalled()
	if probeErr != nil {
		return fmt.Errorf("cannot verify kill-switch removal: %w", probeErr)
	}
	if armed {
		return fmt.Errorf("kill-switch rules are still installed (nft: %v, iptables: %v)", nftErr, iptErr)
	}
	if err := restoreDoT(); err != nil {
		logger.Warnf("kill-switch: %v", err)
	}
	return nil
}

func removeNftKillSwitch() error {
	_, err := killSwitchRunScript("nft delete table inet whoisthat_ks 2>/dev/null || true")
	return err
}

func removeIptablesKillSwitch() error {
	_, err := killSwitchRunScript(buildKillSwitchIptablesTeardownScript())
	return err
}

// buildKillSwitchIptablesTeardownScript removes every jump (older builds could
// leave more than one), flushes and deletes the chain for both families.
func buildKillSwitchIptablesTeardownScript() string {
	return `
for IPT in iptables ip6tables; do
    while "$IPT" -C OUTPUT -j WHOISTHAT_KS 2>/dev/null; do
        "$IPT" -D OUTPUT -j WHOISTHAT_KS
    done
    "$IPT" -F WHOISTHAT_KS 2>/dev/null || true
    "$IPT" -X WHOISTHAT_KS 2>/dev/null || true
done
`
}

// KillSwitchArmed reports whether the block is actually enforcing: for nftables
// the live ruleset must have the output chain with policy drop and at least one
// IPv4 and one IPv6 rule (K-3). A table that merely exists, an empty chain or a
// v4-only half install is NOT armed. The legacy iptables chains count as armed
// when the jump is present. A mixed host must not be reported as disarmed.
//
// A missing binary means that backend is not in use (not an error). An
// inspection that fails for another reason (permissions, broken binary,
// unparseable output) is returned as an error: callers must not treat it as
// "not armed".
func KillSwitchArmed() (bool, error) { return killSwitchProbeAll(false) }

// killSwitchInstalled reports whether ANY kill-switch state is present, even a
// half-built one that would still drop traffic. Used to verify removal.
func killSwitchInstalled() (bool, error) { return killSwitchProbeAll(true) }

func killSwitchProbeAll(anyPresence bool) (bool, error) {
	nftState, nftErr := nftKillSwitchState()
	v4Armed, v4Err := iptablesKillSwitchArmed("iptables")
	v6Armed, v6Err := iptablesKillSwitchArmed("ip6tables")

	nftHit := nftState.armed || (anyPresence && nftState.installed)
	if nftHit || v4Armed || v6Armed {
		return true, nil
	}
	for _, err := range []error{nftErr, v4Err, v6Err} {
		if err != nil {
			return false, err
		}
	}
	return false, nil
}

// Test seams: the probes are what turns "the firewall lies" into a testable
// error, so let unit tests simulate exit codes without real capabilities.
var (
	killSwitchLookPath  = utils.SafeLookPath
	killSwitchRun       = runCommandWithCaps
	killSwitchRunScript = runScriptWithSh
)

type nftKSState struct {
	installed bool // table or chain present in any form
	armed     bool // chain present, policy drop, v4 and v6 rules present
}

func nftKillSwitchState() (nftKSState, error) {
	if _, err := killSwitchLookPath("nft"); err != nil {
		return nftKSState{}, nil
	}
	out, err := killSwitchRun("nft", "-j", "list", "table", "inet", "whoisthat_ks")
	if err != nil {
		// nft uses strerror; runCommandWithCaps pins LC_ALL=C for this string.
		if strings.Contains(out, "No such file or directory") {
			return nftKSState{}, nil
		}
		return nftKSState{}, fmt.Errorf("cannot inspect nftables kill-switch table: %w: %s", err, strings.TrimSpace(out))
	}
	return parseNftKillSwitch(out)
}

// parseNftKillSwitch evaluates `nft -j list table inet whoisthat_ks` output.
func parseNftKillSwitch(out string) (nftKSState, error) {
	var doc struct {
		Nftables []struct {
			Chain *struct {
				Name   string `json:"name"`
				Policy string `json:"policy"`
			} `json:"chain"`
			Rule *struct {
				Chain string            `json:"chain"`
				Expr  []json.RawMessage `json:"expr"`
			} `json:"rule"`
		} `json:"nftables"`
	}
	if err := json.Unmarshal([]byte(out), &doc); err != nil {
		return nftKSState{}, fmt.Errorf("cannot parse nft kill-switch ruleset: %w", err)
	}
	st := nftKSState{installed: true}
	var chain, drop bool
	var v4, v6 int
	for _, item := range doc.Nftables {
		if c := item.Chain; c != nil && c.Name == "output" {
			chain = true
			drop = c.Policy == "drop"
		}
		if r := item.Rule; r != nil && r.Chain == "output" {
			switch nftRuleFamily(r.Expr) {
			case "ip":
				v4++
			case "ip6":
				v6++
			}
		}
	}
	st.armed = chain && drop && v4 > 0 && v6 > 0
	return st, nil
}

// nftRuleFamily returns "ip" or "ip6" when a rule matches on that payload
// protocol, otherwise "".
func nftRuleFamily(exprs []json.RawMessage) string {
	for _, raw := range exprs {
		var e struct {
			Match *struct {
				Left struct {
					Payload *struct {
						Protocol string `json:"protocol"`
					} `json:"payload"`
				} `json:"left"`
			} `json:"match"`
		}
		if json.Unmarshal(raw, &e) == nil && e.Match != nil && e.Match.Left.Payload != nil {
			if p := e.Match.Left.Payload.Protocol; p == "ip" || p == "ip6" {
				return p
			}
		}
	}
	return ""
}

func iptablesKillSwitchArmed(bin string) (bool, error) {
	if _, err := killSwitchLookPath(bin); err != nil {
		return false, nil
	}
	out, err := killSwitchRun(bin, "-C", "OUTPUT", "-j", "WHOISTHAT_KS")
	if err == nil {
		return true, nil
	}
	var exitErr *exec.ExitError
	if errors.As(err, &exitErr) {
		// iptables -C: 1 = rule missing, 2 = chain missing; both mean "not armed".
		if code := exitErr.ExitCode(); code == 1 || code == 2 {
			return false, nil
		}
	}
	return false, fmt.Errorf("%s -C OUTPUT -j WHOISTHAT_KS failed: %w: %s", bin, err, strings.TrimSpace(out))
}

// sanitizeTunName returns name when it is a safe interface name, otherwise "".
// Defense in depth: AppConfig already sanitizes the configured name, but the
// builders are also fed from other call sites.
func sanitizeTunName(name string) string {
	if name != "" && !validTunNameRe.MatchString(name) {
		logger.Warnf("kill-switch: ignoring invalid tun name %q", name)
		return ""
	}
	return name
}

// splitIPsValid splits a list into valid, de-duplicated IPv4 and IPv6 literals.
// Anything that does not parse as an IP is dropped: these values are embedded
// into a root shell script, so this is the injection boundary.
func splitIPsValid(ips []string) (v4, v6 []string) {
	seen := make(map[string]struct{}, len(ips))
	for _, raw := range ips {
		ip := net.ParseIP(strings.TrimSpace(raw))
		if ip == nil {
			if strings.TrimSpace(raw) != "" {
				logger.Warnf("kill-switch: ignoring invalid IP %q", raw)
			}
			continue
		}
		key := ip.String()
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		if ip.To4() != nil {
			v4 = append(v4, key)
		} else {
			v6 = append(v6, key)
		}
	}
	return v4, v6
}

// buildKillSwitchNftScript renders the nftables variant. Pure function so the
// rule set can be asserted in unit tests without touching the host firewall.
func buildKillSwitchNftScript(proxyIPs []string, tunName string, dnsServers []string, direct ...string) string {
	tunName = sanitizeTunName(tunName)
	proxy4, proxy6 := splitIPsValid(proxyIPs)
	dns4, dns6 := splitIPsValid(dnsServers)
	direct4, direct6 := splitNetsValid(direct)

	var b strings.Builder
	// K-6: one transaction. `add table/chain` are idempotent, and the kernel
	// applies or rejects the whole batch, so there is no window with a
	// drop-policy chain and an empty allowlist. If the batch is rejected a
	// previously armed table is untouched (atomic); a table that did not exist
	// before this attempt is deleted so nothing half-built remains.
	b.WriteString("PREV=0\n")
	b.WriteString("nft list table inet whoisthat_ks >/dev/null 2>&1 && PREV=1\n")
	b.WriteString("if ! nft -f - <<'WHOISTHAT_KS_EOF'\n")
	b.WriteString("add table inet whoisthat_ks\n")
	b.WriteString("add chain inet whoisthat_ks output { type filter hook output priority 0; policy drop; }\n")
	b.WriteString("flush chain inet whoisthat_ks output\n")

	b.WriteString("add rule inet whoisthat_ks output oif lo accept\n")
	for _, ip := range killSwitchV4 {
		fmt.Fprintf(&b, "add rule inet whoisthat_ks output ip daddr %s accept\n", ip)
	}
	for _, ip := range killSwitchV6 {
		fmt.Fprintf(&b, "add rule inet whoisthat_ks output ip6 daddr %s accept\n", ip)
	}
	// DHCP client broadcast (K-1): sent before the host has an address.
	b.WriteString("add rule inet whoisthat_ks output ip daddr 255.255.255.255 udp dport 67 accept\n")

	// Resolution must keep working while the tunnel is being (re)established.
	for _, ip := range append(append([]string{}, dns4...), killSwitchDNSSink4) {
		fmt.Fprintf(&b, "add rule inet whoisthat_ks output ip daddr %s accept\n", ip)
	}
	for _, ip := range append(append([]string{}, dns6...), killSwitchDNSSink6) {
		fmt.Fprintf(&b, "add rule inet whoisthat_ks output ip6 daddr %s accept\n", ip)
	}

	// Xray's freedom outbound / split-exclude sockets.
	fmt.Fprintf(&b, "add rule inet whoisthat_ks output meta mark %d accept\n", killSwitchDirectMark)

	if tunName != "" {
		fmt.Fprintf(&b, "add rule inet whoisthat_ks output oifname %q accept\n", tunName)
	}

	for _, ip := range proxy4 {
		fmt.Fprintf(&b, "add rule inet whoisthat_ks output ip daddr %s accept\n", ip)
	}
	for _, ip := range proxy6 {
		fmt.Fprintf(&b, "add rule inet whoisthat_ks output ip6 daddr %s accept\n", ip)
	}

	// Direct-rule destinations (and what they resolve to): the route picks the
	// path, the firewall only has to let them out. Still no rule for teardown.
	for _, ip := range direct4 {
		fmt.Fprintf(&b, "add rule inet whoisthat_ks output ip daddr %s accept\n", ip)
	}
	for _, ip := range direct6 {
		fmt.Fprintf(&b, "add rule inet whoisthat_ks output ip6 daddr %s accept\n", ip)
	}

	b.WriteString("add rule inet whoisthat_ks output drop\n")
	b.WriteString("WHOISTHAT_KS_EOF\n")
	b.WriteString("then\n")
	b.WriteString("    [ \"$PREV\" = 1 ] || nft delete table inet whoisthat_ks 2>/dev/null || true\n")
	b.WriteString("    echo 'kill-switch: nft batch rejected, nothing applied' >&2\n")
	b.WriteString("    exit 1\n")
	b.WriteString("fi\n")
	return b.String()
}

// buildKillSwitchIptablesScript renders the legacy iptables variant.
func buildKillSwitchIptablesScript(proxyIPs []string, tunName string, dnsServers []string, direct ...string) string {
	tunName = sanitizeTunName(tunName)
	proxy4, proxy6 := splitIPsValid(proxyIPs)
	dns4, dns6 := splitIPsValid(dnsServers)
	direct4, direct6 := splitNetsValid(direct)

	var b strings.Builder
	b.WriteString("set -e\n")
	// Remove every jump an older session may have left: one -D only removes
	// the first, and a still-referenced chain cannot be deleted.
	b.WriteString("for IPT in iptables ip6tables; do\n")
	b.WriteString("    while \"$IPT\" -C OUTPUT -j WHOISTHAT_KS 2>/dev/null; do\n")
	b.WriteString("        \"$IPT\" -D OUTPUT -j WHOISTHAT_KS\n")
	b.WriteString("    done\n")
	b.WriteString("    \"$IPT\" -F WHOISTHAT_KS 2>/dev/null || true\n")
	b.WriteString("    \"$IPT\" -X WHOISTHAT_KS 2>/dev/null || true\n")
	b.WriteString("done\n")

	// --- IPv4 ---
	b.WriteString("iptables -N WHOISTHAT_KS 2>/dev/null || iptables -F WHOISTHAT_KS\n")
	b.WriteString("iptables -A WHOISTHAT_KS -o lo -j ACCEPT\n")
	for _, ip := range killSwitchV4 {
		fmt.Fprintf(&b, "iptables -A WHOISTHAT_KS -d %s -j ACCEPT\n", ip)
	}
	// DHCP client broadcast (K-1): sent before the host has an address.
	b.WriteString("iptables -A WHOISTHAT_KS -d 255.255.255.255 -p udp --dport 67 -j ACCEPT\n")
	for _, ip := range append(append([]string{}, dns4...), killSwitchDNSSink4) {
		fmt.Fprintf(&b, "iptables -A WHOISTHAT_KS -d %s -j ACCEPT\n", ip)
	}
	fmt.Fprintf(&b, "iptables -A WHOISTHAT_KS -m mark --mark %d -j ACCEPT\n", killSwitchDirectMark)
	if tunName != "" {
		fmt.Fprintf(&b, "iptables -A WHOISTHAT_KS -o %s -j ACCEPT\n", tunName)
	}
	for _, ip := range proxy4 {
		fmt.Fprintf(&b, "iptables -A WHOISTHAT_KS -d %s -j ACCEPT\n", ip)
	}
	for _, ip := range direct4 {
		fmt.Fprintf(&b, "iptables -A WHOISTHAT_KS -d %s -j ACCEPT\n", ip)
	}
	b.WriteString("iptables -A WHOISTHAT_KS -j DROP\n")
	b.WriteString("iptables -I OUTPUT 1 -j WHOISTHAT_KS\n")

	// --- IPv6 ---
	b.WriteString("ip6tables -N WHOISTHAT_KS 2>/dev/null || ip6tables -F WHOISTHAT_KS\n")
	b.WriteString("ip6tables -A WHOISTHAT_KS -o lo -j ACCEPT\n")
	for _, ip := range killSwitchV6 {
		fmt.Fprintf(&b, "ip6tables -A WHOISTHAT_KS -d %s -j ACCEPT\n", ip)
	}
	for _, ip := range append(append([]string{}, dns6...), killSwitchDNSSink6) {
		fmt.Fprintf(&b, "ip6tables -A WHOISTHAT_KS -d %s -j ACCEPT\n", ip)
	}
	fmt.Fprintf(&b, "ip6tables -A WHOISTHAT_KS -m mark --mark %d -j ACCEPT\n", killSwitchDirectMark)
	if tunName != "" {
		fmt.Fprintf(&b, "ip6tables -A WHOISTHAT_KS -o %s -j ACCEPT\n", tunName)
	}
	for _, ip := range proxy6 {
		fmt.Fprintf(&b, "ip6tables -A WHOISTHAT_KS -d %s -j ACCEPT\n", ip)
	}
	for _, ip := range direct6 {
		fmt.Fprintf(&b, "ip6tables -A WHOISTHAT_KS -d %s -j ACCEPT\n", ip)
	}
	b.WriteString("ip6tables -A WHOISTHAT_KS -j DROP\n")
	b.WriteString("ip6tables -I OUTPUT 1 -j WHOISTHAT_KS\n")

	return b.String()
}

func setupKillSwitchBlockNftables(proxyIPs []string, tunName string, dnsServers []string, direct ...string) error {
	_, err := runScriptWithSh(buildKillSwitchNftScript(proxyIPs, tunName, dnsServers, direct...))
	return err
}

func setupKillSwitchBlockIptables(proxyIPs []string, tunName string, dnsServers []string, direct ...string) error {
	_, err := runScriptWithSh(buildKillSwitchIptablesScript(proxyIPs, tunName, dnsServers, direct...))
	return err
}
