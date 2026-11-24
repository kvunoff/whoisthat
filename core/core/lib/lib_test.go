package lib

import (
	"encoding/base64"
	"encoding/json"
	"testing"

	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/structs"
)

func TestGetDBAddProfileDatasFromStr(t *testing.T) {
	uris := "vless://uuid1@example.com:443?security=reality#Node1\ntrojan://pw2@example.org:443#Node2"
	b64 := base64.StdEncoding.EncodeToString([]byte(uris))

	profiles := GetDBAddProfileDatasFromStr(b64, 1)
	if len(profiles) != 2 {
		t.Fatalf("expected 2 profiles, got %d", len(profiles))
	}

	if profiles[0].Name != "Node1" || profiles[0].Protocol != "vless" {
		t.Errorf("profile 0 unexpected: %+v", profiles[0])
	}
	if profiles[1].Name != "Node2" || profiles[1].Protocol != "trojan" {
		t.Errorf("profile 1 unexpected: %+v", profiles[1])
	}
}

func TestParseUriAppliesAntiDPI(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("XDG_DATA_HOME", dir)
	appconfig.LoadConfig()
	if err := appconfig.SetAntiDPI(structs.AntiDPIConfig{
		Fragment: structs.AntiDPIFragment{Enabled: true, Packets: "tlshello", Length: "100-200", Interval: "10-20"},
		Noise:    structs.AntiDPINoise{Enabled: true, Type: "rand", Packet: "50-100", Delay: "10-16"},
	}); err != nil {
		t.Fatalf("SetAntiDPI: %v", err)
	}

	raw, err := ParseUri("vless://uuid@example.com:443?type=tcp&security=tls", 0, 0)
	if err != nil {
		t.Fatalf("ParseUri: %v", err)
	}
	var cfg struct {
		Outbounds []struct {
			StreamSettings struct {
				Finalmask map[string]any `json:"finalmask"`
			} `json:"streamSettings"`
		} `json:"outbounds"`
	}
	if err := json.Unmarshal(raw, &cfg); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if len(cfg.Outbounds) == 0 {
		t.Fatal("no outbounds")
	}
	fm := cfg.Outbounds[0].StreamSettings.Finalmask
	if _, ok := fm["tcp"]; !ok {
		t.Errorf("fragment missing: %v", fm)
	}
	if _, ok := fm["udp"]; !ok {
		t.Errorf("noise missing: %v", fm)
	}
}
