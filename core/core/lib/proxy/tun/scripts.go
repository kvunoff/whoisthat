package tunmode

import (
	"bytes"
	"fmt"
	"os/exec"
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
// RP filter
// ---------------------------------------------------------------------------

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
