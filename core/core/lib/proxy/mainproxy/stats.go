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

	config["stats"] = map[string]interface{}{}

	config["policy"] = map[string]interface{}{
		"system": map[string]interface{}{
			"statsOutboundUplink":   true,
			"statsOutboundDownlink": true,
		},
	}

	config["api"] = map[string]interface{}{
		"tag":      "socks-in",
		"services": []string{"StatsService"},
	}

	// Tag the first inbound as API to avoid extra port
	if inbounds, ok := config["inbounds"].([]interface{}); ok && len(inbounds) > 0 {
		if inbound, ok := inbounds[0].(map[string]interface{}); ok {
			inbound["tag"] = "socks-in"
			inbounds[0] = inbound
		}
		config["inbounds"] = inbounds
	}

	result, err := json.Marshal(config)
	if err != nil {
		return configJSON, err
	}
	log.Printf("Stats enabled, api port: %d, config size: %d", apiPort, len(result))
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
		return nil, fmt.Errorf("xray api statsquery: %w (output: %s)", err, string(output))
	}

	var resp statsResponse
	if err := json.Unmarshal(output, &resp); err != nil {
		return nil, fmt.Errorf("parse stats: %w (raw: %s)", err, string(output[:min(len(output), 200)]))
	}

	return &resp, nil
}

func parseValue(s string) int64 {
	var v int64
	fmt.Sscanf(s, "%d", &v)
	return v
}
