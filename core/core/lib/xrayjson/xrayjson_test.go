package xrayjson

import (
	"encoding/json"
	"fmt"
	"strings"
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

func TestPrepareRewritesManualGeoRulesForProviderAssets(t *testing.T) {
	profiles, _ := Parse([]byte(fixture))
	prepared, err := Prepare(profiles[0].Raw, Options{
		SocksPort:    3090,
		HttpPort:     3091,
		DNSServers:   []string{"1.1.1.1"},
		GlobalGeoExt: true,
		UserRules: []structs.RoutingRule{
			{
				Type:        "field",
				Domain:      "geosite:category-ru,geosite:ext:custom.dat:foo,example.com",
				OutboundTag: "direct",
				Enabled:     true,
			},
			{
				Type:        "field",
				IP:          "geoip:ru,geoip:!private,203.0.113.0/24",
				OutboundTag: "direct",
				Enabled:     true,
			},
		},
	})
	if err != nil {
		t.Fatalf("Prepare: %v", err)
	}

	var cfg map[string]interface{}
	if err := json.Unmarshal(prepared.Config, &cfg); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	rules := cfg["routing"].(map[string]interface{})["rules"].([]interface{})

	domains := rules[0].(map[string]interface{})["domain"].([]interface{})
	wantDomains := []string{
		"ext-domain:geosite-global.dat:category-ru",
		"geosite:ext:custom.dat:foo",
		"example.com",
	}
	for i, want := range wantDomains {
		if domains[i] != want {
			t.Errorf("domain[%d] = %v, want %s", i, domains[i], want)
		}
	}

	ips := rules[1].(map[string]interface{})["ip"].([]interface{})
	wantIPs := []string{
		"ext-ip:geoip-global.dat:ru",
		"ext-ip:geoip-global.dat:!private",
		"203.0.113.0/24",
	}
	for i, want := range wantIPs {
		if ips[i] != want {
			t.Errorf("ip[%d] = %v, want %s", i, ips[i], want)
		}
	}

	// Provider rules keep using the provider's own geosite.dat.
	provider := rules[2].(map[string]interface{})
	if provider["balancerTag"] != "Provider_Balancer" {
		t.Errorf("provider rule changed: %+v", provider)
	}
	providerDomains := provider["domain"].([]interface{})
	if providerDomains[0] != "geosite:youtube" {
		t.Errorf("provider geo rule was rewritten: %+v", providerDomains)
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

const jsonTestProxyOutbounds = `[
  {"tag":"proxy-a","protocol":"vless","settings":{"vnext":[{"address":"a.example","port":443}]}},
  {"tag":"proxy-b","protocol":"trojan","settings":{"servers":[{"address":"b.example","port":443}]}},
  {"tag":"direct","protocol":"freedom"},
  {"tag":"block","protocol":"blackhole"},
  {"tag":"dns-out","protocol":"dns"}
]`

const jsonTestSingleProxyOutbounds = `[
  {"tag":"proxy-a","protocol":"vless","settings":{"vnext":[{"address":"a.example","port":443}]}},
  {"tag":"direct","protocol":"freedom"},
  {"tag":"block","protocol":"blackhole"},
  {"tag":"dns-out","protocol":"dns"}
]`

const jsonTestLiteralProxyOutbounds = `[
  {"tag":"proxy","protocol":"vless","settings":{"vnext":[{"address":"p.example","port":443}]}},
  {"tag":"proxy-b","protocol":"trojan","settings":{"servers":[{"address":"b.example","port":443}]}},
  {"tag":"direct","protocol":"freedom"},
  {"tag":"block","protocol":"blackhole"},
  {"tag":"dns-out","protocol":"dns"}
]`

var jsonTestManualProxyRule = structs.RoutingRule{
	Type:        "field",
	Domain:      "2ip.io",
	OutboundTag: "proxy",
	Enabled:     true,
}

func jsonTestProvider(outbounds, rules, balancers string) string {
	return fmt.Sprintf(
		`{"outbounds":%s,"routing":{"rules":%s,"balancers":%s}}`,
		outbounds, rules, balancers,
	)
}

// jsonTestPrepareManual returns the first rule (the manual one when no API/TUN
// structural rules are requested) and the routing section.
func jsonTestPrepareManual(t *testing.T, provider string, rule structs.RoutingRule) (map[string]interface{}, map[string]interface{}) {
	t.Helper()
	prepared, err := Prepare([]byte(provider), Options{
		SocksPort: 3090,
		HttpPort:  3091,
		UserRules: []structs.RoutingRule{rule},
	})
	if err != nil {
		t.Fatalf("Prepare: %v", err)
	}
	var cfg map[string]interface{}
	if err := json.Unmarshal(prepared.Config, &cfg); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	routing, ok := cfg["routing"].(map[string]interface{})
	if !ok {
		t.Fatalf("routing missing: %#v", cfg)
	}
	rules, ok := routing["rules"].([]interface{})
	if !ok || len(rules) == 0 {
		t.Fatalf("rules missing: %#v", routing["rules"])
	}
	manual, ok := rules[0].(map[string]interface{})
	if !ok {
		t.Fatalf("manual rule is not a map: %#v", rules[0])
	}
	return manual, routing
}

func jsonTestFindBalancer(routing map[string]interface{}, tag string) map[string]interface{} {
	balancers, _ := routing["balancers"].([]interface{})
	for _, raw := range balancers {
		m, _ := raw.(map[string]interface{})
		if m["tag"] == tag {
			return m
		}
	}
	return nil
}

func TestJSONManualProxyUsesFirstCatchAll(t *testing.T) {
	provider := jsonTestProvider(
		jsonTestProxyOutbounds,
		`[
		  {"type":"field","network":"tcp,udp","outboundTag":"direct"},
		  {"type":"field","network":"tcp,udp","balancerTag":"Provider_Main"}
		]`,
		`[{"tag":"Provider_Main","selector":["proxy-"],"strategy":{"type":"leastPing"}}]`,
	)
	manual, routing := jsonTestPrepareManual(t, provider, jsonTestManualProxyRule)
	if manual["balancerTag"] != "whoisthat-user-proxy" {
		t.Fatalf("manual proxy target = %#v, want fallback because the first catch-all is direct", manual)
	}
	if balancers, _ := routing["balancers"].([]interface{}); len(balancers) != 2 {
		t.Fatalf("balancers = %d, want provider + fallback", len(balancers))
	}
}

func TestJSONManualProxyUsesCatchAllBalancer(t *testing.T) {
	provider := jsonTestProvider(
		jsonTestProxyOutbounds,
		`[{"type":"field","network":"tcp,udp","balancerTag":"Provider_Main"}]`,
		`[{"tag":"Provider_Main","selector":["proxy-"],"strategy":{"type":"leastPing"}}]`,
	)
	manual, routing := jsonTestPrepareManual(t, provider, jsonTestManualProxyRule)
	if manual["balancerTag"] != "Provider_Main" {
		t.Fatalf("manual proxy target = %#v, want Provider_Main", manual)
	}
	if balancers, _ := routing["balancers"].([]interface{}); len(balancers) != 1 {
		t.Fatalf("balancers = %d, want only the provider balancer", len(balancers))
	}
}

func TestJSONManualProxyUsesCatchAllOutbound(t *testing.T) {
	outbounds := `[
	  {"tag":"proxy-main","protocol":"vless","settings":{"vnext":[{"address":"m.example","port":443}]}},
	  {"tag":"proxy-b","protocol":"trojan","settings":{"servers":[{"address":"b.example","port":443}]}},
	  {"tag":"direct","protocol":"freedom"},
	  {"tag":"block","protocol":"blackhole"},
	  {"tag":"dns-out","protocol":"dns"}
	]`
	provider := jsonTestProvider(
		outbounds,
		`[{"type":"field","network":"tcp,udp","outboundTag":"proxy-main"}]`,
		`[]`,
	)
	manual, _ := jsonTestPrepareManual(t, provider, jsonTestManualProxyRule)
	if manual["outboundTag"] != "proxy-main" {
		t.Fatalf("manual proxy target = %#v, want proxy-main", manual)
	}
}

func TestJSONManualProxyCatchAllDirectFallsBack(t *testing.T) {
	rules := `[{"type":"field","network":"tcp,udp","outboundTag":"direct"}]`

	t.Run("multiple proxies", func(t *testing.T) {
		provider := jsonTestProvider(jsonTestProxyOutbounds, rules, `[]`)
		manual, _ := jsonTestPrepareManual(t, provider, jsonTestManualProxyRule)
		if manual["balancerTag"] != "whoisthat-user-proxy" {
			t.Fatalf("manual proxy target = %#v, want fallback", manual)
		}
	})

	t.Run("single proxy", func(t *testing.T) {
		provider := jsonTestProvider(jsonTestSingleProxyOutbounds, rules, `[]`)
		manual, _ := jsonTestPrepareManual(t, provider, jsonTestManualProxyRule)
		if manual["outboundTag"] != "proxy-a" {
			t.Fatalf("manual proxy target = %#v, want proxy-a", manual)
		}
	})
}

func TestJSONManualProxyWithoutCatchAllFallsBack(t *testing.T) {
	provider := jsonTestProvider(
		jsonTestProxyOutbounds,
		`[{"type":"field","domain":["example.com"],"outboundTag":"direct"}]`,
		`[]`,
	)
	manual, _ := jsonTestPrepareManual(t, provider, jsonTestManualProxyRule)
	if manual["balancerTag"] != "whoisthat-user-proxy" {
		t.Fatalf("manual proxy target = %#v, want fallback", manual)
	}
}

func TestJSONSingleProxyOutboundWinsOverCatchAll(t *testing.T) {
	provider := jsonTestProvider(
		jsonTestSingleProxyOutbounds,
		`[{"type":"field","network":"tcp,udp","balancerTag":"Provider_Main"}]`,
		`[{"tag":"Provider_Main","selector":["proxy-a"],"strategy":{"type":"leastPing"}}]`,
	)
	manual, routing := jsonTestPrepareManual(t, provider, jsonTestManualProxyRule)
	if manual["outboundTag"] != "proxy-a" {
		t.Fatalf("manual proxy target = %#v, want the single proxy outbound", manual)
	}
	if balancers, _ := routing["balancers"].([]interface{}); len(balancers) != 1 {
		t.Fatalf("balancers = %d, want only the provider balancer", len(balancers))
	}
}

func TestJSONLiteralProxyOutboundWins(t *testing.T) {
	provider := jsonTestProvider(
		jsonTestLiteralProxyOutbounds,
		`[{"type":"field","network":"tcp,udp","balancerTag":"Provider_Main"}]`,
		`[{"tag":"Provider_Main","selector":["proxy-b"],"strategy":{"type":"leastPing"}}]`,
	)
	manual, _ := jsonTestPrepareManual(t, provider, jsonTestManualProxyRule)
	if manual["outboundTag"] != "proxy" {
		t.Fatalf("manual proxy target = %#v, want literal proxy outbound", manual)
	}
}

func TestJSONCatchAllNetworkForms(t *testing.T) {
	cases := []struct {
		name    string
		network string
		want    bool
	}{
		{"string tcp,udp", `"tcp,udp"`, true},
		{"array tcp,udp", `["tcp","udp"]`, true},
		{"string tcp", `"tcp"`, false},
		{"array udp", `["udp"]`, false},
		{"empty array", `[]`, false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			provider := jsonTestProvider(
				jsonTestProxyOutbounds,
				fmt.Sprintf(`[{"type":"field","network":%s,"balancerTag":"Provider_Main"}]`, tc.network),
				`[{"tag":"Provider_Main","selector":["proxy-"],"strategy":{"type":"leastPing"}}]`,
			)
			manual, _ := jsonTestPrepareManual(t, provider, jsonTestManualProxyRule)
			got := manual["balancerTag"] == "Provider_Main"
			if got != tc.want {
				t.Fatalf("balancer target = %#v, want catch-all=%v", manual, tc.want)
			}
		})
	}
}

func TestJSONCatchAllRejectsSelectorFields(t *testing.T) {
	cases := []struct {
		name     string
		selector string
	}{
		{"sourceIP", `"sourceIP":["10.0.0.0/8"]`},
		{"sourcePort", `"sourcePort":"1000-2000"`},
		{"localIP", `"localIP":["127.0.0.1"]`},
		{"localPort", `"localPort":"1234"`},
		{"process", `"process":["curl"]`},
		{"vlessRoute", `"vlessRoute":"1-10"`},
		{"domains", `"domains":["example.com"]`},
		{"webhook", `"webhook":{"url":"https://example.test"}`},
		{"unknown", `"unknownField":"x"`},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			provider := jsonTestProvider(
				jsonTestProxyOutbounds,
				fmt.Sprintf(`[{"type":"field",%s,"balancerTag":"Provider_Main"}]`, tc.selector),
				`[{"tag":"Provider_Main","selector":["proxy-"],"strategy":{"type":"leastPing"}}]`,
			)
			manual, _ := jsonTestPrepareManual(t, provider, jsonTestManualProxyRule)
			if manual["balancerTag"] != "whoisthat-user-proxy" {
				t.Fatalf("selector %s must not be a catch-all: %#v", tc.name, manual)
			}
		})
	}
}

func TestJSONCatchAllBalancerSelectorMatching(t *testing.T) {
	cases := []struct {
		name     string
		selector string
		want     bool
	}{
		{"prefix", `"proxy-"`, true},
		{"service prefix", `"d"`, false},
		{"empty", `""`, false},
		{"no match", `"nope"`, false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			provider := jsonTestProvider(
				jsonTestProxyOutbounds,
				`[{"type":"field","network":"tcp,udp","balancerTag":"Provider_Main"}]`,
				fmt.Sprintf(`[{"tag":"Provider_Main","selector":[%s],"strategy":{"type":"leastPing"}}]`, tc.selector),
			)
			manual, _ := jsonTestPrepareManual(t, provider, jsonTestManualProxyRule)
			got := manual["balancerTag"] == "Provider_Main"
			if got != tc.want {
				t.Fatalf("balancer target = %#v, want provider balancer=%v", manual, tc.want)
			}
		})
	}
}

func TestJSONCatchAllOutboundWinsOverBalancer(t *testing.T) {
	outbounds := `[
	  {"tag":"proxy-main","protocol":"vless","settings":{"vnext":[{"address":"m.example","port":443}]}},
	  {"tag":"proxy-b","protocol":"trojan","settings":{"servers":[{"address":"b.example","port":443}]}},
	  {"tag":"direct","protocol":"freedom"},
	  {"tag":"block","protocol":"blackhole"},
	  {"tag":"dns-out","protocol":"dns"}
	]`
	provider := jsonTestProvider(
		outbounds,
		`[{"type":"field","network":"tcp,udp","outboundTag":"proxy-main","balancerTag":"Provider_Main"}]`,
		`[{"tag":"Provider_Main","selector":["proxy-b"],"strategy":{"type":"leastPing"}}]`,
	)
	manual, _ := jsonTestPrepareManual(t, provider, jsonTestManualProxyRule)
	if manual["outboundTag"] != "proxy-main" {
		t.Fatalf("manual proxy target = %#v, want outboundTag precedence", manual)
	}
	if _, ok := manual["balancerTag"]; ok {
		t.Fatalf("manual rule must not carry balancerTag: %#v", manual)
	}
}

func TestJSONFallbackSkipsDialerProxyOutbounds(t *testing.T) {
	outbounds := `[
	  {"tag":"proxy-plain","protocol":"vless","settings":{"vnext":[{"address":"p.example","port":443}]}},
	  {"tag":"proxy-chain","protocol":"vless","settings":{"vnext":[{"address":"c.example","port":443}]},
	   "streamSettings":{"sockopt":{"dialerProxy":"proxy-plain"}}},
	  {"tag":"direct","protocol":"freedom"},
	  {"tag":"block","protocol":"blackhole"},
	  {"tag":"dns-out","protocol":"dns"}
	]`
	provider := jsonTestProvider(outbounds, `[]`, `[]`)
	_, routing := jsonTestPrepareManual(t, provider, jsonTestManualProxyRule)
	balancer := jsonTestFindBalancer(routing, "whoisthat-user-proxy")
	if balancer == nil {
		t.Fatal("fallback balancer missing")
	}
	selector, _ := balancer["selector"].([]interface{})
	if len(selector) != 1 || selector[0] != "proxy-plain" {
		t.Fatalf("fallback selector = %#v, want [proxy-plain]", selector)
	}
}

func TestJSONFallbackKeepsAllTagsWhenEveryOutboundIsChained(t *testing.T) {
	outbounds := `[
	  {"tag":"proxy-chain","protocol":"vless","settings":{"vnext":[{"address":"c.example","port":443}]},
	   "streamSettings":{"sockopt":{"dialerProxy":"proxy-plain"}}},
	  {"tag":"proxy-plain","protocol":"vless","settings":{"vnext":[{"address":"p.example","port":443}]},
	   "streamSettings":{"sockopt":{"dialerProxy":"proxy-chain"}}},
	  {"tag":"direct","protocol":"freedom"},
	  {"tag":"block","protocol":"blackhole"},
	  {"tag":"dns-out","protocol":"dns"}
	]`
	provider := jsonTestProvider(outbounds, `[]`, `[]`)
	_, routing := jsonTestPrepareManual(t, provider, jsonTestManualProxyRule)
	balancer := jsonTestFindBalancer(routing, "whoisthat-user-proxy")
	if balancer == nil {
		t.Fatal("fallback balancer missing")
	}
	selector, _ := balancer["selector"].([]interface{})
	if len(selector) != 2 {
		t.Fatalf("fallback selector = %#v, want all proxy tags", selector)
	}
}

func TestStreamDetailsAreSanitized(t *testing.T) {
	raw := json.RawMessage(`{"outbounds":[{"tag":"proxy","protocol":"vless","settings":{"vnext":[{"address":"x.example","port":443,"users":[{"id":"secret-uuid","flow":"xtls-rprx-vision"}]}]},"streamSettings":{"network":"xhttp","security":"reality","realitySettings":{"serverName":"sni.example","fingerprint":"chrome","publicKey":"pub-key","shortId":"1a2b","spiderX":"/"},"xhttpSettings":{"path":"/x","host":"cdn.example","mode":"auto"},"tlsSettings":{"alpn":["h2","http/1.1"],"allowInsecure":true}}}]}`)

	details := StreamDetails(raw)
	if details.Transport != "xhttp" || details.Security != "reality" {
		t.Fatalf("transport/security = %q/%q", details.Transport, details.Security)
	}
	if details.SNI != "sni.example" || details.Fingerprint != "chrome" {
		t.Fatalf("sni/fingerprint = %q/%q", details.SNI, details.Fingerprint)
	}
	if details.PublicKey != "pub-key" || details.ShortId != "1a2b" || details.SpiderX != "/" {
		t.Fatalf("reality fields = %+v", details)
	}
	if details.Path != "/x" || details.Host != "cdn.example" || details.Mode != "auto" {
		t.Fatalf("xhttp fields = %+v", details)
	}
	if details.ALPN != "h2,http/1.1" || !details.AllowInsecure {
		t.Fatalf("tls fields = %+v", details)
	}
	if details.Flow != "xtls-rprx-vision" {
		t.Fatalf("flow = %q", details.Flow)
	}
	if strings.Contains(fmt.Sprintf("%+v", details), "secret-uuid") {
		t.Fatal("credential leaked into profile details")
	}
}
