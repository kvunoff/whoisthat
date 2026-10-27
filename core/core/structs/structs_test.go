package structs

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestRoutingConfigDNSIsOmittedWhenNil(t *testing.T) {
	raw, err := json.Marshal(RoutingConfig{
		DomainStrategy: "IPIfNonMatch",
		Rules:          []RoutingRule{{Type: "field", OutboundTag: "direct", Enabled: true}},
	})
	if err != nil {
		t.Fatalf("marshal failed: %v", err)
	}
	if strings.Contains(string(raw), `"dns"`) {
		t.Errorf("manual config must not contain dns: %s", raw)
	}
}

func TestHappDNSConfigIsAdditive(t *testing.T) {
	// Zero value must serialize to an empty object so old peers see nothing new.
	raw, err := json.Marshal(HappDNSConfig{})
	if err != nil {
		t.Fatalf("marshal failed: %v", err)
	}
	if string(raw) != "{}" {
		t.Errorf("expected zero-value DNS config to marshal as {}, got %s", raw)
	}

	original := HappDNSConfig{
		FakeDNSEnabled:  true,
		RemoteDNSType:   "DoH",
		RemoteDNSDomain: "https://dns.google/dns-query",
		RemoteDNSIP:     "8.8.8.8",
		Hosts:           map[string]string{"vpn.example": "203.0.113.7"},
	}
	raw, err = json.Marshal(original)
	if err != nil {
		t.Fatalf("marshal failed: %v", err)
	}

	var decoded HappDNSConfig
	if err := json.Unmarshal(raw, &decoded); err != nil {
		t.Fatalf("unmarshal failed: %v", err)
	}
	if decoded.RemoteDNSType != original.RemoteDNSType ||
		decoded.RemoteDNSDomain != original.RemoteDNSDomain ||
		decoded.Hosts["vpn.example"] != "203.0.113.7" {
		t.Errorf("round-trip mismatch: %+v", decoded)
	}
}
