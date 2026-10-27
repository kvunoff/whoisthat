package happdns

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestParse_FullConfig(t *testing.T) {
	cfg, warnings := Parse(Raw{
		RemoteDNSType:   "DoH",
		RemoteDNSDomain: "https://cloudflare-dns.com/dns-query",
		RemoteDNSIP:     "1.1.1.1",
		DomesticDNSType: "dou",
		DomesticDNSIP:   "8.8.8.8",
		DnsHosts:        json.RawMessage(`{"vpn.example.com":"203.0.113.7","broken":"not-an-ip"}`),
		FakeDNS:         json.RawMessage(`"true"`),
	})

	if cfg == nil {
		t.Fatal("expected DNS config, got nil")
	}
	if !cfg.FakeDNSEnabled {
		t.Error("expected FakeDNSEnabled=true")
	}
	if cfg.RemoteDNSType != "DoH" || cfg.RemoteDNSDomain != "https://cloudflare-dns.com/dns-query" || cfg.RemoteDNSIP != "1.1.1.1" {
		t.Errorf("unexpected remote DNS: %+v", cfg)
	}
	if cfg.DomesticDNSType != "DoU" || cfg.DomesticDNSDomain != "" || cfg.DomesticDNSIP != "8.8.8.8" {
		t.Errorf("unexpected domestic DNS: %+v", cfg)
	}
	if cfg.Hosts["vpn.example.com"] != "203.0.113.7" || len(cfg.Hosts) != 1 {
		t.Errorf("unexpected hosts: %+v", cfg.Hosts)
	}
	if len(warnings) != 1 || !strings.Contains(warnings[0], "not-an-ip") {
		t.Errorf("expected one warning about broken host, got %v", warnings)
	}

	if got := RemoteLabel(cfg); got != "DoH cloudflare-dns.com" {
		t.Errorf("RemoteLabel = %q", got)
	}
	if got := DomesticLabel(cfg); got != "DoU 8.8.8.8" {
		t.Errorf("DomesticLabel = %q", got)
	}
}

func TestParse_InfersType(t *testing.T) {
	cfg, warnings := Parse(Raw{
		RemoteDNSDomain: "dns.google/dns-query",
		DomesticDNSIP:   "9.9.9.9",
	})
	if cfg == nil {
		t.Fatal("expected DNS config, got nil")
	}
	if len(warnings) != 0 {
		t.Errorf("unexpected warnings: %v", warnings)
	}
	if cfg.RemoteDNSType != "DoH" || cfg.RemoteDNSDomain != "https://dns.google/dns-query" {
		t.Errorf("expected inferred DoH, got %+v", cfg)
	}
	if cfg.DomesticDNSType != "DoU" || cfg.DomesticDNSIP != "9.9.9.9" {
		t.Errorf("expected inferred DoU, got %+v", cfg)
	}
}

func TestParse_InvalidValuesFallBack(t *testing.T) {
	cfg, warnings := Parse(Raw{
		RemoteDNSType:   "DoH",
		RemoteDNSDomain: "http://insecure.example/dns-query",
		RemoteDNSIP:     "1.1.1.1",
		DomesticDNSType: "DoU",
		DomesticDNSIP:   "not-an-ip",
	})

	if cfg == nil {
		t.Fatal("expected DNS config, got nil")
	}
	if cfg.RemoteDNSType != "DoU" || cfg.RemoteDNSIP != "1.1.1.1" {
		t.Errorf("expected DoH -> DoU fallback, got %+v", cfg)
	}
	if cfg.DomesticDNSType != "" {
		t.Errorf("expected invalid DoU to be dropped, got %+v", cfg)
	}
	if len(warnings) < 2 {
		t.Errorf("expected warnings for both servers, got %v", warnings)
	}
}

func TestParse_UnknownTypeAndBadFakeDNS(t *testing.T) {
	cfg, warnings := Parse(Raw{
		RemoteDNSType: "DoQ",
		RemoteDNSIP:   "1.1.1.1",
		FakeDNS:       json.RawMessage(`"maybe"`),
	})
	if cfg == nil {
		t.Fatal("expected DNS config, got nil")
	}
	if cfg.RemoteDNSType != "DoU" {
		t.Errorf("expected inference to DoU, got %+v", cfg)
	}
	if cfg.FakeDNSEnabled {
		t.Error("invalid FakeDNS must not enable fake DNS")
	}
	if len(warnings) < 2 {
		t.Errorf("expected warnings about type and FakeDNS, got %v", warnings)
	}
}

func TestParse_EmptyReturnsNil(t *testing.T) {
	cfg, warnings := Parse(Raw{})
	if cfg != nil {
		t.Errorf("expected nil config, got %+v", cfg)
	}
	if len(warnings) != 0 {
		t.Errorf("unexpected warnings: %v", warnings)
	}

	cfg, _ = Parse(Raw{FakeDNS: json.RawMessage(`false`)})
	if cfg != nil {
		t.Errorf("expected nil config for fake=false only, got %+v", cfg)
	}
}

func TestParse_HostsArrayTakesFirstIP(t *testing.T) {
	cfg, warnings := Parse(Raw{
		DnsHosts: json.RawMessage(`{"a.example":["not-an-ip","192.0.2.1","192.0.2.2"]}`),
	})
	if cfg == nil {
		t.Fatal("expected DNS config, got nil")
	}
	if cfg.Hosts["a.example"] != "192.0.2.1" {
		t.Errorf("unexpected host value: %+v", cfg.Hosts)
	}
	if len(warnings) != 0 {
		t.Errorf("unexpected warnings: %v", warnings)
	}
}

func TestParseRawJSON(t *testing.T) {
	cfg, warnings := ParseRawJSON(json.RawMessage(`{
		"Name":"P1",
		"RemoteDNSType":"DoH",
		"RemoteDNSDomain":"https://dns.google/dns-query",
		"DnsHosts":{"vpn.example":"203.0.113.9"}
	}`))
	if cfg == nil {
		t.Fatal("expected config, got nil")
	}
	if cfg.RemoteDNSType != "DoH" || cfg.Hosts["vpn.example"] != "203.0.113.9" {
		t.Errorf("unexpected config: %+v", cfg)
	}
	if len(warnings) != 0 {
		t.Errorf("unexpected warnings: %v", warnings)
	}

	cfg, warnings = ParseRawJSON(json.RawMessage(`not json`))
	if cfg != nil || len(warnings) == 0 {
		t.Errorf("expected nil + warning for broken JSON, got cfg=%+v warnings=%v", cfg, warnings)
	}
}

func TestFlexibleBool(t *testing.T) {
	cases := []struct {
		input    string
		expected bool
		wantErr  bool
	}{
		{`true`, true, false},
		{`"true"`, true, false},
		{`"1"`, true, false},
		{`"0"`, false, false},
		{`"FALSE"`, false, false},
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
			continue
		}
		if err != nil {
			t.Errorf("input %s: unexpected error: %v", tc.input, err)
		}
		if fb.Value() != tc.expected {
			t.Errorf("input %s: expected %v, got %v", tc.input, tc.expected, fb.Value())
		}
	}
}
