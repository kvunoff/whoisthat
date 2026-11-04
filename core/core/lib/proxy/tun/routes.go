package tunmode

import (
	"fmt"
	"strings"
	"time"
)

// ---------------------------------------------------------------------------
// TUN device
// ---------------------------------------------------------------------------

const DefaultTunMTU = 1400

func createTun(name string, ip string, ipv6 string) error {
	var lastErr error
	for attempt := 0; attempt < 5; attempt++ {
		if attempt > 0 {
			time.Sleep(50 * time.Millisecond)
		}
		script := fmt.Sprintf(`
TUN_NAME="%s"
TUN_IP="%s"
TUN_IP6="%s"
TUN_MTU="%d"

# If device already exists, remove it first to avoid EBUSY on recreate
if ip link show "$TUN_NAME" >/dev/null 2>&1; then
    ip link set "$TUN_NAME" down 2>/dev/null || true
    ip tuntap del mode tun dev "$TUN_NAME" 2>/dev/null || true
    ip link del dev "$TUN_NAME" 2>/dev/null || true
fi

set -e
ip tuntap add mode tun dev "$TUN_NAME"
ip link set dev "$TUN_NAME" mtu "$TUN_MTU"
ip addr add "$TUN_IP" dev "$TUN_NAME"
if [ -n "$TUN_IP6" ]; then
    ip addr add "$TUN_IP6" dev "$TUN_NAME"
fi
ip link set dev "$TUN_NAME" up
		`, name, ip, ipv6, DefaultTunMTU)
		_, err := runScriptWithSh(script)
		if err == nil {
			return nil
		}
		lastErr = err
		errStr := err.Error()
		if !strings.Contains(errStr, "busy") && !strings.Contains(errStr, "File exists") {
			break
		}
	}
	return lastErr
}

func deleteTun(name string) error {
	script := fmt.Sprintf(`
TUN_NAME="%s"
ip link set "$TUN_NAME" down 2>/dev/null || true
ip tuntap del mode tun dev "$TUN_NAME" 2>/dev/null || true
ip link del dev "$TUN_NAME" 2>/dev/null || true
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
