package mainproxy

import (
	"encoding/json"
	"fmt"
	"whoisthat-core/structs"
	"os/exec"
	"regexp"
	"strconv"
	"time"
)

func injectStatsConfig(configJSON []byte) ([]byte, error) {
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

	result, err := json.Marshal(config)
	if err != nil {
		return configJSON, err
	}
	return result, nil
}

type statEntry struct {
	Name  string `json:"name"`
	Value string `json:"value"`
}

type statsResponse struct {
	Stat []statEntry `json:"stat"`
}

func (p *ProxyManager) collectStats(socksPort int, cancel chan struct{}, out chan<- structs.TrafficStats) {
	ticker := time.NewTicker(1 * time.Second)
	defer ticker.Stop()

	var prevProxyUp, prevProxyDown int64
	var firstRun = true

	for {
		select {
		case <-cancel:
			return
		case <-ticker.C:
			stats, err := querySsStats(socksPort)
			if err != nil {
				continue
			}

			var proxyUp, proxyDown int64
			for _, s := range stats.Stat {
				v := parseValue(s.Value)
				switch s.Name {
				case "outbound>>>proxy>>>traffic>>>uplink":
					proxyUp = v
				case "outbound>>>proxy>>>traffic>>>downlink":
					proxyDown = v
				}
			}

			if firstRun {
				firstRun = false
				prevProxyUp, prevProxyDown = proxyUp, proxyDown
				continue
			}

			delta := structs.TrafficStats{
				ProxyUp:   proxyUp - prevProxyUp,
				ProxyDown: proxyDown - prevProxyDown,
			}

			prevProxyUp, prevProxyDown = proxyUp, proxyDown

			select {
			case out <- delta:
			default:
			}
		}
	}
}

var (
	reBytesSent     = regexp.MustCompile(`bytes_sent:(\d+)`)
	reBytesReceived = regexp.MustCompile(`bytes_received:(\d+)`)
)

func querySsStats(port int) (*statsResponse, error) {
	filter := fmt.Sprintf("dport = :%d", port)
	cmd := exec.Command("ss", "-tie", filter)
	output, err := cmd.Output()
	if err != nil {
		if len(output) > 0 {
			return nil, fmt.Errorf("ss (port %d): %w (output: %s)", port, err, string(output))
		}
		return nil, fmt.Errorf("ss (port %d): %w", port, err)
	}

	var proxyUp, proxyDown int64
	for _, m := range reBytesSent.FindAllStringSubmatch(string(output), -1) {
		if v, err := strconv.ParseInt(m[1], 10, 64); err == nil {
			proxyUp += v
		}
	}
	for _, m := range reBytesReceived.FindAllStringSubmatch(string(output), -1) {
		if v, err := strconv.ParseInt(m[1], 10, 64); err == nil {
			proxyDown += v
		}
	}

	return &statsResponse{
		Stat: []statEntry{
			{Name: "outbound>>>proxy>>>traffic>>>uplink", Value: strconv.FormatInt(proxyUp, 10)},
			{Name: "outbound>>>proxy>>>traffic>>>downlink", Value: strconv.FormatInt(proxyDown, 10)},
		},
	}, nil
}

func parseValue(s string) int64 {
	var v int64
	fmt.Sscanf(s, "%d", &v)
	return v
}
