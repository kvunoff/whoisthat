package happ

import (
	"encoding/base64"
	"encoding/json"
	"strings"
	"testing"
)

func TestParseLink_Actions(t *testing.T) {
	sampleProfile := `{"Name":"TestProfile","GlobalProxy":true}`
	b64 := base64.StdEncoding.EncodeToString([]byte(sampleProfile))

	// Action Add
	linkAdd := "happ://routing/add/" + b64
	parsed, err := ParseLink(linkAdd)
	if err != nil {
		t.Fatalf("ParseLink add failed: %v", err)
	}
	if parsed.Action != ActionAdd {
		t.Errorf("expected ActionAdd, got %v", parsed.Action)
	}
	if parsed.Profile.Name != "TestProfile" {
		t.Errorf("expected TestProfile, got %v", parsed.Profile.Name)
	}

	// Action OnAdd with uppercase
	linkOnAdd := "HAPP://ROUTING/ONADD/" + b64
	parsedOnAdd, err := ParseLink(linkOnAdd)
	if err != nil {
		t.Fatalf("ParseLink onadd failed: %v", err)
	}
	if parsedOnAdd.Action != ActionOnAdd {
		t.Errorf("expected ActionOnAdd, got %v", parsedOnAdd.Action)
	}

	// Action Off
	parsedOff, err := ParseLink("happ://routing/off")
	if err != nil {
		t.Fatalf("ParseLink off failed: %v", err)
	}
	if parsedOff.Action != ActionOff {
		t.Errorf("expected ActionOff, got %v", parsedOff.Action)
	}
	if parsedOff.Profile != nil {
		t.Errorf("expected nil profile for ActionOff, got %v", parsedOff.Profile)
	}

	// Action Off with trailing slash
	parsedOffSlash, err := ParseLink("happ://routing/off/")
	if err != nil {
		t.Fatalf("ParseLink off/ failed: %v", err)
	}
	if parsedOffSlash.Action != ActionOff {
		t.Errorf("expected ActionOff, got %v", parsedOffSlash.Action)
	}
}

func TestParseLink_Base64Variants(t *testing.T) {
	// A JSON payload containing chars that test +, /, =
	jsonContent := `{"Name":"Complex+Profile/Name==","DirectSites":["site1.com","site2.com"]}`

	std := base64.StdEncoding.EncodeToString([]byte(jsonContent))
	rawStd := base64.RawStdEncoding.EncodeToString([]byte(jsonContent))
	urlEnc := base64.URLEncoding.EncodeToString([]byte(jsonContent))
	rawUrl := base64.RawURLEncoding.EncodeToString([]byte(jsonContent))

	for name, enc := range map[string]string{
		"std":      std,
		"rawStd":   rawStd,
		"url":      urlEnc,
		"rawUrl":   rawUrl,
		"newlines": "\r\n " + std + "\t\n",
	} {
		t.Run(name, func(t *testing.T) {
			parsed, err := ParseLink("happ://routing/add/" + enc)
			if err != nil {
				t.Fatalf("failed for %s: %v", name, err)
			}
			if parsed.Profile.Name != "Complex+Profile/Name==" {
				t.Errorf("name mismatch for %s: got %s", name, parsed.Profile.Name)
			}
		})
	}
}

func TestParseLink_PercentEncodedAndSpaces(t *testing.T) {
	jsonContent := `{"Name":"Test","ProxySites":["google.com"]}`
	b64 := base64.StdEncoding.EncodeToString([]byte(jsonContent))

	// Replace + with space (simulating form query decoding issue)
	withSpaces := strings.ReplaceAll(b64, "+", " ")
	parsed, err := ParseLink("happ://routing/add/" + withSpaces)
	if err != nil {
		t.Fatalf("failed to decode base64 with spaces: %v", err)
	}
	if parsed.Profile.Name != "Test" {
		t.Errorf("expected Test, got %s", parsed.Profile.Name)
	}

	// Percent-encoded link
	escapedScheme := "happ%3A%2F%2Frouting%2Fadd%2F" + b64
	parsedEsc, err := ParseLink(escapedScheme)
	if err != nil {
		t.Fatalf("failed to decode percent-encoded link: %v", err)
	}
	if parsedEsc.Profile.Name != "Test" {
		t.Errorf("expected Test, got %s", parsedEsc.Profile.Name)
	}
}

func TestParseLink_Errors(t *testing.T) {
	// Empty link
	if _, err := ParseLink(""); err == nil {
		t.Error("expected error for empty link")
	}

	// Invalid scheme
	if _, err := ParseLink("vless://test"); err == nil {
		t.Error("expected error for invalid scheme")
	}

	// Unknown action
	if _, err := ParseLink("happ://routing/unknown/123"); err == nil {
		t.Error("expected error for unknown action")
	}

	// Invalid base64
	if _, err := ParseLink("happ://routing/add/@@@not-base64@@@"); err == nil {
		t.Error("expected error for invalid base64")
	}

	// Not JSON
	badJSON := base64.StdEncoding.EncodeToString([]byte("this is not json"))
	if _, err := ParseLink("happ://routing/add/" + badJSON); err == nil {
		t.Error("expected error for invalid json")
	}

	// Missing Name
	noName := base64.StdEncoding.EncodeToString([]byte(`{"GlobalProxy":true}`))
	if _, err := ParseLink("happ://routing/add/" + noName); err == nil {
		t.Error("expected error for missing profile Name")
	}

	// Oversized payload
	huge := make([]byte, MaxPayloadBytes+10)
	hugeB64 := base64.StdEncoding.EncodeToString(huge)
	if _, err := ParseLink("happ://routing/add/" + hugeB64); err == nil {
		t.Error("expected error for oversized payload")
	}
}

func TestFlexibleBool(t *testing.T) {
	cases := []struct {
		input    string
		expected bool
		wantErr  bool
	}{
		{`true`, true, false},
		{`false`, false, false},
		{`"true"`, true, false},
		{`"false"`, false, false},
		{`"1"`, true, false},
		{`"0"`, false, false},
		{`"TRUE"`, true, false},
		{`"False"`, false, false},
		{`"invalid"`, false, true},
		{`123`, false, true},
	}

	for _, tc := range cases {
		var fb FlexibleBool
		err := json.Unmarshal([]byte(tc.input), &fb)
		if tc.wantErr {
			if err == nil {
				t.Errorf("input %s: expected error, got nil", tc.input)
			}
		} else {
			if err != nil {
				t.Errorf("input %s: unexpected error: %v", tc.input, err)
			}
			if fb.Value() != tc.expected {
				t.Errorf("input %s: expected %v, got %v", tc.input, tc.expected, fb.Value())
			}
		}
	}
}

func TestFlexibleUnix(t *testing.T) {
	cases := []struct {
		input       string
		expectedVal int64
		wantInvalid bool
	}{
		{`1760000000`, 1760000000, false},
		{`"1760000000"`, 1760000000, false},
		{`"1760000000.0"`, 1760000000, false},
		{`""`, 0, false},
		{`null`, 0, false},
		{`"garbage"`, 0, true},
	}

	for _, tc := range cases {
		var fu FlexibleUnix
		err := json.Unmarshal([]byte(tc.input), &fu)
		if err != nil {
			t.Errorf("input %s: should never return error on unmarshal: %v", tc.input, err)
		}
		if fu.Value() != tc.expectedVal {
			t.Errorf("input %s: expected %d, got %d", tc.input, tc.expectedVal, fu.Value())
		}
		if fu.IsInvalid() != tc.wantInvalid {
			t.Errorf("input %s: expected invalid=%v, got %v", tc.input, tc.wantInvalid, fu.IsInvalid())
		}
	}
}

func TestToRoutingConfig_RuleOrderAndPriority(t *testing.T) {
	fbFalse := FlexibleBool(false)
	profile := HappProfile{
		Name:           "FullTest",
		GlobalProxy:    &fbFalse,
		BlockSites:     []string{"geosite:category-ads-all"},
		BlockIp:        []string{"geoip:bad"},
		DirectSites:    []string{"geosite:cn", "domain:example.cn"},
		DirectIp:       []string{"geoip:cn", "1.1.1.0/24"},
		ProxySites:     []string{"geosite:google"},
		ProxyIp:        []string{"geoip:us"},
		DomainStrategy: "AsIs",
		Geoipurl:       "https://example.com/geoip.dat",
		Geositeurl:     "https://example.com/geosite.dat",
		LastUpdated:    FlexibleUnix{val: 1760000000},
	}

	cfg, warnings, err := ToRoutingConfig(profile)
	if err != nil {
		t.Fatalf("ToRoutingConfig failed: %v", err)
	}
	if len(warnings) != 0 {
		t.Errorf("unexpected warnings: %v", warnings)
	}
	if cfg.DomainStrategy != "AsIs" {
		t.Errorf("expected AsIs, got %s", cfg.DomainStrategy)
	}

	// Expected rules order:
	// 0: LAN rule (direct)
	// 1: BlockSites (domain)
	// 2: BlockIp (ip)
	// 3: DirectSites (domain)
	// 4: DirectIp (ip)
	// 5: ProxySites (domain)
	// 6: ProxyIp (ip)
	// 7: Catch-all rule (network tcp,udp -> direct) because GlobalProxy == false
	expectedCount := 8
	if len(cfg.Rules) != expectedCount {
		t.Fatalf("expected %d rules, got %d: %+v", expectedCount, len(cfg.Rules), cfg.Rules)
	}

	// 0: LAN
	r0 := cfg.Rules[0]
	if r0.OutboundTag != "direct" || r0.IP != LANRuleIP || r0.Domain != "" {
		t.Errorf("rule 0 LAN error: %+v", r0)
	}

	// 1: BlockSites
	r1 := cfg.Rules[1]
	if r1.OutboundTag != "block" || r1.Domain != "geosite:category-ads-all" || r1.IP != "" {
		t.Errorf("rule 1 BlockSites error: %+v", r1)
	}

	// 2: BlockIp
	r2 := cfg.Rules[2]
	if r2.OutboundTag != "block" || r2.IP != "geoip:bad" || r2.Domain != "" {
		t.Errorf("rule 2 BlockIp error: %+v", r2)
	}

	// 3: DirectSites
	r3 := cfg.Rules[3]
	if r3.OutboundTag != "direct" || r3.Domain != "geosite:cn,domain:example.cn" || r3.IP != "" {
		t.Errorf("rule 3 DirectSites error: %+v", r3)
	}

	// 4: DirectIp
	r4 := cfg.Rules[4]
	if r4.OutboundTag != "direct" || r4.IP != "geoip:cn,1.1.1.0/24" || r4.Domain != "" {
		t.Errorf("rule 4 DirectIp error: %+v", r4)
	}

	// 5: ProxySites
	r5 := cfg.Rules[5]
	if r5.OutboundTag != "proxy" || r5.Domain != "geosite:google" || r5.IP != "" {
		t.Errorf("rule 5 ProxySites error: %+v", r5)
	}

	// 6: ProxyIp
	r6 := cfg.Rules[6]
	if r6.OutboundTag != "proxy" || r6.IP != "geoip:us" || r6.Domain != "" {
		t.Errorf("rule 6 ProxyIp error: %+v", r6)
	}

	// 7: Catch-all
	r7 := cfg.Rules[7]
	if r7.OutboundTag != "direct" || r7.Network != "tcp,udp" {
		t.Errorf("rule 7 catch-all error: %+v", r7)
	}
}

func TestToRoutingConfig_GlobalProxyDefaultsAndWarnings(t *testing.T) {
	// GlobalProxy nil should default to true (no catch-all rule added)
	profile := HappProfile{
		Name:           "DefaultGP",
		DomainStrategy: "unknown_strategy",
		Geoipurl:       "http://insecure.com/geoip.dat", // http should trigger warning
		LastUpdated:    FlexibleUnix{invalid: true, raw: "bad-time"},
	}

	cfg, warnings, err := ToRoutingConfig(profile)
	if err != nil {
		t.Fatalf("ToRoutingConfig failed: %v", err)
	}

	if cfg.DomainStrategy != "IPIfNonMatch" {
		t.Errorf("expected fallback to IPIfNonMatch, got %s", cfg.DomainStrategy)
	}

	// Should have LAN rule only (no catch-all since globalProxy=true by default)
	if len(cfg.Rules) != 1 {
		t.Fatalf("expected only LAN rule (1), got %d", len(cfg.Rules))
	}
	if cfg.Rules[0].IP != LANRuleIP {
		t.Errorf("expected LAN rule, got %+v", cfg.Rules[0])
	}

	// Check warnings: domain strategy, geoipurl http, last updated
	if len(warnings) < 3 {
		t.Errorf("expected at least 3 warnings, got %v", warnings)
	}
}

func TestToRoutingConfig_SeparateDomainAndIPInvariant(t *testing.T) {
	// Test that Domain and IP are NEVER combined in a single rule
	profile := HappProfile{
		Name:        "SeparateTest",
		DirectSites: []string{"domain:example.com"},
		DirectIp:    []string{"1.2.3.4"},
	}

	cfg, _, err := ToRoutingConfig(profile)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	for i, rule := range cfg.Rules {
		if rule.Domain != "" && rule.IP != "" {
			t.Errorf("rule %d violates invariant: has both Domain (%s) and IP (%s)", i, rule.Domain, rule.IP)
		}
	}
}

func TestToRoutingConfig_DNSSection(t *testing.T) {
	raw := `{
		"Name":"DNSProfile",
		"GlobalProxy":true,
		"RemoteDNSType":"DoH",
		"RemoteDNSDomain":"https://cloudflare-dns.com/dns-query",
		"RemoteDNSIP":"1.1.1.1",
		"DomesticDNSType":"DoU",
		"DomesticDNSIP":"8.8.8.8",
		"DnsHosts":{"vpn.example.com":"203.0.113.7"},
		"FakeDNS":"true"
	}`

	var profile HappProfile
	if err := json.Unmarshal([]byte(raw), &profile); err != nil {
		t.Fatalf("failed to unmarshal profile: %v", err)
	}

	cfg, warnings, err := ToRoutingConfig(profile)
	if err != nil {
		t.Fatalf("ToRoutingConfig failed: %v", err)
	}
	if len(warnings) != 0 {
		t.Errorf("unexpected warnings: %v", warnings)
	}
	if cfg.DNS == nil {
		t.Fatal("expected DNS config, got nil")
	}
	if !cfg.DNS.FakeDNSEnabled {
		t.Error("expected FakeDNSEnabled=true")
	}
	if cfg.DNS.RemoteDNSType != "DoH" || cfg.DNS.RemoteDNSDomain != "https://cloudflare-dns.com/dns-query" {
		t.Errorf("unexpected remote DNS: %+v", cfg.DNS)
	}
	if cfg.DNS.DomesticDNSType != "DoU" || cfg.DNS.DomesticDNSIP != "8.8.8.8" {
		t.Errorf("unexpected domestic DNS: %+v", cfg.DNS)
	}
	if cfg.DNS.Hosts["vpn.example.com"] != "203.0.113.7" {
		t.Errorf("unexpected hosts: %+v", cfg.DNS.Hosts)
	}
}

func TestToRoutingConfig_NoDNSSection(t *testing.T) {
	cfg, warnings, err := ToRoutingConfig(HappProfile{Name: "Plain"})
	if err != nil {
		t.Fatalf("ToRoutingConfig failed: %v", err)
	}
	if len(warnings) != 0 {
		t.Errorf("unexpected warnings: %v", warnings)
	}
	if cfg.DNS != nil {
		t.Errorf("expected nil DNS for profile without DNS fields, got %+v", cfg.DNS)
	}
}

func TestToRoutingConfig_InvalidDNSWarnsButKeepsProfile(t *testing.T) {
	var profile HappProfile
	raw := `{"Name":"BadDNS","RemoteDNSType":"DoH","RemoteDNSDomain":"http://insecure/dns-query"}`
	if err := json.Unmarshal([]byte(raw), &profile); err != nil {
		t.Fatalf("failed to unmarshal profile: %v", err)
	}

	cfg, warnings, err := ToRoutingConfig(profile)
	if err != nil {
		t.Fatalf("ToRoutingConfig must not fail on invalid DNS: %v", err)
	}
	if len(warnings) == 0 {
		t.Error("expected warnings for invalid DNS URL")
	}
	if cfg.DNS != nil {
		t.Errorf("expected unusable DNS to be dropped, got %+v", cfg.DNS)
	}
}
