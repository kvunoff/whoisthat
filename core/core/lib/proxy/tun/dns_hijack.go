package tunmode

import (
	"errors"
	"fmt"
	"net"
	"strings"
)

// Reserved DNS sink addresses used by the TUN hijack. They are not assigned to
// any interface: packets routed to them fall through the TUN default route and
// are handled by Xray's DNS module.
const (
	DNSSink4 = "198.18.0.53"
	DNSSink6 = "fd00::53"
)

// Dedicated iptables chains keep DNS hijack cleanup exact and independent of
// the destination address that was in use when the rules were created.
const (
	dnsChainOut  = "WHOISTHAT_DNS_OUT"
	dnsChainPre  = "WHOISTHAT_DNS_PRE"
	dnsChainPost = "WHOISTHAT_DNS_POST"
)

// DnsHijack describes how port 53 is redirected while TUN is active.
//
// Enabled is the effective decision for the current split mode; Happ keeps the
// requested configuration even when Enabled is false so a live switch from
// include back to exclude/off can reinstall the same hijack.
type DnsHijack struct {
	Enabled   bool
	Legacy    bool
	Happ      bool
	Sink4     string
	Sink6     string
	ScopeMark int
	Exclude   []string // proxy server IPs that must not be DNAT'd on port 53
}

func sanitizeDnsHijack(d DnsHijack) DnsHijack {
	if d.Sink4 != "" {
		if ip := net.ParseIP(strings.TrimSpace(d.Sink4)); ip != nil && ip.To4() != nil {
			d.Sink4 = ip.String()
		} else {
			d.Sink4 = ""
		}
	}
	if d.Sink4 == "" && d.Happ {
		d.Sink4 = DNSSink4
	}
	if d.Sink6 != "" {
		if ip := net.ParseIP(strings.TrimSpace(d.Sink6)); ip != nil && ip.To4() == nil {
			d.Sink6 = ip.String()
		} else {
			d.Sink6 = ""
		}
	}
	if d.Sink6 == "" && d.Happ {
		d.Sink6 = DNSSink6
	}

	seen := make(map[string]struct{}, len(d.Exclude))
	exclude := make([]string, 0, len(d.Exclude))
	for _, raw := range d.Exclude {
		ip := net.ParseIP(strings.TrimSpace(raw))
		if ip == nil {
			continue
		}
		value := ip.String()
		if _, exists := seen[value]; exists {
			continue
		}
		seen[value] = struct{}{}
		exclude = append(exclude, value)
	}
	d.Exclude = exclude
	return d
}

func setupDnsHijackRules(interfaceName string, d DnsHijack) error {
	d = sanitizeDnsHijack(d)
	if !d.Enabled {
		return nil
	}

	var errs []error
	switch probeFirewall() {
	case firewallNftables:
		if d.Sink4 != "" {
			if _, err := runScriptWithSh(buildDnsHijackNftScript("ip", interfaceName, d)); err != nil {
				errs = append(errs, err)
			}
		}
		if d.Sink6 != "" {
			if _, err := runScriptWithSh(buildDnsHijackNftScript("ip6", interfaceName, d)); err != nil {
				errs = append(errs, err)
			}
		}
	default:
		if d.Sink4 != "" {
			if _, err := runScriptWithSh(buildDnsHijackIptablesScript("iptables", interfaceName, d)); err != nil {
				errs = append(errs, err)
			}
		}
		if d.Sink6 != "" {
			if _, err := runScriptWithSh(buildDnsHijackIptablesScript("ip6tables", interfaceName, d)); err != nil {
				errs = append(errs, err)
			}
		}
	}
	return errors.Join(errs...)
}

func cleanDnsHijackRules() error {
	switch probeFirewall() {
	case firewallNftables:
		_, err := runScriptWithSh(`
nft delete table ip whoisthat 2>/dev/null || true
nft delete table ip6 whoisthat 2>/dev/null || true
`)
		return err
	default:
		_, err := runScriptWithSh(
			buildDnsHijackCleanupIptablesScript("iptables") +
				buildDnsHijackCleanupIptablesScript("ip6tables"),
		)
		return err
	}
}

func buildDnsHijackNftScript(family, interfaceName string, d DnsHijack) string {
	sink := d.Sink4
	target := sink + ":53"
	if family == "ip6" {
		sink = d.Sink6
		target = "[" + sink + "]:53"
	}
	if sink == "" {
		return ""
	}

	var b strings.Builder
	b.WriteString("set -e\n")
	fmt.Fprintf(&b, "nft delete table %s whoisthat 2>/dev/null || true\n", family)
	fmt.Fprintf(&b, "nft add table %s whoisthat\n", family)
	fmt.Fprintf(&b, "nft add chain %s whoisthat prerouting '{ type nat hook prerouting priority -100; }'\n", family)
	fmt.Fprintf(&b, "nft add chain %s whoisthat output '{ type nat hook output priority -100; }'\n", family)

	if d.Legacy {
		fmt.Fprintf(&b, "nft add chain %s whoisthat postrouting '{ type nat hook postrouting priority 100; }'\n", family)
	}

	fmt.Fprintf(&b, "nft add rule %s whoisthat prerouting udp dport 53 dnat to %s\n", family, target)
	fmt.Fprintf(&b, "nft add rule %s whoisthat prerouting tcp dport 53 dnat to %s\n", family, target)

	if d.Legacy {
		if interfaceName != "" {
			fmt.Fprintf(&b, "IFACE=%q\n", interfaceName)
			fmt.Fprintf(&b, "nft add rule %s whoisthat postrouting %s daddr %s udp dport 53 oif $IFACE masquerade\n", family, family, sink)
			fmt.Fprintf(&b, "nft add rule %s whoisthat postrouting %s daddr %s tcp dport 53 oif $IFACE masquerade\n", family, family, sink)
		}
	} else {
		if d.ScopeMark != 0 {
			fmt.Fprintf(&b, "nft add rule %s whoisthat output meta mark %d return\n", family, d.ScopeMark)
		}
		fmt.Fprintf(&b, "nft add rule %s whoisthat output %s daddr %s return\n", family, family, sink)
		for _, ip := range d.Exclude {
			if !dnsIPMatchesFamily(ip, family) {
				continue
			}
			fmt.Fprintf(&b, "nft add rule %s whoisthat output %s daddr %s return\n", family, family, ip)
		}
	}

	fmt.Fprintf(&b, "nft add rule %s whoisthat output udp dport 53 dnat to %s\n", family, target)
	fmt.Fprintf(&b, "nft add rule %s whoisthat output tcp dport 53 dnat to %s\n", family, target)
	return b.String()
}

func buildDnsHijackIptablesScript(ipt, interfaceName string, d DnsHijack) string {
	family := "ip"
	family6 := ipt == "ip6tables"
	sink := d.Sink4
	target := sink + ":53"
	if family6 {
		family = "ip6"
		sink = d.Sink6
		target = "[" + sink + "]:53"
	}
	if sink == "" {
		return ""
	}

	var b strings.Builder
	b.WriteString("set -e\n")

	chains := []string{dnsChainOut, dnsChainPre}
	if d.Legacy {
		chains = append(chains, dnsChainPost)
	}
	for _, chain := range chains {
		fmt.Fprintf(&b, "%s -t nat -N %s 2>/dev/null || true\n", ipt, chain)
		fmt.Fprintf(&b, "%s -t nat -F %s\n", ipt, chain)
	}

	fmt.Fprintf(&b, "%s -t nat -C OUTPUT -j %s 2>/dev/null || %s -t nat -I OUTPUT 1 -j %s\n", ipt, dnsChainOut, ipt, dnsChainOut)
	fmt.Fprintf(&b, "%s -t nat -C PREROUTING -j %s 2>/dev/null || %s -t nat -I PREROUTING 1 -j %s\n", ipt, dnsChainPre, ipt, dnsChainPre)
	if d.Legacy {
		fmt.Fprintf(&b, "%s -t nat -C POSTROUTING -j %s 2>/dev/null || %s -t nat -I POSTROUTING 1 -j %s\n", ipt, dnsChainPost, ipt, dnsChainPost)
	}

	if !d.Legacy {
		if d.ScopeMark != 0 {
			fmt.Fprintf(&b, "%s -t nat -A %s -m mark --mark %d -j RETURN\n", ipt, dnsChainOut, d.ScopeMark)
		}
		fmt.Fprintf(&b, "%s -t nat -A %s -d %s -j RETURN\n", ipt, dnsChainOut, sink)
		for _, ip := range d.Exclude {
			if !dnsIPMatchesFamily(ip, family) {
				continue
			}
			fmt.Fprintf(&b, "%s -t nat -A %s -d %s -j RETURN\n", ipt, dnsChainOut, ip)
		}
	}

	fmt.Fprintf(&b, "%s -t nat -A %s -p udp --dport 53 -j DNAT --to-destination %s\n", ipt, dnsChainOut, target)
	fmt.Fprintf(&b, "%s -t nat -A %s -p tcp --dport 53 -j DNAT --to-destination %s\n", ipt, dnsChainOut, target)
	fmt.Fprintf(&b, "%s -t nat -A %s -p udp --dport 53 -j DNAT --to-destination %s\n", ipt, dnsChainPre, target)
	fmt.Fprintf(&b, "%s -t nat -A %s -p tcp --dport 53 -j DNAT --to-destination %s\n", ipt, dnsChainPre, target)

	if d.Legacy && interfaceName != "" {
		fmt.Fprintf(&b, "IFACE=%q\n", interfaceName)
		fmt.Fprintf(&b, "%s -t nat -A %s -p udp -d %s --dport 53 -o $IFACE -j MASQUERADE\n", ipt, dnsChainPost, sink)
		fmt.Fprintf(&b, "%s -t nat -A %s -p tcp -d %s --dport 53 -o $IFACE -j MASQUERADE\n", ipt, dnsChainPost, sink)
	}

	return b.String()
}

func buildDnsHijackCleanupIptablesScript(ipt string) string {
	var b strings.Builder
	b.WriteString("set +e\n")
	pairs := []struct {
		hook  string
		chain string
	}{
		{"OUTPUT", dnsChainOut},
		{"PREROUTING", dnsChainPre},
		{"POSTROUTING", dnsChainPost},
	}
	for _, pair := range pairs {
		fmt.Fprintf(&b, "%s -t nat -D %s -j %s 2>/dev/null || true\n", ipt, pair.hook, pair.chain)
	}
	for _, pair := range pairs {
		fmt.Fprintf(&b, "%s -t nat -F %s 2>/dev/null || true\n", ipt, pair.chain)
		fmt.Fprintf(&b, "%s -t nat -X %s 2>/dev/null || true\n", ipt, pair.chain)
	}
	return b.String()
}

func dnsIPMatchesFamily(ip, family string) bool {
	parsed := net.ParseIP(ip)
	if parsed == nil {
		return false
	}
	if family == "ip6" {
		return parsed.To4() == nil
	}
	return parsed.To4() != nil
}
