package mainproxy

import (
	"encoding/json"
	"fmt"
	"os/exec"
	"regexp"
	"strconv"
	"time"
	"whoisthat-core/structs"
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
			proxyUp, proxyDown, err := querySsStats(socksPort)
			if err != nil {
				continue
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

func querySsStats(port int) (up, down int64, err error) {
	filter := fmt.Sprintf("dport = :%d", port)
	cmd := exec.Command("ss", "-tie", filter)
	output, cmdErr := cmd.Output()
	if cmdErr != nil {
		if len(output) > 0 {
			return 0, 0, fmt.Errorf("ss (port %d): %w (output: %s)", port, cmdErr, string(output))
		}
		return 0, 0, fmt.Errorf("ss (port %d): %w", port, cmdErr)
	}
	for _, m := range reBytesSent.FindAllStringSubmatch(string(output), -1) {
		if v, e := strconv.ParseInt(m[1], 10, 64); e == nil {
			up += v
		}
	}
	for _, m := range reBytesReceived.FindAllStringSubmatch(string(output), -1) {
		if v, e := strconv.ParseInt(m[1], 10, 64); e == nil {
			down += v
		}
	}
	return up, down, nil
}
