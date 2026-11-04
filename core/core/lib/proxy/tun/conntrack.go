package tunmode

import "fmt"

// ---------------------------------------------------------------------------
// Conntrack bypass (incoming connections bypass TUN via fwmark)
// ---------------------------------------------------------------------------

func setupConntrackRules(tun_name string, def_iface string) error {
	switch probeFirewall() {
	case firewallNftables:
		return setupConntrackRulesNftables(tun_name, def_iface)
	default:
		return setupConntrackRulesIptables(tun_name, def_iface)
	}
}

func setupConntrackRulesIptables(tun_name string, def_iface string) error {
	// Mirror of setupConntrackRulesNftables on the iptables/mangle backend; see
	// that function for the full rationale. The mangle OUTPUT chain is a route
	// hook, so `-j MARK` reroutes locally-generated replies; for forwarded
	// (Docker) replies the mark is applied in PREROUTING before the forward
	// routing decision.
	//
	// `--ctdir REPLY` restricts the mark copy to reply-direction packets so the
	// inbound/original direction (incl. packets DNAT'd into a container) is not
	// misrouted. The conditional `-m connmark --mark 1` (not
	// `CONNMARK --restore-mark`) avoids clobbering xray's SO_MARK=1.
	script := fmt.Sprintf(`
set -e
TUN_NAME="%s"
DEF_IFACE="%s"

for IPT in iptables ip6tables; do
  "$IPT" -t mangle -A PREROUTING -i "$DEF_IFACE" \
    -m conntrack --ctstate NEW -j CONNMARK --set-mark 1
  "$IPT" -t mangle -A PREROUTING \
    -m conntrack --ctdir REPLY -m connmark --mark 1 -j MARK --set-mark 1
  "$IPT" -t mangle -A OUTPUT \
    -m conntrack --ctdir REPLY -m connmark --mark 1 -j MARK --set-mark 1
done
`, tun_name, def_iface)
	_, err := runScriptWithSh(script)
	return err
}

func setupConntrackRulesNftables(tun_name string, def_iface string) error {
	// Goal: an externally-initiated connection (e.g. an external client hitting
	// a local or Docker-published server) must have its *reply* packets leave
	// via the physical gateway, not the TUN default route.
	//
	// Strategy — tag the flow, then mark only the reply direction:
	//   1. PREROUTING: a NEW flow arriving on the physical interface gets
	//      conn mark 1. Scoping to the physical iface (not "anything but TUN")
	//      is deliberate: tagging NEW flows from docker0/veth would catch
	//      containers' *outbound* connections, whose replies would then wrongly
	//      bypass the TUN.
	//   2. PREROUTING: for reply-direction packets of a tagged flow, copy the
	//      conn mark to the packet mark. This handles *forwarded* replies
	//      (Docker: the container SYN-ACK is forwarded, never hits OUTPUT) — the
	//      forward routing decision happens right after PREROUTING, so the mark
	//      is in place before it and no rerouting is needed.
	//   3. OUTPUT (route hook): same copy for locally-generated replies (a
	//      server running directly on the host). The route hook reroutes on the
	//      mark change.
	//
	// Crucially the mark copy is gated on "ct direction reply": the original
	// direction (client -> server, incl. the inbound SYN and packets forwarded
	// *into* the container) must NOT get the mark, otherwise after DNAT it would
	// match "fwmark 1 -> table 100", which has no route to the container subnet,
	// and the inbound packet would be misrouted to the gateway.
	//
	// The copy is also conditional on "ct mark 1", so it never clobbers the
	// SO_MARK=1 xray sets on its own direct sockets (those flows are locally
	// initiated, never tagged here).
	//
	// Counters are attached to every rule for live inspection via
	// "nft list table ip whoisthat_mangle".
	script := fmt.Sprintf(`
set -e
TUN_NAME="%s"
DEF_IFACE="%s"

nft delete table ip whoisthat_mangle 2>/dev/null || true
nft delete table ip6 whoisthat_mangle 2>/dev/null || true
nft delete table inet whoisthat_mangle 2>/dev/null || true

# ---- IPv4 ----
nft add table ip whoisthat_mangle
nft 'add chain ip whoisthat_mangle prerouting { type filter hook prerouting priority -150; policy accept; }'
nft 'add chain ip whoisthat_mangle output { type route hook output priority -150; policy accept; }'

nft add rule ip whoisthat_mangle prerouting \
  iifname "$DEF_IFACE" ct state new counter ct mark set 1
nft add rule ip whoisthat_mangle prerouting \
  ct mark 1 ct direction reply counter meta mark set 1
nft add rule ip whoisthat_mangle output \
  ct mark 1 ct direction reply counter meta mark set 1

# ---- IPv6 ----
nft add table ip6 whoisthat_mangle
nft 'add chain ip6 whoisthat_mangle prerouting { type filter hook prerouting priority -150; policy accept; }'
nft 'add chain ip6 whoisthat_mangle output { type route hook output priority -150; policy accept; }'

nft add rule ip6 whoisthat_mangle prerouting \
  iifname "$DEF_IFACE" ct state new counter ct mark set 1
nft add rule ip6 whoisthat_mangle prerouting \
  ct mark 1 ct direction reply counter meta mark set 1
nft add rule ip6 whoisthat_mangle output \
  ct mark 1 ct direction reply counter meta mark set 1
`, tun_name, def_iface)
	_, err := runScriptWithSh(script)
	return err
}

func cleanConntrackRules(tun_name string, def_iface string) error {
	switch probeFirewall() {
	case firewallNftables:
		return cleanConntrackRulesNftables()
	default:
		return cleanConntrackRulesIptables(tun_name, def_iface)
	}
}

func cleanConntrackRulesIptables(tun_name string, def_iface string) error {
	script := fmt.Sprintf(`
DEF_IFACE="%s"

delete_all() {
    local ipt=$1
    shift
    while "$ipt" -t mangle -C "$@" 2>/dev/null; do
        "$ipt" -t mangle -D "$@"
    done
}

for IPT in iptables ip6tables; do
    delete_all "$IPT" PREROUTING -i "$DEF_IFACE" \
      -m conntrack --ctstate NEW -j CONNMARK --set-mark 1
    delete_all "$IPT" PREROUTING \
      -m conntrack --ctdir REPLY -m connmark --mark 1 -j MARK --set-mark 1
    delete_all "$IPT" OUTPUT \
      -m conntrack --ctdir REPLY -m connmark --mark 1 -j MARK --set-mark 1
done
`, def_iface)
	_, err := runScriptWithSh(script)
	return err
}

func cleanConntrackRulesNftables() error {
	_, err := runScriptWithSh(`
nft delete table inet whoisthat_mangle 2>/dev/null || true
nft delete table ip whoisthat_mangle 2>/dev/null || true
nft delete table ip6 whoisthat_mangle 2>/dev/null || true
`)
	return err
}
