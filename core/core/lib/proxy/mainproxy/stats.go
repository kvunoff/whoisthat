package mainproxy

import (
	"encoding/json"
	"fmt"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
	"log"
	"os/exec"
	"time"
)

func injectStatsConfig(configJSON []byte, apiPort int) ([]byte, error) {
	var config map[string]interface{}
	if err := json.Unmarshal(configJSON, &config); err != nil {
		return configJSON, err
	}

	// Dedicated API inbound (dokodemo-door on separate port)
	// avoids SOCKS/gRPC protocol conflict on the same port
	apiInbound := map[string]interface{}{
		"tag":      "api",
		"port":     float64(apiPort),
		"listen":   "127.0.0.1",
		"protocol": "dokodemo-door",
		"settings": map[string]interface{}{
			"address": "127.0.0.1",
		},
	}

	if inbounds, ok := config["inbounds"].([]interface{}); ok {
		config["inbounds"] = append([]interface{}{apiInbound}, inbounds...)
	} else {
		config["inbounds"] = []interface{}{apiInbound}
	}

	config["stats"] = map[string]interface{}{}

	config["policy"] = map[string]interface{}{
		"system": map[string]interface{}{
			"statsOutboundUplink":   true,
			"statsOutboundDownlink": true,
		},
	}

	// Attach StatsService to the dedicated API inbound, not SOCKS
	config["api"] = map[string]interface{}{
		"tag":      "api",
		"services": []string{"StatsService"},
	}

	result, err := json.Marshal(config)
	if err != nil {
		return configJSON, err
	}
	inboundCount := len(config["inbounds"].([]interface{}))
	log.Printf("Stats enabled: api port=%d, inbounds=%d, config=%d bytes", apiPort, inboundCount, len(result))
	return result, nil
}

type statEntry struct {
	Name  string `json:"name"`
	Value string `json:"value"`
}

type statsResponse struct {
	Stat []statEntry `json:"stat"`
}

func (p *ProxyManager) collectStats(apiPort int, cancel chan struct{}, out chan<- structs.TrafficStats) {
	log.Println("Stats collector started on port", apiPort)
	xraybin, err := utils.GetXrayBin()
	if err != nil {
		log.Println("Stats collector: xray binary not found:", err)
		return
	}

	ticker := time.NewTicker(1 * time.Second)
	defer ticker.Stop()

	var prevProxyUp, prevProxyDown, prevDirectUp, prevDirectDown int64
	var firstRun = true

	for {
		select {
		case <-cancel:
			log.Println("Stats collector stopped")
			return
		case <-ticker.C:
			stats, err := queryStats(xraybin, apiPort)
			if err != nil {
				log.Println("Stats query failed:", err)
				continue
			}

			var proxyUp, proxyDown, directUp, directDown int64
			var matched int
			for _, s := range stats.Stat {
				v := parseValue(s.Value)
				switch s.Name {
				case "outbound>>>proxy>>>traffic>>>uplink":
					proxyUp = v
					matched++
				case "outbound>>>proxy>>>traffic>>>downlink":
					proxyDown = v
					matched++
				case "outbound>>>direct>>>traffic>>>uplink":
					directUp = v
					matched++
				case "outbound>>>direct>>>traffic>>>downlink":
					directDown = v
					matched++
				}
			}

			if firstRun {
				firstRun = false
				log.Printf("Stats first run: %d stats, %d matched, proxy(up=%d down=%d) direct(up=%d down=%d)\n",
					len(stats.Stat), matched, proxyUp, proxyDown, directUp, directDown)
				prevProxyUp, prevProxyDown = proxyUp, proxyDown
				prevDirectUp, prevDirectDown = directUp, directDown
				continue
			}

			delta := structs.TrafficStats{
				ProxyUp:    proxyUp - prevProxyUp,
				ProxyDown:  proxyDown - prevProxyDown,
				DirectUp:   directUp - prevDirectUp,
				DirectDown: directDown - prevDirectDown,
			}

			prevProxyUp, prevProxyDown = proxyUp, proxyDown
			prevDirectUp, prevDirectDown = directUp, directDown

			if delta.ProxyUp > 0 || delta.ProxyDown > 0 || delta.DirectUp > 0 || delta.DirectDown > 0 {
				log.Printf("Stats delta: proxy(↑%d ↓%d) direct(↑%d ↓%d)",
					delta.ProxyUp, delta.ProxyDown, delta.DirectUp, delta.DirectDown)
			}

			select {
			case out <- delta:
			default:
			}
		}
	}
}

func queryStats(xraybin string, apiPort int) (*statsResponse, error) {
	server := fmt.Sprintf("127.0.0.1:%d", apiPort)
	cmd := exec.Command(xraybin, "api", "statsquery", "--server="+server)
	output, err := cmd.Output()
	if err != nil {
		if len(output) > 0 {
			return nil, fmt.Errorf("xray api statsquery (port %d): %w (output: %s)", apiPort, err, string(output))
		}
		return nil, fmt.Errorf("xray api statsquery (port %d): %w", apiPort, err)
	}

	var resp statsResponse
	if err := json.Unmarshal(output, &resp); err != nil {
		preview := output
		if len(preview) > 500 {
			preview = preview[:500]
		}
		return nil, fmt.Errorf("parse stats (port %d): %w (raw: %s)", apiPort, err, string(preview))
	}

	return &resp, nil
}

func parseValue(s string) int64 {
	var v int64
	fmt.Sscanf(s, "%d", &v)
	return v
}
