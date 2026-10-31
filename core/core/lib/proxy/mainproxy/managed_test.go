package mainproxy

import (
	"encoding/json"
	"testing"

	"whoisthat-core/db"
	"whoisthat-core/structs"
)

func newManagedTestDB(t *testing.T) (*db.DB, structs.Profile) {
	t.Helper()
	database := &db.DB{Path: t.TempDir()}
	database.Initialize()
	if err := database.SaveGroup(structs.Group{Id: 1, Name: "JSON"}); err != nil {
		t.Fatalf("SaveGroup: %v", err)
	}

	raw := json.RawMessage(`{
		"remarks": "Provider",
		"dns": {"servers": ["https://8.8.8.8/dns-query"]},
		"routing": {"domainStrategy": "IPIfNonMatch", "rules": [
			{"type": "field", "domain": ["geosite:youtube"], "outboundTag": "proxy-a"}
		]},
		"inbounds": [
			{"tag": "socks", "protocol": "socks", "port": 10808},
			{"tag": "http", "protocol": "http", "port": 10809}
		],
		"outbounds": [
			{"tag": "proxy-a", "protocol": "vless", "settings": {"vnext": [{"address": "1.2.3.4", "port": 443}]}},
			{"tag": "direct", "protocol": "freedom"},
			{"tag": "block", "protocol": "blackhole"},
			{"tag": "dns-out", "protocol": "dns"}
		]
	}`)
	profiles, err := database.UpdateGroupAndProfiles(1, []structs.DBAddProfileData{{
		GroupId:     1,
		Name:        "Provider",
		Protocol:    "vless",
		Address:     "1.2.3.4",
		Port:        443,
		Config:      raw,
		ProviderKey: "Provider",
		Managed:     true,
	}}, 0)
	if err != nil {
		t.Fatalf("UpdateGroupAndProfiles: %v", err)
	}
	return database, profiles[0]
}

func TestPrepareManagedConfigKeepsProviderRoutingAndAddsStats(t *testing.T) {
	database, profile := newManagedTestDB(t)
	p := &ProxyManager{DB: database}

	cfg, tags, err := p.prepareManagedConfig(profile, 3090, 3091, "tun0", 3095, structs.RoutingConfig{
		Rules: []structs.RoutingRule{
			{Type: "field", Domain: "override.example", OutboundTag: "proxy", Enabled: true},
		},
	}, map[string][]string{"1.2.3.4": {"1.2.3.4"}})
	if err != nil {
		t.Fatalf("prepareManagedConfig: %v", err)
	}
	if len(tags) != 1 || tags[0] != "proxy-a" {
		t.Fatalf("proxy tags = %v, want [proxy-a]", tags)
	}

	var parsed map[string]interface{}
	if err := json.Unmarshal(cfg, &parsed); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}

	if _, ok := parsed["stats"]; !ok {
		t.Error("stats section missing")
	}
	if _, ok := parsed["api"]; !ok {
		t.Error("api section missing")
	}

	inbounds := parsed["inbounds"].([]interface{})
	if len(inbounds) != 4 { // socks, http, tun, api
		t.Fatalf("inbounds = %d, want 4", len(inbounds))
	}

	rules := parsed["routing"].(map[string]interface{})["rules"].([]interface{})
	if len(rules) != 4 { // api, dns capture, manual, provider
		t.Fatalf("rules = %d, want 4", len(rules))
	}
	if rules[0].(map[string]interface{})["outboundTag"] != "api" {
		t.Errorf("api rule missing: %+v", rules[0])
	}
	if rules[2].(map[string]interface{})["domain"].([]interface{})[0] != "override.example" {
		t.Errorf("manual rule not before provider rule: %+v", rules[2])
	}
	if rules[3].(map[string]interface{})["outboundTag"] != "proxy-a" {
		t.Errorf("provider rule changed: %+v", rules[3])
	}

	policy := parsed["policy"].(map[string]interface{})
	system := policy["system"].(map[string]interface{})
	if system["statsOutboundUplink"] != true {
		t.Errorf("stats switches missing from policy: %+v", policy)
	}
}
