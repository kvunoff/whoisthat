package mainproxy

import (
	"encoding/json"
	"testing"
)

// TestInjectStatsConfig_Policy verifies the stats switches are merged into an
// existing policy section instead of replacing provider levels.
func TestInjectStatsConfig_Policy(t *testing.T) {
	base := `{"policy":{"system":{"statsOutboundUplink":false,"someLevel":1}},"inbounds":[]}`
	out, err := injectStatsConfig([]byte(base), 0)
	if err != nil {
		t.Fatalf("injectStatsConfig: %v", err)
	}
	var cfg map[string]interface{}
	if err := json.Unmarshal(out, &cfg); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	policy := cfg["policy"].(map[string]interface{})
	system := policy["system"].(map[string]interface{})
	for _, key := range []string{"statsInboundUplink", "statsInboundDownlink", "statsOutboundUplink", "statsOutboundDownlink"} {
		if system[key] != true {
			t.Errorf("stats switch %s = %v, want true", key, system[key])
		}
	}
	if system["someLevel"] != float64(1) {
		t.Errorf("provider policy lost: %+v", system)
	}
	if _, has := cfg["stats"]; !has {
		t.Error("stats section missing")
	}
}

// TestInjectStatsConfig_ApiInbound verifies that apiPort>0 adds the api
// service + api dokodemo-door inbound, and that apiPort=0 leaves them out.
func TestInjectStatsConfig_ApiInbound(t *testing.T) {
	base := `{"inbounds":[{"tag":"socks","port":3090,"protocol":"socks"}]}`
	t.Run("apiPort=0", func(t *testing.T) {
		out, err := injectStatsConfig([]byte(base), 0)
		if err != nil {
			t.Fatalf("injectStatsConfig: %v", err)
		}
		var cfg map[string]interface{}
		if err := json.Unmarshal(out, &cfg); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		if _, has := cfg["api"]; has {
			t.Error("api object present with apiPort=0, want absent")
		}
		ibs, _ := cfg["inbounds"].([]interface{})
		if len(ibs) != 1 {
			t.Errorf("inbounds = %d, want 1 (no api inbound)", len(ibs))
		}
	})
	t.Run("apiPort=12345", func(t *testing.T) {
		out, err := injectStatsConfig([]byte(base), 12345)
		if err != nil {
			t.Fatalf("injectStatsConfig: %v", err)
		}
		var cfg map[string]interface{}
		if err := json.Unmarshal(out, &cfg); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		api, ok := cfg["api"].(map[string]interface{})
		if !ok {
			t.Fatal("api object missing")
		}
		if api["tag"] != "api" {
			t.Errorf("api.tag = %v, want \"api\"", api["tag"])
		}
		svcs, _ := api["services"].([]interface{})
		if len(svcs) != 1 || svcs[0] != "StatsService" {
			t.Errorf("api.services = %v, want [StatsService]", svcs)
		}
		ibs, _ := cfg["inbounds"].([]interface{})
		if len(ibs) != 2 {
			t.Fatalf("inbounds = %d, want 2 (socks + api)", len(ibs))
		}
		apiInbound, ok := ibs[1].(map[string]interface{})
		if !ok {
			t.Fatal("second inbound not a map")
		}
		if apiInbound["tag"] != "api" {
			t.Errorf("api inbound tag = %v, want \"api\"", apiInbound["tag"])
		}
		if apiInbound["port"] != 12345.0 {
			t.Errorf("api inbound port = %v, want 12345", apiInbound["port"])
		}
		if apiInbound["protocol"] != "dokodemo-door" {
			t.Errorf("api inbound protocol = %v, want dokodemo-door", apiInbound["protocol"])
		}
	})
}

// TestInjectStatsConfig_NoInbounds verifies the inbound-append path works
// even when the source config has no inbounds at all.
func TestInjectStatsConfig_NoInbounds(t *testing.T) {
	base := `{"outbounds":[{"tag":"proxy"}]}`
	out, err := injectStatsConfig([]byte(base), 9999)
	if err != nil {
		t.Fatalf("injectStatsConfig: %v", err)
	}
	var cfg map[string]interface{}
	if err := json.Unmarshal(out, &cfg); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	ibs, _ := cfg["inbounds"].([]interface{})
	if len(ibs) != 1 {
		t.Fatalf("inbounds = %d, want 1 (api only)", len(ibs))
	}
}
