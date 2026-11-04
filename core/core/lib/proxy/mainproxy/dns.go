package mainproxy

import (
	"net/url"
	"strings"

	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/structs"
)

func buildHappDNSSection(selection structs.RoutingSelection, fakeDNS bool) (map[string]interface{}, bool, bool) {
	cfg := selection.Config.DNS
	domesticAddr := dnsServerAddress(cfg.DomesticDNSType, cfg.DomesticDNSDomain, cfg.DomesticDNSIP)
	remoteAddr := dnsServerAddress(cfg.RemoteDNSType, cfg.RemoteDNSDomain, cfg.RemoteDNSIP)

	directDomains := domainsForOutbound(selection.Config.Rules, "direct")
	proxyDomains := domainsForOutbound(selection.Config.Rules, "proxy")

	var servers []map[string]interface{}
	hasDomestic := false
	hasRemote := false

	if selection.GlobalProxy {
		// Everything not explicitly direct goes through the proxy. The domestic
		// server is only useful for direct domains; remote is the fallback.
		if domesticAddr != "" && len(directDomains) > 0 {
			servers = append(servers, dnsServer("domestic-dns", domesticAddr, directDomains, true))
			hasDomestic = true
		}
		if remoteAddr != "" {
			servers = append(servers, dnsServer("remote-dns", remoteAddr, nil, false))
			hasRemote = true
		}
	} else {
		// GlobalProxy=false: everything is direct by default. Domestic is the
		// fallback resolver; remote is used only for explicit proxy domains.
		if domesticAddr != "" {
			servers = append(servers, dnsServer("domestic-dns", domesticAddr, nil, false))
			hasDomestic = true
		}
		if remoteAddr != "" {
			if domesticAddr == "" {
				servers = append(servers, dnsServer("remote-dns", remoteAddr, nil, false))
				hasRemote = true
			} else if len(proxyDomains) > 0 {
				servers = append(servers, dnsServer("remote-dns", remoteAddr, proxyDomains, true))
				hasRemote = true
			}
		}
	}

	// Unmatched queries need at least one server without a domains filter.
	if !hasFallbackServer(servers) {
		servers = append(servers, fallbackDNSServers()...)
	}
	if servers == nil {
		servers = fallbackDNSServers()
	}
	if fakeDNS {
		servers = append([]map[string]interface{}{
			{"address": "fakedns", "tag": "fakedns"},
		}, servers...)
	}

	section := map[string]interface{}{
		"servers": servers,
	}
	if hosts := buildDNSHosts(selection); len(hosts) > 0 {
		section["hosts"] = hosts
	}
	return section, hasDomestic, hasRemote
}

func dnsServerAddress(kind, domain, ip string) string {
	switch kind {
	case "DoH":
		return domain
	case "DoU":
		return ip
	}
	if domain != "" {
		return domain
	}
	return ip
}

func dnsServer(tag, address string, domains []string, skipFallback bool) map[string]interface{} {
	server := map[string]interface{}{
		"address": address,
		"tag":     tag,
	}
	if len(domains) > 0 {
		server["domains"] = domains
		server["skipFallback"] = skipFallback
	}
	return server
}

func fallbackDNSServers() []map[string]interface{} {
	configured := appconfig.GetConfig().DnsServers
	servers := make([]map[string]interface{}, 0, len(configured))
	for _, address := range configured {
		servers = append(servers, map[string]interface{}{"address": address})
	}
	return servers
}

func hasFallbackServer(servers []map[string]interface{}) bool {
	for _, server := range servers {
		if _, hasDomains := server["domains"]; !hasDomains {
			return true
		}
	}
	return false
}

// buildDNSHosts merges static hosts from the profile, the DoH/DoU bootstrap
// entries, and the already resolved VPN server addresses. Earlier entries win.
func buildDNSHosts(selection structs.RoutingSelection) map[string]interface{} {
	cfg := selection.Config.DNS
	hosts := make(map[string]interface{})

	for domain, ip := range cfg.Hosts {
		addDNSHost(hosts, domain, ip)
	}
	if cfg.RemoteDNSDomain != "" && cfg.RemoteDNSIP != "" {
		addDNSHost(hosts, hostFromDNSAddress(cfg.RemoteDNSDomain), cfg.RemoteDNSIP)
	}
	if cfg.DomesticDNSDomain != "" && cfg.DomesticDNSIP != "" {
		addDNSHost(hosts, hostFromDNSAddress(cfg.DomesticDNSDomain), cfg.DomesticDNSIP)
	}
	for domain, ips := range selection.BootstrapHosts {
		addDNSHost(hosts, domain, ips...)
	}

	return hosts
}

func addDNSHost(hosts map[string]interface{}, domain string, ips ...string) {
	domain = strings.TrimSpace(domain)
	if domain == "" {
		return
	}
	if _, exists := hosts[domain]; exists {
		return
	}

	values := make([]string, 0, len(ips))
	for _, ip := range ips {
		if ip = strings.TrimSpace(ip); ip != "" {
			values = append(values, ip)
		}
	}
	switch len(values) {
	case 0:
		return
	case 1:
		hosts[domain] = values[0]
	default:
		hosts[domain] = values
	}
}

func hostFromDNSAddress(address string) string {
	address = strings.TrimSpace(address)
	if address == "" {
		return ""
	}
	if u, err := url.Parse(address); err == nil && u.Hostname() != "" {
		return u.Hostname()
	}
	return address
}

// domainsForOutbound collects domain matchers from the compiled rules for one
// outbound tag. The same patterns can safely be reused by Xray's DNS server
// selection (including geosite:/regexp: forms).
func domainsForOutbound(rules []structs.RoutingRule, outboundTag string) []string {
	seen := make(map[string]struct{})
	var domains []string
	for _, rule := range rules {
		if !rule.Enabled || rule.OutboundTag != outboundTag || strings.TrimSpace(rule.Domain) == "" {
			continue
		}
		for _, domain := range splitAndTrim(rule.Domain) {
			key := strings.ToLower(domain)
			if _, exists := seen[key]; exists {
				continue
			}
			seen[key] = struct{}{}
			domains = append(domains, domain)
		}
	}
	return domains
}

func outboundTagExists(outbounds []interface{}, tag string) bool {
	for _, raw := range outbounds {
		outbound, ok := raw.(map[string]interface{})
		if !ok {
			continue
		}
		if outbound["tag"] == tag {
			return true
		}
	}
	return false
}

func splitAndTrim(s string) []string {
	parts := strings.Split(s, ",")
	var result []string
	for _, p := range parts {
		p = strings.TrimSpace(p)
		if p != "" {
			result = append(result, p)
		}
	}
	return result
}

// ReloadRouting recompiles the routing configuration and restarts xray-core
// ReloadRouting hot-reloads the running xray-core instance
// with the new rules in-place, without dropping connection state or resetting
// the TUN device. If the proxy is disconnected, ReloadRouting is a no-op.
