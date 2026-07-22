package tunmode

// buildReconcileScript returns an idempotent teardown script that removes any
// leftover whoisthat networking state from a previous core that exited without
// cleaning up — a hard `Q`, a crash, an OOM kill, or a power loss mid-TUN.
//
// It covers every artifact the core can create:
//   - nftables tables: whoisthat (DNS hijack v4/v6), whoisthat_ks (kill-switch),
//     whoisthat_mangle (conntrack bypass), whoisthat_split (split tunnel)
//   - iptables kill-switch chains (legacy backend)
//   - policy-routing rules pointing at table 100 (fwmark + uidrange) and the
//     table 100 routes themselves
//
// Every command is guarded so the script is a clean no-op when nothing exists.
// The `ip rule del table 100` loops delete one matching rule per iteration and
// stop as soon as none remain (bounded to avoid an unbounded loop). This
// deletes fwmark AND uidrange rules regardless of their specific values, so it
// stays correct even if the dedicated UID differs from the crashed session's.
//
// The iptables DNS-hijack nat rules are intentionally NOT reconciled here: they
// are keyed by the default interface and DNS IP at creation time, which may have
// changed since the crash, making blind removal unreliable. In practice nft is
// the default backend and its whole-table delete is complete; on the iptables
// backend the kill-switch chains (the documented breakage) are still cleaned.
func buildReconcileScript() string {
	return `
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

# --- policy routing: drop every rule pointing at table 100, then flush it ---
i=0
while [ $i -lt 32 ]; do
    ip rule del table 100 2>/dev/null || break
    i=$((i + 1))
done
i=0
while [ $i -lt 32 ]; do
    ip -6 rule del table 100 2>/dev/null || break
    i=$((i + 1))
done
ip route flush table 100 2>/dev/null || true
ip -6 route flush table 100 2>/dev/null || true
`
}

// ReconcileOrphanedRules tears down any leftover whoisthat networking state from
// a previous core that did not clean up after itself. It is safe to call at
// startup only once the caller has confirmed no other core instance is running
// (otherwise it would nuke the live core's rules); see main.go's port probe.
func ReconcileOrphanedRules() error {
	_, err := runScriptWithSh(buildReconcileScript())
	return err
}
