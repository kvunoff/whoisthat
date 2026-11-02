package mainproxy

import (
	"encoding/json"
	"testing"

	"whoisthat-core/lib/xrayjson"
	"whoisthat-core/structs"
)

// TestJSONCatchAllConfigPassesXrayValidation checks that the generated config
// for a JSON subscription with a provider catch-all balancer is accepted by the
// real pinned Xray binary. Skips when the runtime is not installed.
func TestJSONCatchAllConfigPassesXrayValidation(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping xray binary integration test in -short mode")
	}
	if xrayTestBinary() == "" {
		t.Skip("xray binary not available")
	}

	raw := json.RawMessage(`{
		"remarks": "Multik-like",
		"routing": {
			"rules": [
				{"type": "field", "network": "tcp,udp", "balancerTag": "NL_Trojan_Balancer"}
			],
			"balancers": [
				{"tag": "NL_Trojan_Balancer", "selector": ["proxy-nl-trojan"], "strategy": {"type": "random"}}
			]
		},
		"outbounds": [
			{"tag": "proxy-nl-trojan", "protocol": "socks", "settings": {"servers": [{"address": "nl.example", "port": 1080}]}},
			{"tag": "proxy-nl-trojan-2", "protocol": "socks", "settings": {"servers": [{"address": "nl2.example", "port": 1080}]}},
			{"tag": "direct", "protocol": "freedom"},
			{"tag": "block", "protocol": "blackhole"},
			{"tag": "dns-out", "protocol": "dns"}
		]
	}`)

	prepared, err := xrayjson.Prepare(raw, xrayjson.Options{
		SocksPort: 3090,
		HttpPort:  3091,
		UserRules: []structs.RoutingRule{
			{Type: "field", Domain: "2ip.io", OutboundTag: "proxy", Enabled: true},
		},
	})
	if err != nil {
		t.Fatalf("Prepare: %v", err)
	}
	if err := validateXrayConfig(prepared.Config, ""); err != nil {
		t.Fatalf("generated catch-all config rejected by xray: %v", err)
	}
}
