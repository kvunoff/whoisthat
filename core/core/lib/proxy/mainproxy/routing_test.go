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
	injected, err := injectRoutingConfig(raw, sel)
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
	injected, err := injectRoutingConfig(raw, sel)
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
