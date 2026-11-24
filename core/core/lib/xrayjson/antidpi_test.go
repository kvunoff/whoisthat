package xrayjson

import (
	"bytes"
	"encoding/json"
	"testing"

	"whoisthat-core/structs"
)

func testAntiDPI() structs.AntiDPIConfig {
	return structs.AntiDPIConfig{
		Fragment: structs.AntiDPIFragment{
			Enabled:  true,
			Packets:  "tlshello",
			Length:   "100-200",
			Interval: "10-20",
		},
		Noise: structs.AntiDPINoise{
			Enabled: true,
			Type:    "rand",
			Packet:  "50-100",
			Delay:   "10-16",
		},
	}
}

func decodeConfig(t *testing.T, raw []byte) map[string]interface{} {
	t.Helper()
	var cfg map[string]interface{}
	if err := json.Unmarshal(raw, &cfg); err != nil {
		t.Fatalf("unmarshal config: %v", err)
	}
	return cfg
}

func finalmaskOf(t *testing.T, cfg map[string]interface{}, tag string) map[string]interface{} {
	t.Helper()
	for _, raw := range asSlice(cfg["outbounds"]) {
		ob := raw.(map[string]interface{})
		if ob["tag"] != tag {
			continue
		}
		return asMap(asMap(ob["streamSettings"])["finalmask"])
	}
	t.Fatalf("outbound %q not found", tag)
	return nil
}

func TestApplyAntiDPIOffIsNoop(t *testing.T) {
	raw := []byte(`{"outbounds":[{"tag":"proxy","protocol":"vless","streamSettings":{}}]}`)
	got, err := ApplyAntiDPI(raw, structs.AntiDPIConfig{})
	if err != nil {
		t.Fatalf("ApplyAntiDPI: %v", err)
	}
	if !bytes.Equal(got, raw) {
		t.Fatalf("off must return the input unchanged:\n%s", got)
	}
}

func TestApplyAntiDPIAddsMasksToProxyOnly(t *testing.T) {
	raw := []byte(`{"outbounds":[
		{"tag":"proxy","protocol":"vless","streamSettings":{}},
		{"tag":"direct","protocol":"freedom"},
		{"tag":"block","protocol":"blackhole"}
	]}`)
	got, err := ApplyAntiDPI(raw, testAntiDPI())
	if err != nil {
		t.Fatalf("ApplyAntiDPI: %v", err)
	}
	cfg := decodeConfig(t, got)

	fm := finalmaskOf(t, cfg, "proxy")
	if len(asSlice(fm["tcp"])) != 1 || len(asSlice(fm["udp"])) != 1 {
		t.Fatalf("unexpected proxy finalmask: %v", fm)
	}
	tcp := asSlice(fm["tcp"])[0].(map[string]interface{})
	if tcp["type"] != "fragment" {
		t.Errorf("unexpected tcp mask: %v", tcp)
	}
	settings := asMap(tcp["settings"])
	if settings["packets"] != "tlshello" || settings["length"] != "100-200" || settings["delay"] != "10-20" {
		t.Errorf("unexpected fragment settings: %v", settings)
	}

	for _, tag := range []string{"direct", "block"} {
		for _, raw := range asSlice(cfg["outbounds"]) {
			ob := raw.(map[string]interface{})
			if ob["tag"] == tag && ob["streamSettings"] != nil {
				t.Errorf("%s must not be touched: %v", tag, ob)
			}
		}
	}
}

func TestApplyAntiDPIProviderMasksWin(t *testing.T) {
	raw := []byte(`{"outbounds":[{"tag":"proxy","protocol":"vless","streamSettings":{
		"finalmask":{"tcp":[{"type":"fragment","settings":{"packets":"1-2"}}]}
	}}]}`)
	cfg := testAntiDPI()
	cfg.Noise.Type = "hex"
	cfg.Noise.Packet = "77686f697374686174"
	got, err := ApplyAntiDPI(raw, cfg)
	if err != nil {
		t.Fatalf("ApplyAntiDPI: %v", err)
	}
	fm := finalmaskOf(t, decodeConfig(t, got), "proxy")
	if len(asSlice(fm["tcp"])) != 1 {
		t.Fatalf("provider tcp mask must be preserved, got %v", fm["tcp"])
	}
	if got := asMap(asSlice(fm["tcp"])[0].(map[string]interface{})["settings"])["packets"]; got != "1-2" {
		t.Errorf("global fragment overwrote provider mask: %v", fm["tcp"])
	}
	udp := asSlice(fm["udp"])
	if len(udp) != 1 {
		t.Fatalf("noise should fill the empty direction, got %v", fm["udp"])
	}
	item := asMap(asSlice(asMap(udp[0].(map[string]interface{})["settings"])["noise"])[0])
	if item["type"] != "hex" || item["packet"] != "77686f697374686174" {
		t.Errorf("unexpected noise item: %v", item)
	}
	if _, exists := item["rand"]; exists {
		t.Errorf("hex noise must not emit rand: %v", item)
	}
}

func TestApplyAntiDPIRandNoiseShape(t *testing.T) {
	raw := []byte(`{"outbounds":[{"tag":"proxy","protocol":"vless","streamSettings":{}}]}`)
	got, err := ApplyAntiDPI(raw, testAntiDPI())
	if err != nil {
		t.Fatalf("ApplyAntiDPI: %v", err)
	}
	fm := finalmaskOf(t, decodeConfig(t, got), "proxy")
	udp := asSlice(fm["udp"])
	if len(udp) != 1 {
		t.Fatalf("unexpected udp mask: %v", fm["udp"])
	}
	item := asMap(asSlice(asMap(udp[0].(map[string]interface{})["settings"])["noise"])[0])
	if item["rand"] != "50-100" {
		t.Errorf("unexpected rand: %v", item)
	}
	if _, exists := item["type"]; exists {
		t.Errorf("rand noise must not emit type: %v", item)
	}
	if _, exists := item["packet"]; exists {
		t.Errorf("rand noise must not emit packet: %v", item)
	}
}

func TestApplyAntiDPIIsIdempotent(t *testing.T) {
	raw := []byte(`{"outbounds":[{"tag":"proxy","protocol":"vless","streamSettings":{}}]}`)
	once, err := ApplyAntiDPI(raw, testAntiDPI())
	if err != nil {
		t.Fatalf("first apply: %v", err)
	}
	twice, err := ApplyAntiDPI(once, testAntiDPI())
	if err != nil {
		t.Fatalf("second apply: %v", err)
	}
	fm := finalmaskOf(t, decodeConfig(t, twice), "proxy")
	if len(asSlice(fm["tcp"])) != 1 || len(asSlice(fm["udp"])) != 1 {
		t.Fatalf("masks were duplicated: %v", fm)
	}
}

func TestPrepareOffDoesNotAddFinalmask(t *testing.T) {
	raw := []byte(`{"outbounds":[{"tag":"proxy","protocol":"vless","streamSettings":{}}]}`)
	prepared, err := Prepare(raw, Options{AntiDPI: structs.AntiDPIConfig{}})
	if err != nil {
		t.Fatalf("Prepare: %v", err)
	}
	fm := finalmaskOf(t, decodeConfig(t, prepared.Config), "proxy")
	if len(fm) != 0 {
		t.Fatalf("off must not add finalmask: %v", fm)
	}
}

func TestPrepareAppliesAntiDPI(t *testing.T) {
	raw := []byte(`{"outbounds":[{"tag":"proxy","protocol":"vless","streamSettings":{}}]}`)
	prepared, err := Prepare(raw, Options{AntiDPI: testAntiDPI()})
	if err != nil {
		t.Fatalf("Prepare: %v", err)
	}
	fm := finalmaskOf(t, decodeConfig(t, prepared.Config), "proxy")
	if len(asSlice(fm["tcp"])) != 1 || len(asSlice(fm["udp"])) != 1 {
		t.Fatalf("Prepare did not inject masks: %v", fm)
	}
}
