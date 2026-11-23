package mainproxy

import "encoding/json"

// injectStatsConfig enables Xray's per-inbound/per-outbound traffic counters
// and, when apiPort > 0, exposes the StatsService over a loopback dokodemo-door
// inbound. Sampling and classification live in lib/traffic.
func injectStatsConfig(configJSON []byte, apiPort int) ([]byte, error) {
	var config map[string]interface{}
	if err := json.Unmarshal(configJSON, &config); err != nil {
		return configJSON, err
	}

	config["stats"] = map[string]interface{}{}

	// Provider configs may carry their own policy (levels timeouts); only the
	// stats switches are ours, so merge instead of replacing the section.
	policy, _ := config["policy"].(map[string]interface{})
	if policy == nil {
		policy = map[string]interface{}{}
	}
	system, _ := policy["system"].(map[string]interface{})
	if system == nil {
		system = map[string]interface{}{}
	}
	system["statsInboundUplink"] = true
	system["statsInboundDownlink"] = true
	system["statsOutboundUplink"] = true
	system["statsOutboundDownlink"] = true
	policy["system"] = system
	config["policy"] = policy

	if apiPort > 0 {
		config["api"] = map[string]interface{}{
			"tag":      "api",
			"services": []string{"StatsService"},
		}

		newInbound := map[string]interface{}{
			"tag":      "api",
			"listen":   "127.0.0.1",
			"port":     apiPort,
			"protocol": "dokodemo-door",
			"settings": map[string]interface{}{
				"address": "127.0.0.1",
			},
		}

		inboundsRaw, ok := config["inbounds"].([]interface{})
		if !ok {
			inboundsRaw = []interface{}{}
		}
		inboundsRaw = append(inboundsRaw, newInbound)
		config["inbounds"] = inboundsRaw
	}

	result, err := json.Marshal(config)
	if err != nil {
		return configJSON, err
	}
	return result, nil
}
