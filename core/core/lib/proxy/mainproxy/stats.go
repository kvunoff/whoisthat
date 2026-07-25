package mainproxy

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"time"
	"whoisthat-core/structs"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"

	statscmd "github.com/xtls/xray-core/app/stats/command"
)

// sysfsBase is the root of /sys/class/net/. Exposed as a package var so tests
// can substitute a tempdir; production reads from the real sysfs.
var sysfsBase = "/sys/class/net"

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

func (p *ProxyManager) collectStats(socksPort int, apiPort int, tunName string, cancel chan struct{}, out chan<- structs.TrafficStats) {
	ticker := time.NewTicker(1 * time.Second)
	defer ticker.Stop()

	var prevProxyUp, prevProxyDown int64
	var prevDirectTx, prevDirectRx int64
	var firstRun = true

	for {
		select {
		case <-cancel:
			return
		case <-ticker.C:
			var proxyUp, proxyDown int64
			var err error
			if apiPort > 0 {
				ctx, ctxCancel := context.WithTimeout(context.Background(), 500*time.Millisecond)
				proxyUp, proxyDown, err = queryXrayStats(ctx, apiPort)
				ctxCancel()
			} else {
				proxyUp, proxyDown, err = querySsStats(socksPort)
			}
			if err != nil {
				continue
			}

			var directRx, directTx int64
			if tunName != "" {
				directRx, directTx, err = readIfaceBytes(tunName)
				if err != nil {
					// TUN device may not be up (or was just removed); treat
					// as no-meter movement: keep prevs, delta stays 0.
					directRx = prevDirectRx
					directTx = prevDirectTx
				}
			}

			if firstRun {
				firstRun = false
				prevProxyUp, prevProxyDown = proxyUp, proxyDown
				prevDirectTx, prevDirectRx = directTx, directRx
				continue
			}

			delta := structs.TrafficStats{
				ProxyUp:    proxyUp - prevProxyUp,
				ProxyDown:  proxyDown - prevProxyDown,
				DirectUp:   directTx - prevDirectTx,
				DirectDown: directRx - prevDirectRx,
			}

			prevProxyUp, prevProxyDown = proxyUp, proxyDown
			prevDirectTx, prevDirectRx = directTx, directRx

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

// queryXrayStats queries the running xray-core's gRPC StatsService for the
// cumulative counter of the tagged "proxy" outbound's uplink/downlink.
func queryXrayStats(ctx context.Context, apiPort int) (up, down int64, err error) {
	conn, err := grpc.NewClient(
		fmt.Sprintf("127.0.0.1:%d", apiPort),
		grpc.WithTransportCredentials(insecure.NewCredentials()),
	)
	if err != nil {
		return 0, 0, err
	}
	defer conn.Close()

	resp, err := statscmd.NewStatsServiceClient(conn).QueryStats(ctx, &statscmd.QueryStatsRequest{
		Pattern: "outbound>>>proxy>>>traffic",
		Reset_:  false,
	})
	if err != nil {
		return 0, 0, err
	}
	for _, s := range resp.Stat {
		switch {
		case strings.HasSuffix(s.Name, "uplink"):
			up = s.Value
		case strings.HasSuffix(s.Name, "downlink"):
			down = s.Value
		}
	}
	return up, down, nil
}

// readIfaceBytes reads rx/tx byte counters from sysfs for the given network
// interface (e.g. the TUN device). rx is receive (download), tx is transmit
// (upload), matching sysfs naming.
func readIfaceBytes(iface string) (rx, tx int64, err error) {
	base := filepath.Join(sysfsBase, iface, "statistics")
	rx, err = readSysfsInt(filepath.Join(base, "rx_bytes"))
	if err != nil {
		return
	}
	tx, err = readSysfsInt(filepath.Join(base, "tx_bytes"))
	return
}

func readSysfsInt(path string) (int64, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return 0, err
	}
	return strconv.ParseInt(strings.TrimSpace(string(b)), 10, 64)
}