package tunmode

import "fmt"

// buildReconcileScript returns an idempotent teardown script that removes any
// leftover whoisthat networking state from a previous core that exited without
// cleaning up — a hard `Q`, a crash, an OOM kill, or a power loss mid-TUN.
//
// It covers every artifact the core can create:
//   - nftables tables: whoisthat (DNS hijack v4/v6), whoisthat_ks (kill-switch),
//     whoisthat_mangle (conntrack bypass), whoisthat_split (split tunnel)
//   - iptables kill-switch chains (legacy backend)
//   - policy-routing rules pointing at table 100 (fwmark + uidrange) and
//     table 200 (include-mode split tunnel), plus their routes
//
// Every command is guarded so the script is a clean no-op when nothing exists.
// The `ip rule del table N` loops delete one matching rule per iteration and
// stop as soon as none remain (bounded to avoid an unbounded loop). This
// deletes fwmark AND uidrange rules regardless of their specific values, so it
// stays correct even if the dedicated UID differs from the crashed session's.
//
// The iptables DNS hijack lives in dedicated nat chains, so a crash cannot
// leave address-specific rules behind: removing the chain jump and flushing the
// chain is enough regardless of which sink or interface was in use.
func buildReconcileScript(tunName string) string {
	if tunName == "" {
		tunName = "whoisthattun"
	}
	return fmt.Sprintf(`
# --- nftables tables (modern default backend) ---
nft delete table ip whoisthat 2>/dev/null || true
nft delete table ip6 whoisthat 2>/dev/null || true
nft delete table inet whoisthat_ks 2>/dev/null || true
nft delete table ip whoisthat_mangle 2>/dev/null || true
nft delete table ip6 whoisthat_mangle 2>/dev/null || true
nft delete table inet whoisthat_mangle 2>/dev/null || true
nft delete table inet whoisthat_split 2>/dev/null || true

# --- iptables kill-switch chains (legacy backend) ---
for IPT in iptables ip6tables; do
    "$IPT" -D OUTPUT -j WHOISTHAT_KS 2>/dev/null || true
    "$IPT" -F WHOISTHAT_KS 2>/dev/null || true
    "$IPT" -X WHOISTHAT_KS 2>/dev/null || true
done

# --- iptables DNS hijack chains (legacy backend) ---
for IPT in iptables ip6tables; do
    "$IPT" -t nat -D OUTPUT -j WHOISTHAT_DNS_OUT 2>/dev/null || true
    "$IPT" -t nat -D PREROUTING -j WHOISTHAT_DNS_PRE 2>/dev/null || true
    "$IPT" -t nat -D POSTROUTING -j WHOISTHAT_DNS_POST 2>/dev/null || true
    "$IPT" -t nat -F WHOISTHAT_DNS_OUT 2>/dev/null || true
    "$IPT" -t nat -F WHOISTHAT_DNS_PRE 2>/dev/null || true
    "$IPT" -t nat -F WHOISTHAT_DNS_POST 2>/dev/null || true
    "$IPT" -t nat -X WHOISTHAT_DNS_OUT 2>/dev/null || true
    "$IPT" -t nat -X WHOISTHAT_DNS_PRE 2>/dev/null || true
    "$IPT" -t nat -X WHOISTHAT_DNS_POST 2>/dev/null || true
done

# --- policy routing: drop every rule pointing at table 100/200, then flush ---
for TABLE in 100 200; do
    i=0
    while [ $i -lt 32 ]; do
        ip rule del table $TABLE 2>/dev/null || break
        i=$((i + 1))
    done
    i=0
    while [ $i -lt 32 ]; do
        ip -6 rule del table $TABLE 2>/dev/null || break
        i=$((i + 1))
    done
    ip route flush table $TABLE 2>/dev/null || true
    ip -6 route flush table $TABLE 2>/dev/null || true
done

resolvectl revert %[1]q 2>/dev/null || true
ip link set %[1]q down 2>/dev/null || true
ip tuntap del mode tun dev %[1]q 2>/dev/null || true
ip link del dev %[1]q 2>/dev/null || true
`, tunName)
}

// ReconcileOrphanedRules tears down any leftover whoisthat networking state from
// a previous core that did not clean up after itself. It is safe to call at
// startup only once the caller has confirmed no other core instance is running
// (otherwise it would nuke the live core's rules); see main.go's port probe.
func ReconcileOrphanedRules(tunName string) error {
	// Restore rp_filter before deleting the interfaces: once the TUN device is
	// gone its sysctl node disappears and the saved value can no longer be
	// applied. The state file is written by loosenRpFilter before it changes
	// anything.
	if err := restoreRpFilter(); err != nil {
		return err
	}
	_, err := runScriptWithSh(buildReconcileScript(tunName))
	return err
}
