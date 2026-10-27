package mainproxy

import (
	"encoding/json"
	"testing"
	"whoisthat-core/structs"
)

func sampleBaseConfig(withAPI bool) []byte {
	m := map[string]interface{}{
		"inbounds": []interface{}{
			map[string]interface{}{
				"tag":      "socks-in",
				"protocol": "socks",
				"sniffing": map[string]interface{}{
					"enabled":   true,
					"routeOnly": true,
				},
			},
		},
		"outbounds": []interface{}{
			map[string]interface{}{
				"tag":      "proxy",
				"protocol": "vless",
			},
		},
	}
	if withAPI {
		m["api"] = map[string]interface{}{
			"tag":      "api",
			"services": []string{"StatsService"},
		}
	}
	b, _ := json.Marshal(m)
	return b
}

func TestInjectRoutingConfig_ManualBaseline(t *testing.T) {
	sel := structs.RoutingSelection{
		Config: structs.RoutingConfig{
			DomainStrategy: "IPIfNonMatch",
			Rules: []structs.RoutingRule{
				{
					Type:        "field",
					Domain:      "example.com,test.com",
					OutboundTag: "direct",
					Enabled:     true,
				},
				{
					Type:        "field",
					IP:          "1.2.3.4",
					OutboundTag: "block",
					Enabled:     false, // Disabled should be skipped
				},
			},
		},
		Source: "manual",
	}

	raw := sampleBaseConfig(false)
	injected, err := injectRoutingConfig(raw, sel, false)
	if err != nil {
		t.Fatalf("injectRoutingConfig failed: %v", err)
	}

	var parsed map[string]interface{}
	if err := json.Unmarshal(injected, &parsed); err != nil {
		t.Fatalf("failed to parse injected config: %v", err)
	}

	// Verify routing section
	routing, ok := parsed["routing"].(map[string]interface{})
	if !ok {
		t.Fatalf("routing section missing")
	}
	if routing["domainStrategy"] != "IPIfNonMatch" {
		t.Errorf("expected IPIfNonMatch, got %v", routing["domainStrategy"])
	}

	rules, ok := routing["rules"].([]interface{})
	if !ok {
		t.Fatalf("rules missing in routing")
	}

	// Rule 0: DNS rule (port 53 -> proxy)
	r0 := rules[0].(map[string]interface{})
	if r0["port"] != "53" || r0["outboundTag"] != "proxy" {
		t.Errorf("expected DNS rule at index 0, got %+v", r0)
	}

	// Rule 1: user rule (example.com -> direct)
	r1 := rules[1].(map[string]interface{})
	if r1["outboundTag"] != "direct" {
		t.Errorf("expected direct outboundTag at index 1, got %+v", r1)
	}
	domains := r1["domain"].([]interface{})
	if len(domains) != 2 || domains[0] != "example.com" || domains[1] != "test.com" {
		t.Errorf("expected domains [example.com test.com], got %v", domains)
	}

	// Only 2 rules total (disabled rule skipped)
	if len(rules) != 2 {
		t.Errorf("expected 2 rules, got %d", len(rules))
	}

	// Verify outbounds: proxy, direct, block
	outbounds := parsed["outbounds"].([]interface{})
	if len(outbounds) != 3 {
		t.Fatalf("expected 3 outbounds, got %d", len(outbounds))
	}
}

func TestInjectRoutingConfig_HappWithNetworkAndAPI(t *testing.T) {
	sel := structs.RoutingSelection{
		Config: structs.RoutingConfig{
			DomainStrategy: "AsIs",
			Rules: []structs.RoutingRule{
				{
					Type:        "field",
					IP:          "10.0.0.0/8",
					OutboundTag: "direct",
					Enabled:     true,
				},
				{
					Type:        "field",
					Network:     "tcp,udp",
					OutboundTag: "direct",
					Enabled:     true,
				},
			},
		},
		Source: "happ",
	}

	raw := sampleBaseConfig(true) // with API
	injected, err := injectRoutingConfig(raw, sel, false)
	if err != nil {
		t.Fatalf("injectRoutingConfig failed: %v", err)
	}

	var parsed map[string]interface{}
	if err := json.Unmarshal(injected, &parsed); err != nil {
		t.Fatalf("failed to parse injected config: %v", err)
	}

	routing := parsed["routing"].(map[string]interface{})
	rules := routing["rules"].([]interface{})

	// Rule 0: DNS rule
	r0 := rules[0].(map[string]interface{})
	if r0["port"] != "53" || r0["outboundTag"] != "proxy" {
		t.Errorf("expected DNS rule at index 0, got %+v", r0)
	}

	// Rule 1: API rule
	r1 := rules[1].(map[string]interface{})
	if r1["outboundTag"] != "api" {
		t.Errorf("expected api rule at index 1, got %+v", r1)
	}

	// Rule 2: IP rule
	r2 := rules[2].(map[string]interface{})
	if r2["outboundTag"] != "direct" {
		t.Errorf("expected direct rule at index 2, got %+v", r2)
	}

	// Rule 3: Catch-all rule with network "tcp,udp"
	r3 := rules[3].(map[string]interface{})
	if r3["network"] != "tcp,udp" || r3["outboundTag"] != "direct" {
		t.Errorf("expected network: tcp,udp rule at index 3, got %+v", r3)
	}
}

func TestInjectRoutingConfig_HappDNSInTun(t *testing.T) {
	sel := structs.RoutingSelection{
		Source:      "happ",
		GlobalProxy: true,
		Config: structs.RoutingConfig{
			DomainStrategy: "IPIfNonMatch",
			DNS: &structs.HappDNSConfig{
				RemoteDNSType:   "DoH",
				RemoteDNSDomain: "https://cloudflare-dns.com/dns-query",
				RemoteDNSIP:     "1.1.1.1",
				DomesticDNSType: "DoU",
				DomesticDNSIP:   "8.8.8.8",
				Hosts: map[string]string{
					"static.example": "203.0.113.5",
				},
			},
			Rules: []structs.RoutingRule{
				{Type: "field", IP: "10.0.0.0/8", OutboundTag: "direct", Enabled: true},
				{Type: "field", Domain: "direct.example,geosite:cn", OutboundTag: "direct", Enabled: true},
				{Type: "field", Domain: "proxy.example", OutboundTag: "proxy", Enabled: true},
			},
		},
		BootstrapHosts: map[string][]string{
			"vpn.example.com": {"203.0.113.7", "203.0.113.8"},
			"static.example":  {"198.51.100.1"},
		},
	}

	injected, err := injectRoutingConfig(sampleBaseConfig(true), sel, true)
	if err != nil {
		t.Fatalf("injectRoutingConfig failed: %v", err)
	}

	var parsed map[string]interface{}
	if err := json.Unmarshal(injected, &parsed); err != nil {
		t.Fatalf("failed to parse injected config: %v", err)
	}

	rules := parsed["routing"].(map[string]interface{})["rules"].([]interface{})
	if len(rules) != 7 {
		t.Fatalf("expected 7 rules, got %d: %+v", len(rules), rules)
	}

	assertRule(t, rules[0], "api", "")
	capture := rules[1].(map[string]interface{})
	if capture["outboundTag"] != "dns-out" || capture["port"] != "53" || capture["network"] != "tcp,udp" {
		t.Errorf("unexpected dns capture rule: %+v", capture)
	}
	if tags := capture["inboundTag"].([]interface{}); len(tags) != 1 || tags[0] != "tun-in" {
		t.Errorf("unexpected dns capture inbound tags: %+v", tags)
	}
	assertInboundRule(t, rules[2], "domestic-dns", "direct")
	assertInboundRule(t, rules[3], "remote-dns", "proxy")
	assertRule(t, rules[4], "direct", "")
	assertRule(t, rules[5], "direct", "")
	assertRule(t, rules[6], "proxy", "")

	for _, raw := range rules {
		rule := raw.(map[string]interface{})
		if rule["port"] == "53" && rule["outboundTag"] == "proxy" {
			t.Errorf("legacy port 53 rule must not be present for Happ DNS: %+v", rule)
		}
	}

	dnsSection := parsed["dns"].(map[string]interface{})
	hosts := dnsSection["hosts"].(map[string]interface{})
	assertHost(t, hosts, "static.example", "203.0.113.5")
	assertHost(t, hosts, "cloudflare-dns.com", "1.1.1.1")
	vpnHosts, ok := hosts["vpn.example.com"].([]interface{})
	if !ok || len(vpnHosts) != 2 || vpnHosts[0] != "203.0.113.7" || vpnHosts[1] != "203.0.113.8" {
		t.Errorf("unexpected bootstrap hosts: %#v", hosts["vpn.example.com"])
	}

	servers := dnsSection["servers"].([]interface{})
	if len(servers) != 2 {
		t.Fatalf("expected 2 DNS servers, got %d: %+v", len(servers), servers)
	}
	domestic := servers[0].(map[string]interface{})
	if domestic["tag"] != "domestic-dns" || domestic["address"] != "8.8.8.8" || domestic["skipFallback"] != true {
		t.Errorf("unexpected domestic server: %+v", domestic)
	}
	assertDomains(t, domestic, "direct.example", "geosite:cn")

	remote := servers[1].(map[string]interface{})
	if remote["tag"] != "remote-dns" || remote["address"] != "https://cloudflare-dns.com/dns-query" {
		t.Errorf("unexpected remote server: %+v", remote)
	}
	if _, hasDomains := remote["domains"]; hasDomains {
		t.Errorf("remote fallback must not have domains: %+v", remote)
	}

	if !hasOutbound(parsed, "dns-out", "dns") {
		t.Errorf("dns-out outbound missing: %+v", parsed["outbounds"])
	}
}

func TestInjectRoutingConfig_HappDNSProxyMode(t *testing.T) {
	sel := structs.RoutingSelection{
		Source:      "happ",
		GlobalProxy: true,
		Config: structs.RoutingConfig{
			DNS: &structs.HappDNSConfig{
				RemoteDNSType:   "DoU",
				RemoteDNSIP:     "1.1.1.1",
				DomesticDNSType: "DoU",
				DomesticDNSIP:   "8.8.8.8",
			},
			Rules: []structs.RoutingRule{
				{Type: "field", Domain: "direct.example", OutboundTag: "direct", Enabled: true},
			},
		},
	}

	injected, err := injectRoutingConfig(sampleBaseConfig(false), sel, false)
	if err != nil {
		t.Fatalf("injectRoutingConfig failed: %v", err)
	}

	var parsed map[string]interface{}
	if err := json.Unmarshal(injected, &parsed); err != nil {
		t.Fatalf("failed to parse injected config: %v", err)
	}

	rules := parsed["routing"].(map[string]interface{})["rules"].([]interface{})
	if len(rules) != 3 {
		t.Fatalf("expected 3 rules, got %d: %+v", len(rules), rules)
	}
	assertInboundRule(t, rules[0], "domestic-dns", "direct")
	assertInboundRule(t, rules[1], "remote-dns", "proxy")
	assertRule(t, rules[2], "direct", "")

	for _, raw := range rules {
		rule := raw.(map[string]interface{})
		if rule["port"] == "53" {
			t.Errorf("proxy mode must not capture port 53: %+v", rule)
		}
	}
	if hasOutbound(parsed, "dns-out", "dns") {
		t.Error("dns-out must not be emitted without TUN")
	}
	if _, ok := parsed["dns"].(map[string]interface{}); !ok {
		t.Error("DNS section must still be emitted in proxy mode")
	}
}

func TestInjectRoutingConfig_GlobalProxyFalseDNS(t *testing.T) {
	sel := structs.RoutingSelection{
		Source:      "happ",
		GlobalProxy: false,
		Config: structs.RoutingConfig{
			DNS: &structs.HappDNSConfig{
				RemoteDNSType:   "DoH",
				RemoteDNSDomain: "https://dns.google/dns-query",
				RemoteDNSIP:     "8.8.8.8",
				DomesticDNSType: "DoU",
				DomesticDNSIP:   "1.1.1.1",
			},
			Rules: []structs.RoutingRule{
				{Type: "field", Domain: "direct.example", OutboundTag: "direct", Enabled: true},
				{Type: "field", Domain: "proxy.example", OutboundTag: "proxy", Enabled: true},
			},
		},
	}

	injected, err := injectRoutingConfig(sampleBaseConfig(false), sel, false)
	if err != nil {
		t.Fatalf("injectRoutingConfig failed: %v", err)
	}

	var parsed map[string]interface{}
	if err := json.Unmarshal(injected, &parsed); err != nil {
		t.Fatalf("failed to parse injected config: %v", err)
	}

	rules := parsed["routing"].(map[string]interface{})["rules"].([]interface{})
	if len(rules) != 4 {
		t.Fatalf("expected 4 rules, got %d: %+v", len(rules), rules)
	}
	assertInboundRule(t, rules[0], "domestic-dns", "direct")
	assertInboundRule(t, rules[1], "remote-dns", "proxy")

	servers := parsed["dns"].(map[string]interface{})["servers"].([]interface{})
	if len(servers) != 2 {
		t.Fatalf("expected 2 DNS servers, got %d: %+v", len(servers), servers)
	}
	domestic := servers[0].(map[string]interface{})
	if _, hasDomains := domestic["domains"]; hasDomains {
		t.Errorf("domestic fallback must not have domains: %+v", domestic)
	}
	remote := servers[1].(map[string]interface{})
	if remote["skipFallback"] != true {
		t.Errorf("proxy-domain server must skip fallback: %+v", remote)
	}
	assertDomains(t, remote, "proxy.example")
}

func TestInjectRoutingConfig_HappDNSOverwritesSection(t *testing.T) {
	var base map[string]interface{}
	if err := json.Unmarshal(sampleBaseConfig(false), &base); err != nil {
		t.Fatal(err)
	}
	base["dns"] = map[string]interface{}{
		"servers": []interface{}{map[string]interface{}{"address": "9.9.9.9"}},
	}
	raw, _ := json.Marshal(base)

	sel := structs.RoutingSelection{
		Source:      "happ",
		GlobalProxy: true,
		Config: structs.RoutingConfig{
			DNS: &structs.HappDNSConfig{
				RemoteDNSType: "DoU",
				RemoteDNSIP:   "1.1.1.1",
			},
		},
	}

	injected, err := injectRoutingConfig(raw, sel, false)
	if err != nil {
		t.Fatalf("injectRoutingConfig failed: %v", err)
	}
	var parsed map[string]interface{}
	if err := json.Unmarshal(injected, &parsed); err != nil {
		t.Fatal(err)
	}
	servers := parsed["dns"].(map[string]interface{})["servers"].([]interface{})
	if len(servers) != 1 || servers[0].(map[string]interface{})["address"] != "1.1.1.1" {
		t.Errorf("Happ DNS must replace the existing section: %+v", servers)
	}

	// Manual selections keep an existing DNS section untouched.
	manual := structs.RoutingSelection{Source: "manual"}
	injected, err = injectRoutingConfig(raw, manual, false)
	if err != nil {
		t.Fatalf("injectRoutingConfig manual failed: %v", err)
	}
	if err := json.Unmarshal(injected, &parsed); err != nil {
		t.Fatal(err)
	}
	servers = parsed["dns"].(map[string]interface{})["servers"].([]interface{})
	if len(servers) != 1 || servers[0].(map[string]interface{})["address"] != "9.9.9.9" {
		t.Errorf("manual mode must preserve the existing DNS section: %+v", servers)
	}
}

func TestInjectRoutingConfig_HappDNSNoDirectDomains(t *testing.T) {
	sel := structs.RoutingSelection{
		Source:      "happ",
		GlobalProxy: true,
		Config: structs.RoutingConfig{
			DNS: &structs.HappDNSConfig{
				RemoteDNSType:   "DoU",
				RemoteDNSIP:     "1.1.1.1",
				DomesticDNSType: "DoU",
				DomesticDNSIP:   "8.8.8.8",
			},
			Rules: []structs.RoutingRule{
				{Type: "field", Domain: "proxy.example", OutboundTag: "proxy", Enabled: true},
			},
		},
	}

	injected, err := injectRoutingConfig(sampleBaseConfig(false), sel, false)
	if err != nil {
		t.Fatalf("injectRoutingConfig failed: %v", err)
	}
	var parsed map[string]interface{}
	if err := json.Unmarshal(injected, &parsed); err != nil {
		t.Fatal(err)
	}

	servers := parsed["dns"].(map[string]interface{})["servers"].([]interface{})
	if len(servers) != 1 || servers[0].(map[string]interface{})["tag"] != "remote-dns" {
		t.Errorf("domestic server must be skipped without direct domains: %+v", servers)
	}

	rules := parsed["routing"].(map[string]interface{})["rules"].([]interface{})
	for _, raw := range rules {
		if rule := raw.(map[string]interface{}); rule["outboundTag"] == "direct" {
			if tags, ok := rule["inboundTag"].([]interface{}); ok && len(tags) == 1 && tags[0] == "domestic-dns" {
				t.Errorf("domestic-dns rule must not be emitted without a server: %+v", rule)
			}
		}
	}
}

func TestInjectRoutingConfig_HappDNSFallsBackToConfiguredServers(t *testing.T) {
	sel := structs.RoutingSelection{
		Source:      "happ",
		GlobalProxy: true,
		Config: structs.RoutingConfig{
			DNS: &structs.HappDNSConfig{
				FakeDNSEnabled: true,
				Hosts:          map[string]string{"static.example": "203.0.113.5"},
			},
		},
	}

	injected, err := injectRoutingConfig(sampleBaseConfig(false), sel, true)
	if err != nil {
		t.Fatalf("injectRoutingConfig failed: %v", err)
	}
	var parsed map[string]interface{}
	if err := json.Unmarshal(injected, &parsed); err != nil {
		t.Fatal(err)
	}

	dnsSection := parsed["dns"].(map[string]interface{})
	servers := dnsSection["servers"].([]interface{})
	if len(servers) == 0 {
		t.Fatal("expected configured fallback DNS servers")
	}
	for _, raw := range servers {
		server := raw.(map[string]interface{})
		if server["address"] == "fakedns" {
			t.Errorf("fake DNS must not be emitted yet: %+v", server)
		}
	}
	hosts := dnsSection["hosts"].(map[string]interface{})
	assertHost(t, hosts, "static.example", "203.0.113.5")

	rules := parsed["routing"].(map[string]interface{})["rules"].([]interface{})
	if len(rules) != 1 || rules[0].(map[string]interface{})["outboundTag"] != "dns-out" {
		t.Errorf("expected only the dns capture rule, got %+v", rules)
	}
}

func assertRule(t *testing.T, raw interface{}, outbound, domain string) {
	t.Helper()
	rule := raw.(map[string]interface{})
	if rule["outboundTag"] != outbound {
		t.Errorf("expected outbound %q, got %+v", outbound, rule)
	}
	if domain != "" {
		domains := rule["domain"].([]interface{})
		if len(domains) != 1 || domains[0] != domain {
			t.Errorf("expected domain %q, got %+v", domain, domains)
		}
	}
}

func assertInboundRule(t *testing.T, raw interface{}, inbound, outbound string) {
	t.Helper()
	rule := raw.(map[string]interface{})
	tags, ok := rule["inboundTag"].([]interface{})
	if !ok || len(tags) != 1 || tags[0] != inbound {
		t.Errorf("expected inbound %q, got %+v", inbound, rule)
	}
	if rule["outboundTag"] != outbound {
		t.Errorf("expected outbound %q, got %+v", outbound, rule)
	}
}

func assertHost(t *testing.T, hosts map[string]interface{}, domain, ip string) {
	t.Helper()
	if got, ok := hosts[domain].(string); !ok || got != ip {
		t.Errorf("expected host %q -> %q, got %#v", domain, ip, hosts[domain])
	}
}

func assertDomains(t *testing.T, server map[string]interface{}, expected ...string) {
	t.Helper()
	raw, ok := server["domains"].([]interface{})
	if !ok {
		t.Fatalf("server has no domains: %+v", server)
	}
	if len(raw) != len(expected) {
		t.Fatalf("expected domains %v, got %v", expected, raw)
	}
	for i, domain := range expected {
		if raw[i] != domain {
			t.Errorf("domain %d: expected %q, got %v", i, domain, raw[i])
		}
	}
}

func hasOutbound(parsed map[string]interface{}, tag, protocol string) bool {
	outbounds, _ := parsed["outbounds"].([]interface{})
	for _, raw := range outbounds {
		outbound, ok := raw.(map[string]interface{})
		if !ok {
			continue
		}
		if outbound["tag"] == tag && outbound["protocol"] == protocol {
			return true
		}
	}
	return false
}
