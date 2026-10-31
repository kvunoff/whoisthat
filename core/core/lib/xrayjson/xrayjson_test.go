package xrayjson

import (
	"encoding/json"
	"testing"

	"whoisthat-core/structs"
)

const fixture = `[
  {
    "remarks": "Germany Auto",
    "dns": {"servers": ["https://8.8.8.8/dns-query"], "hosts": {"static.example": "203.0.113.5"}},
    "routing": {
      "domainStrategy": "IPIfNonMatch",
      "rules": [{"type": "field", "domain": ["geosite:youtube"], "balancerTag": "Provider_Balancer"}],
      "balancers": [{"tag": "Provider_Balancer", "selector": ["proxy-a", "proxy-b"], "strategy": {"type": "leastPing"}}]
    },
    "inbounds": [
      {"tag": "socks", "protocol": "socks", "port": 10808, "listen": "127.0.0.1"},
      {"tag": "http", "protocol": "http", "port": 10809, "listen": "127.0.0.1"}
    ],
    "outbounds": [
      {"tag": "proxy-a", "protocol": "vless", "settings": {"vnext": [{"address": "a.example.com", "port": 443}]},
       "streamSettings": {"network": "ws", "wsSettings": {"host": "a.example.com"}}},
      {"tag": "proxy-b", "protocol": "trojan", "settings": {"servers": [{"address": "b.example.com", "port": 8443}]}},
      {"tag": "direct", "protocol": "freedom"},
      {"tag": "block", "protocol": "blackhole"},
      {"tag": "dns-out", "protocol": "dns", "streamSettings": {"sockopt": {"dialerProxy": "missing-tag"}}}
    ]
  },
  {
    "remarks": "Simple",
    "routing": {"rules": [{"type": "field", "protocol": ["bittorrent"], "outboundTag": "direct"}]},
    "inbounds": [{"tag": "socks", "protocol": "socks", "port": 10808}],
    "outbounds": [
      {"tag": "proxy", "protocol": "shadowsocks", "settings": {"servers": [{"address": "1.1.1.1", "port": 443}]}},
      {"tag": "direct", "protocol": "freedom"},
      {"tag": "block", "protocol": "blackhole"}
    ]
  }
]`

func TestParseArray(t *testing.T) {
	profiles, err := Parse([]byte(fixture))
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	if len(profiles) != 2 {
		t.Fatalf("profiles = %d, want 2", len(profiles))
	}
	if profiles[0].Name != "Germany Auto" || profiles[0].Protocol != "vless" || profiles[0].Address != "a.example.com" || profiles[0].Port != 443 {
		t.Errorf("unexpected first profile: %+v", profiles[0])
	}
	if profiles[1].Protocol != "shadowsocks" || profiles[1].Address != "1.1.1.1" {
		t.Errorf("unexpected second profile: %+v", profiles[1])
	}
	if profiles[0].ProviderKey == profiles[1].ProviderKey {
		t.Error("provider keys must be unique")
	}
}

func TestParseSingleObjectAndInvalid(t *testing.T) {
	single := `{"remarks":"Only","outbounds":[{"tag":"proxy","protocol":"vless","settings":{"vnext":[{"address":"x.example","port":1}]}}]}`
	profiles, err := Parse([]byte(single))
	if err != nil || len(profiles) != 1 {
		t.Fatalf("single object: profiles=%d err=%v", len(profiles), err)
	}
	if _, err := Parse([]byte("vless://abc@example.com:443")); err == nil {
		t.Fatal("share-link body must not be treated as JSON")
	}
	if IsConfigBody([]byte(`{"error":"nope"}`)) {
		t.Fatal("error object must not pass IsConfigBody")
	}
}

func TestServerAddresses(t *testing.T) {
	profiles, _ := Parse([]byte(fixture))
	refs := ServerAddresses(profiles[0].Raw)
	got := map[string]bool{}
	for _, ref := range refs {
		got[ref.Host] = true
	}
	for _, host := range []string{"a.example.com", "b.example.com"} {
		if !got[host] {
			t.Errorf("missing server address %s: %+v", host, refs)
		}
	}
}

func TestPrepareMergesManualRulesAndKeepsProviderRouting(t *testing.T) {
	profiles, _ := Parse([]byte(fixture))
	prepared, err := Prepare(profiles[0].Raw, Options{
		SocksPort:      3090,
		HttpPort:       3091,
		TunName:        "tun0",
		APIPort:        3095,
		DNSServers:     []string{"1.1.1.1"},
		BootstrapHosts: map[string][]string{"a.example.com": {"203.0.113.7"}},
		UserRules: []structs.RoutingRule{
			{Type: "field", Domain: "override.example", OutboundTag: "proxy", Enabled: true},
			{Type: "field", Domain: "ignored.example", OutboundTag: "proxy", Enabled: false},
		},
	})
	if err != nil {
		t.Fatalf("Prepare: %v", err)
	}

	var cfg map[string]interface{}
	if err := json.Unmarshal(prepared.Config, &cfg); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}

	inbounds := cfg["inbounds"].([]interface{})
	if len(inbounds) != 3 {
		t.Fatalf("inbounds = %d, want socks+http+tun", len(inbounds))
	}
	socks := inbounds[0].(map[string]interface{})
	if socks["port"] != float64(3090) || socks["listen"] != "127.0.0.1" {
		t.Errorf("socks inbound not rewritten: %+v", socks)
	}
	tun := inbounds[2].(map[string]interface{})
	if tun["tag"] != "tun-in" {
		t.Errorf("tun inbound missing: %+v", tun)
	}

	routing := cfg["routing"].(map[string]interface{})
	rules := routing["rules"].([]interface{})
	if len(rules) != 4 {
		t.Fatalf("rules = %d, want api+dns+user+provider", len(rules))
	}
	if rules[0].(map[string]interface{})["outboundTag"] != "api" {
		t.Errorf("api rule not first: %+v", rules[0])
	}
	if rules[1].(map[string]interface{})["outboundTag"] != "dns-out" {
		t.Errorf("dns capture rule not second: %+v", rules[1])
	}
	user := rules[2].(map[string]interface{})
	if user["domain"].([]interface{})[0] != "override.example" {
		t.Errorf("user rule not first: %+v", user)
	}
	if user["balancerTag"] != "whoisthat-user-proxy" {
		t.Errorf("user proxy rule must target the user balancer: %+v", user)
	}
	provider := rules[3].(map[string]interface{})
	if provider["balancerTag"] != "Provider_Balancer" {
		t.Errorf("provider rule changed: %+v", provider)
	}

	balancers := routing["balancers"].([]interface{})
	if len(balancers) != 2 {
		t.Fatalf("balancers = %d, want provider + user", len(balancers))
	}

	outbounds := cfg["outbounds"].([]interface{})
	for _, raw := range outbounds {
		ob := raw.(map[string]interface{})
		if ob["protocol"] == "freedom" {
			stream := ob["streamSettings"].(map[string]interface{})
			sockopt := stream["sockopt"].(map[string]interface{})
			if sockopt["mark"] != float64(1) {
				t.Errorf("freedom outbound missing mark 1: %+v", ob)
			}
		}
		if ob["tag"] == "dns-out" {
			stream := ob["streamSettings"].(map[string]interface{})
			sockopt := stream["sockopt"].(map[string]interface{})
			if sockopt["dialerProxy"] != "proxy-a" {
				t.Errorf("dangling dialerProxy not re-pointed at the proxy: %+v", ob)
			}
		}
	}

	dns := cfg["dns"].(map[string]interface{})
	hosts := dns["hosts"].(map[string]interface{})
	if hosts["a.example.com"] != "203.0.113.7" || hosts["static.example"] != "203.0.113.5" {
		t.Errorf("bootstrap hosts not merged: %+v", hosts)
	}
}
