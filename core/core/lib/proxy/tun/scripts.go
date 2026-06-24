package tunmode

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"strings"
	"sync"
	"syscall"

	"golang.org/x/sys/unix"
)

type firewallBackend int

const (
	firewallIptables firewallBackend = iota
	firewallNftables
)

var (
	fwBackend firewallBackend
	fwOnce    sync.Once
)

func probeFirewall() firewallBackend {
	fwOnce.Do(func() {
		// Allow forcing a backend. Useful when the nftables `type route hook
		// output` reroute-on-mark misbehaves on a given kernel: set
		// WHOISTHAT_FW_BACKEND=iptables to drive the (potentially legacy)
		// iptables mangle path instead. For a genuinely different reroute
		// implementation point the PATH at iptables-legacy.
		switch strings.ToLower(strings.TrimSpace(os.Getenv("WHOISTHAT_FW_BACKEND"))) {
		case "iptables", "legacy":
			fwBackend = firewallIptables
			return
		case "nft", "nftables":
			fwBackend = firewallNftables
			return
		}
		if _, err := exec.LookPath("nft"); err == nil {
			fwBackend = firewallNftables
		} else {
			fwBackend = firewallIptables
		}
	})
	return fwBackend
}

func runScriptWithSh(script string) (string, error) {
	cmd := exec.Command("sh")
	cmd.SysProcAttr = &syscall.SysProcAttr{
		AmbientCaps: []uintptr{unix.CAP_NET_ADMIN, unix.CAP_NET_RAW},
	}
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return "", fmt.Errorf("failed to get stdin of sh %w", err)
	}
	_, err = stdin.Write([]byte(script))

	if err != nil {
		return "", fmt.Errorf("failed to write to stdin of sh %w", err)
	}
	err = stdin.Close()
	if err != nil {
		return "", fmt.Errorf("failed to close stdin of sh %w", err)
	}
	stderr := bytes.Buffer{}
	cmd.Stderr = &stderr
	output, err := cmd.Output()

	if err != nil {
		return "", fmt.Errorf("%w: %s", err, string(stderr.String()))
	}
	return string(output), nil
}

// ---------------------------------------------------------------------------
// DNS hijack (IPv4)
// ---------------------------------------------------------------------------

func setupDnsHijackRules(interface_name string, dns_ip string) error {
	switch probeFirewall() {
	case firewallNftables:
		return setupDnsHijackRulesNftables(interface_name, dns_ip)
	default:
		return setupDnsHijackRulesIptables(interface_name, dns_ip)
	}
}

func setupDnsHijackRulesIptables(interface_name string, dns_ip string) error {
	script := fmt.Sprintf(`
IFACE="%s"
DNS_IP="%s"

iptables -t nat -A PREROUTING -p udp --dport 53 \
  -j DNAT --to-destination ${DNS_IP}:53
iptables -t nat -A PREROUTING -p tcp --dport 53 \
  -j DNAT --to-destination ${DNS_IP}:53

iptables -t nat -A POSTROUTING -p udp -d ${DNS_IP} --dport 53 -o ${IFACE} \
  -j MASQUERADE
iptables -t nat -A POSTROUTING -p tcp -d ${DNS_IP} --dport 53 -o ${IFACE} \
  -j MASQUERADE

iptables -t nat -A OUTPUT -p udp --dport 53 \
  -j DNAT --to-destination ${DNS_IP}:53
iptables -t nat -A OUTPUT -p tcp --dport 53 \
  -j DNAT --to-destination ${DNS_IP}:53
	`, interface_name, dns_ip)
	_, err := runScriptWithSh(script)
	return err
}

func setupDnsHijackRulesNftables(interface_name string, dns_ip string) error {
	script := fmt.Sprintf(`
set -e
IFACE="%s"
DNS_IP="%s"

nft delete table ip whoisthat 2>/dev/null || true

nft add table ip whoisthat
nft add chain ip whoisthat prerouting '{ type nat hook prerouting priority -100; }'
nft add chain ip whoisthat postrouting '{ type nat hook postrouting priority 100; }'
nft add chain ip whoisthat output '{ type nat hook output priority -100; }'

nft add rule ip whoisthat prerouting udp dport 53 dnat to $DNS_IP:53
nft add rule ip whoisthat prerouting tcp dport 53 dnat to $DNS_IP:53
nft add rule ip whoisthat postrouting ip daddr $DNS_IP udp dport 53 oif $IFACE masquerade
nft add rule ip whoisthat postrouting ip daddr $DNS_IP tcp dport 53 oif $IFACE masquerade
nft add rule ip whoisthat output udp dport 53 dnat to $DNS_IP:53
nft add rule ip whoisthat output tcp dport 53 dnat to $DNS_IP:53
`, interface_name, dns_ip)
	_, err := runScriptWithSh(script)
	return err
}

// ---------------------------------------------------------------------------
// DNS hijack (IPv6)
// ---------------------------------------------------------------------------

func setupDnsHijackRules6(interface_name string, dns_ip6 string) error {
	switch probeFirewall() {
	case firewallNftables:
		return setupDnsHijackRules6Nftables(interface_name, dns_ip6)
	default:
		return setupDnsHijackRules6Iptables(interface_name, dns_ip6)
	}
}

func setupDnsHijackRules6Iptables(interface_name string, dns_ip6 string) error {
	script := fmt.Sprintf(`
IFACE="%s"
DNS_IP6="%s"

ip6tables -t nat -A PREROUTING -p udp --dport 53 \
  -j DNAT --to-destination [${DNS_IP6}]:53
ip6tables -t nat -A PREROUTING -p tcp --dport 53 \
  -j DNAT --to-destination [${DNS_IP6}]:53

ip6tables -t nat -A POSTROUTING -p udp -d ${DNS_IP6} --dport 53 -o ${IFACE} \
  -j MASQUERADE
ip6tables -t nat -A POSTROUTING -p tcp -d ${DNS_IP6} --dport 53 -o ${IFACE} \
  -j MASQUERADE

ip6tables -t nat -A OUTPUT -p udp --dport 53 \
  -j DNAT --to-destination [${DNS_IP6}]:53
ip6tables -t nat -A OUTPUT -p tcp --dport 53 \
  -j DNAT --to-destination [${DNS_IP6}]:53
	`, interface_name, dns_ip6)
	_, err := runScriptWithSh(script)
	return err
}

func setupDnsHijackRules6Nftables(interface_name string, dns_ip6 string) error {
	script := fmt.Sprintf(`
set -e
IFACE="%s"
DNS_IP6="%s"

nft delete table ip6 whoisthat 2>/dev/null || true

nft add table ip6 whoisthat
nft add chain ip6 whoisthat prerouting '{ type nat hook prerouting priority -100; }'
nft add chain ip6 whoisthat postrouting '{ type nat hook postrouting priority 100; }'
nft add chain ip6 whoisthat output '{ type nat hook output priority -100; }'

nft add rule ip6 whoisthat prerouting udp dport 53 dnat to $DNS_IP6:53
nft add rule ip6 whoisthat prerouting tcp dport 53 dnat to $DNS_IP6:53
nft add rule ip6 whoisthat postrouting ip6 daddr $DNS_IP6 udp dport 53 oif $IFACE masquerade
nft add rule ip6 whoisthat postrouting ip6 daddr $DNS_IP6 tcp dport 53 oif $IFACE masquerade
nft add rule ip6 whoisthat output udp dport 53 dnat to $DNS_IP6:53
nft add rule ip6 whoisthat output tcp dport 53 dnat to $DNS_IP6:53
`, interface_name, dns_ip6)
	_, err := runScriptWithSh(script)
	return err
}

// ---------------------------------------------------------------------------
// DNS hijack cleanup (IPv4)
// ---------------------------------------------------------------------------

func cleanDnsHijackRules(interface_name string, dns_ip string) error {
	switch probeFirewall() {
	case firewallNftables:
		return cleanDnsHijackRulesNftables(interface_name, dns_ip)
	default:
		return cleanDnsHijackRulesIptables(interface_name, dns_ip)
	}
}

func cleanDnsHijackRulesIptables(interface_name string, dns_ip string) error {
	script := fmt.Sprintf(`
IFACE="%s"
DNS_IP="%s"

delete_all_matches() {
    local table=$1
    shift
    while iptables -t "$table" -C "$@" 2>/dev/null; do
        iptables -t "$table" -D "$@"
    done
}

delete_all_matches nat PREROUTING -p udp --dport 53 \
  -j DNAT --to-destination ${DNS_IP}:53
delete_all_matches nat PREROUTING -p tcp --dport 53 \
  -j DNAT --to-destination ${DNS_IP}:53

delete_all_matches nat POSTROUTING -p udp -d ${DNS_IP} --dport 53 -o ${IFACE} \
  -j MASQUERADE
delete_all_matches nat POSTROUTING -p tcp -d ${DNS_IP} --dport 53 -o ${IFACE} \
  -j MASQUERADE

delete_all_matches nat OUTPUT -p udp --dport 53 \
  -j DNAT --to-destination ${DNS_IP}:53
delete_all_matches nat OUTPUT -p tcp --dport 53 \
  -j DNAT --to-destination ${DNS_IP}:53
	`, interface_name, dns_ip)
	_, err := runScriptWithSh(script)
	return err
}

func cleanDnsHijackRulesNftables(interface_name string, dns_ip string) error {
	_, err := runScriptWithSh("nft delete table ip whoisthat 2>/dev/null || true")
	return err
}

// ---------------------------------------------------------------------------
// DNS hijack cleanup (IPv6)
// ---------------------------------------------------------------------------

func cleanDnsHijackRules6(interface_name string, dns_ip6 string) error {
	switch probeFirewall() {
	case firewallNftables:
		return cleanDnsHijackRules6Nftables(interface_name, dns_ip6)
	default:
		return cleanDnsHijackRules6Iptables(interface_name, dns_ip6)
	}
}

func cleanDnsHijackRules6Iptables(interface_name string, dns_ip6 string) error {
	script := fmt.Sprintf(`
IFACE="%s"
DNS_IP6="%s"

delete_all_matches6() {
    local table=$1
    shift
    while ip6tables -t "$table" -C "$@" 2>/dev/null; do
        ip6tables -t "$table" -D "$@"
    done
}

delete_all_matches6 nat PREROUTING -p udp --dport 53 \
  -j DNAT --to-destination [${DNS_IP6}]:53
delete_all_matches6 nat PREROUTING -p tcp --dport 53 \
  -j DNAT --to-destination [${DNS_IP6}]:53

delete_all_matches6 nat POSTROUTING -p udp -d ${DNS_IP6} --dport 53 -o ${IFACE} \
  -j MASQUERADE
delete_all_matches6 nat POSTROUTING -p tcp -d ${DNS_IP6} --dport 53 -o ${IFACE} \
  -j MASQUERADE

delete_all_matches6 nat OUTPUT -p udp --dport 53 \
  -j DNAT --to-destination [${DNS_IP6}]:53
delete_all_matches6 nat OUTPUT -p tcp --dport 53 \
  -j DNAT --to-destination [${DNS_IP6}]:53
	`, interface_name, dns_ip6)
	_, err := runScriptWithSh(script)
	return err
}

func cleanDnsHijackRules6Nftables(interface_name string, dns_ip6 string) error {
	_, err := runScriptWithSh("nft delete table ip6 whoisthat 2>/dev/null || true")
	return err
}

// ---------------------------------------------------------------------------
// TUN device
// ---------------------------------------------------------------------------

func createTun(name string, ip string, ipv6 string) error {
	script := fmt.Sprintf(`
set -e
TUN_NAME="%s"
TUN_IP="%s"
TUN_IP6="%s"
ip tuntap add mode tun dev $TUN_NAME
ip addr add $TUN_IP dev $TUN_NAME
ip addr add $TUN_IP6 dev $TUN_NAME
ip link set dev $TUN_NAME up
	`, name, ip, ipv6)
	_, err := runScriptWithSh(script)
	return err
}

func deleteTun(name string) error {
	script := fmt.Sprintf(`
TUN_NAME="%s"
ip link set $TUN_NAME down 2>/dev/null || true
ip tuntap del mode tun dev $TUN_NAME
	`, name)
	_, err := runScriptWithSh(script)
	return err
}

// ---------------------------------------------------------------------------
// DNS IP routes
// ---------------------------------------------------------------------------

func setupDnsIpRoutes(dns_ip string, dns_ip6 string, interface_ip string, interface_ip6 string) error {
	script := "\nset -e\n"
	if dns_ip != "" && interface_ip != "" {
		script += fmt.Sprintf("ip route add %s via %s\n", dns_ip, interface_ip)
		script += "ip route add 127.0.0.53 via " + interface_ip + "\n"
	}
	if dns_ip6 != "" && interface_ip6 != "" {
		script += fmt.Sprintf("ip -6 route add %s via %s\n", dns_ip6, interface_ip6)
	}
	_, err := runScriptWithSh(script)
	return err
}

func deleteDnsIpRoutes(dns_ip string, interface_ip string) error {
	script := ""
	if dns_ip != "" && interface_ip != "" {
		script += fmt.Sprintf("ip route del %s via %s || true\n", dns_ip, interface_ip)
		script += "ip route del 127.0.0.53 via " + interface_ip + " || true\n"
	}
	_, err := runScriptWithSh(script)
	return err
}

func deleteDnsIpRoutes6(dns_ip6 string, interface_ip6 string) error {
	script := ""
	if dns_ip6 != "" && interface_ip6 != "" {
		script += fmt.Sprintf("ip -6 route del %s via %s || true\n", dns_ip6, interface_ip6)
	}
	_, err := runScriptWithSh(script)
	return err
}

// ---------------------------------------------------------------------------
// Proxy IP routes
// ---------------------------------------------------------------------------

func setupProxyIpRoutes(proxy_ipv4s []string, proxy_ipv6s []string, default_interface_ip string, default_interface_ipv6 string) error {
	script := "\nset -e\n"
	for _, ip := range proxy_ipv4s {
		script += fmt.Sprintf("ip route add %s via %s\n", ip, default_interface_ip)
	}
	for _, ip := range proxy_ipv6s {
		script += fmt.Sprintf("ip -6 route add %s via %s\n", ip, default_interface_ipv6)
	}
	_, err := runScriptWithSh(script)
	return err
}

func deleteProxyIpRoutes(proxy_ipv4s []string, proxy_ipv6s []string, default_interface_ip string, default_interface_ipv6 string) error {
	script := ""
	for _, ip := range proxy_ipv4s {
		script += fmt.Sprintf("ip route del %s via %s || true\n", ip, default_interface_ip)
	}
	for _, ip := range proxy_ipv6s {
		script += fmt.Sprintf("ip -6 route del %s via %s || true\n", ip, default_interface_ipv6)
	}
	_, err := runScriptWithSh(script)
	return err
}

// ---------------------------------------------------------------------------
// TUN default route
// ---------------------------------------------------------------------------

func setupTunIpRoute(tun_name string, tun_interface_ip string) error {
	script := fmt.Sprintf(`
set -e
TUN_NAME="%s"
TUN_IP="%s"
ip route add default via $TUN_IP dev $TUN_NAME metric 1
	`, tun_name, tun_interface_ip)
	_, err := runScriptWithSh(script)
	return err
}

func deleteTunIpRoute(tun_name string, tun_interface_ip string) error {
	script := fmt.Sprintf(`
TUN_NAME="%s"
TUN_IP="%s"
ip route del default via $TUN_IP dev $TUN_NAME metric 1 || true
	`, tun_name, tun_interface_ip)
	_, err := runScriptWithSh(script)
	return err
}

func setupTunIpRoute6(tun_name string, tun_interface_ip6 string) error {
	script := fmt.Sprintf(`
set -e
TUN_NAME="%s"
ip -6 route add default dev $TUN_NAME metric 1
	`, tun_name)
	_, err := runScriptWithSh(script)
	return err
}

func deleteTunIpRoute6(tun_name string, tun_interface_ip6 string) error {
	script := fmt.Sprintf(`
TUN_NAME="%s"
ip -6 route del default dev $TUN_NAME metric 1 || true
	`, tun_name)
	_, err := runScriptWithSh(script)
	return err
}

// ---------------------------------------------------------------------------
// UID routing (bypass TUN for xray)
// ---------------------------------------------------------------------------

func addUidRouting(uid int, interface_name string, gateway_ip string) error {
	script := fmt.Sprintf(`
set -e
IFACE="%s"
GATEWAY="%s"
RULE_UID=%d

ip rule add uidrange $RULE_UID-$RULE_UID lookup 100 2>/dev/null || true
ip route replace $GATEWAY/32 dev $IFACE table 100
ip route replace default via $GATEWAY table 100
`, interface_name, gateway_ip, uid)
	_, err := runScriptWithSh(script)
	return err
}

func removeUidRouting(uid int, interface_name string, gateway_ip string) error {
	script := fmt.Sprintf(`
IFACE="%s"
GATEWAY="%s"
RULE_UID=%d

ip rule del uidrange $RULE_UID-$RULE_UID lookup 100 2>/dev/null || true
ip route del $GATEWAY/32 dev $IFACE table 100 2>/dev/null || true
ip route del default via $GATEWAY table 100 2>/dev/null || true
`, interface_name, gateway_ip, uid)
	_, err := runScriptWithSh(script)
	return err
}

func addUidRouting6(uid int, interface_name string, gateway_ip6 string) error {
	script := fmt.Sprintf(`
set -e
IFACE="%s"
GATEWAY6="%s"
RULE_UID=%d

ip -6 rule add uidrange $RULE_UID-$RULE_UID lookup 100 2>/dev/null || true
ip -6 route replace $GATEWAY6/128 dev $IFACE table 100
ip -6 route replace default via $GATEWAY6 table 100
`, interface_name, gateway_ip6, uid)
	_, err := runScriptWithSh(script)
	return err
}

func removeUidRouting6(uid int, interface_name string, gateway_ip6 string) error {
	script := fmt.Sprintf(`
IFACE="%s"
GATEWAY6="%s"
RULE_UID=%d

ip -6 rule del uidrange $RULE_UID-$RULE_UID lookup 100 2>/dev/null || true
ip -6 route del $GATEWAY6/128 dev $IFACE table 100 2>/dev/null || true
ip -6 route del default via $GATEWAY6 table 100 2>/dev/null || true
`, interface_name, gateway_ip6, uid)
	_, err := runScriptWithSh(script)
	return err
}

// ---------------------------------------------------------------------------
// Fwmark routing (bypass TUN for xray — capability mode)
// ---------------------------------------------------------------------------

func addFwmarkRouting(mark int, iface string, gateway string) error {
	script := fmt.Sprintf(`
set -e
ip rule add fwmark %d table 100 2>/dev/null || true
ip route replace %s/32 dev %s table 100
ip route replace default via %s table 100
`, mark, gateway, iface, gateway)
	_, err := runScriptWithSh(script)
	return err
}

func removeFwmarkRouting(mark int, iface string, gateway string) error {
	script := fmt.Sprintf(`
ip rule del fwmark %d table 100 2>/dev/null || true
ip route del %s/32 dev %s table 100 2>/dev/null || true
ip route del default via %s table 100 2>/dev/null || true
`, mark, gateway, iface, gateway)
	_, err := runScriptWithSh(script)
	return err
}

func addFwmarkRouting6(mark int, iface string, gateway6 string) error {
	script := fmt.Sprintf(`
set -e
ip -6 rule add fwmark %d table 100 2>/dev/null || true
ip -6 route replace %s/128 dev %s table 100
ip -6 route replace default via %s table 100
`, mark, gateway6, iface, gateway6)
	_, err := runScriptWithSh(script)
	return err
}

func removeFwmarkRouting6(mark int, iface string, gateway6 string) error {
	script := fmt.Sprintf(`
ip -6 rule del fwmark %d table 100 2>/dev/null || true
ip -6 route del %s/128 dev %s table 100 2>/dev/null || true
ip -6 route del default via %s table 100 2>/dev/null || true
`, mark, gateway6, iface, gateway6)
	_, err := runScriptWithSh(script)
	return err
}

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

func loosenRpFilter(tun_name string, default_interface_name string) error {
	script := fmt.Sprintf(`
TUN_NAME="%s"
DEF_IFACE="%s"

for IFACE in "$DEF_IFACE" "$TUN_NAME"; do
    if ip link show "$IFACE" &>/dev/null; then
        echo "Setting rp_filter=2 for $IFACE (temporary)"
        sysctl -w net.ipv4.conf."$IFACE".rp_filter=2
    else
        echo "Warning: Interface '$IFACE' not found, skipping."
    fi
done
	`, tun_name, default_interface_name)
	_, err := runScriptWithSh(script)
	return err
}
