package xrayjson

import (
	"encoding/json"
	"fmt"
	"strings"

	"whoisthat-core/structs"
)

// ApplyAntiDPI injects the user's global fragment/noise masks into a config
// generated from a URI profile. With both switches off it returns raw
// untouched so the probe/preflight path keeps byte-for-byte compatibility.
func ApplyAntiDPI(raw []byte, cfg structs.AntiDPIConfig) ([]byte, error) {
	if !cfg.Fragment.Enabled && !cfg.Noise.Enabled {
		return raw, nil
	}

	var config map[string]interface{}
	if err := json.Unmarshal(raw, &config); err != nil {
		return nil, fmt.Errorf("parse xray config: %w", err)
	}
	applyAntiDPIToConfig(config, cfg)

	result, err := json.Marshal(config)
	if err != nil {
		return nil, fmt.Errorf("marshal xray config: %w", err)
	}
	return result, nil
}

// applyAntiDPIToConfig adds masks to every proxy outbound that does not
// already carry a provider mask for that direction. Mirrors and other
// provider-owned fields are left intact.
func applyAntiDPIToConfig(config map[string]interface{}, cfg structs.AntiDPIConfig) {
	if !cfg.Fragment.Enabled && !cfg.Noise.Enabled {
		return
	}

	outbounds := asSlice(config["outbounds"])
	for _, raw := range outbounds {
		ob, ok := raw.(map[string]interface{})
		if !ok {
			continue
		}
		protocol, _ := ob["protocol"].(string)
		if serviceProtocols[strings.ToLower(strings.TrimSpace(protocol))] {
			continue
		}

		stream := asMap(ob["streamSettings"])
		fm := asMap(stream["finalmask"])
		changed := false

		if cfg.Fragment.Enabled && len(asSlice(fm["tcp"])) == 0 {
			fm["tcp"] = []interface{}{fragmentMask(cfg.Fragment)}
			changed = true
		}
		if cfg.Noise.Enabled && len(asSlice(fm["udp"])) == 0 {
			fm["udp"] = []interface{}{noiseMask(cfg.Noise)}
			changed = true
		}
		if !changed {
			continue
		}

		stream["finalmask"] = fm
		ob["streamSettings"] = stream
	}
}

func fragmentMask(cfg structs.AntiDPIFragment) map[string]interface{} {
	return map[string]interface{}{
		"type": "fragment",
		"settings": map[string]interface{}{
			"packets": cfg.Packets,
			"length":  cfg.Length,
			"delay":   cfg.Interval,
		},
	}
}

func noiseMask(cfg structs.AntiDPINoise) map[string]interface{} {
	item := map[string]interface{}{"delay": cfg.Delay}
	if cfg.Type == "rand" {
		item["rand"] = cfg.Packet
	} else {
		item["type"] = cfg.Type
		item["packet"] = cfg.Packet
	}
	return map[string]interface{}{
		"type": "noise",
		"settings": map[string]interface{}{
			"noise": []interface{}{item},
		},
	}
}
