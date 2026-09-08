package mainproxy

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"strings"
	"time"
	"whoisthat-core/db"
	"whoisthat-core/lib"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/geo"
	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/proxy/xray"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

func injectRoutingConfig(configJSON []byte, database *db.DB) ([]byte, error) {
	var config map[string]interface{}
	if err := json.Unmarshal(configJSON, &config); err != nil {
		return configJSON, err
	}

	routingCfg, err := database.LoadRouting()
	if err != nil {
		logger.Warnf("failed to load routing config, skipping injection: %v", err)
		return configJSON, nil
	}

	// Build routing rules from config, skipping disabled ones
	var rules []map[string]interface{}
	for _, r := range routingCfg.Rules {
		if !r.Enabled {
			continue
		}
		rule := map[string]interface{}{
			"type":        r.Type,
			"outboundTag": r.OutboundTag,
		}
		if r.Domain != "" {
			rule["domain"] = splitAndTrim(r.Domain)
		}
		if r.IP != "" {
			rule["ip"] = splitAndTrim(r.IP)
		}
		if r.Protocol != "" {
			rule["protocol"] = []string{r.Protocol}
		}
		if r.Port != "" {
			rule["port"] = r.Port
		}
		rules = append(rules, rule)
	}

	// Route API gRPC traffic (xray StatsService) to the api handler outbound
	// before any other rule. The api inbound is dokodemo-door on 127.0.0.1
	// listening for our gRPC stats queries; without this rule those queries
	// would be misrouted through user/DNS rules. Only emit if injectStatsConfig
	// actually added the `api` service (apiPort > 0); otherwise the rule would
	// reference a nonexistent outbound and break xray startup.
	if _, hasAPI := config["api"]; hasAPI {
		rules = append([]map[string]interface{}{{
			"type":        "field",
			"outboundTag": "api",
			"inboundTag":  []string{"api"},
		}}, rules...)
	}

	// Route all DNS traffic through proxy first — prevents DNS queries from
	// matching user domain rules (e.g. ifconfig.me → direct would also send
	// the DNS query for ifconfig.me to the direct outbound, breaking resolution).
	rules = append([]map[string]interface{}{{
		"type":        "field",
		"port":        "53",
		"network":     "udp",
		"outboundTag": "proxy",
	}}, rules...)

	config["routing"] = map[string]interface{}{
		"domainStrategy": routingCfg.DomainStrategy,
		"rules":          rules,
	}

	// Set routeOnly to false on all inbounds so sniffed domains replace
	// the original destination (needed for domain-based routing to work
	// when the client resolves DNS locally and sends IPs through SOCKS).
	if inbounds, ok := config["inbounds"].([]interface{}); ok {
		for _, ib := range inbounds {
			if inbound, ok := ib.(map[string]interface{}); ok {
				if sniffing, ok := inbound["sniffing"].(map[string]interface{}); ok {
					sniffing["routeOnly"] = false
				}
			}
		}
	}

	// DNS section — needed for the freedom (direct) outbound to resolve domains.
	// Without this, domain-routed direct traffic has no DNS resolver.
	if _, exists := config["dns"]; !exists {
		dnsServersRaw := make([]map[string]interface{}, len(appconfig.GetConfig().DnsServers))
		for i, s := range appconfig.GetConfig().DnsServers {
			dnsServersRaw[i] = map[string]interface{}{"address": s}
		}
		config["dns"] = map[string]interface{}{
			"servers": dnsServersRaw,
		}
	}

	// Add direct and block outbounds
	outboundsRaw, ok := config["outbounds"].([]interface{})
	if !ok {
		logger.Warn("outbounds not found in config, cannot inject routing outbounds")
		return json.Marshal(config)
	}

	// Tag the existing proxy outbound
	if len(outboundsRaw) > 0 {
		if ob, ok := outboundsRaw[0].(map[string]interface{}); ok {
			if ob["tag"] == nil || ob["tag"] == "" {
				ob["tag"] = "proxy"
			}
		}
	}

	direct := map[string]interface{}{
		"tag":      "direct",
		"protocol": "freedom",
		"settings": map[string]interface{}{
			"domainStrategy": "UseIP",
		},
		"streamSettings": map[string]interface{}{
			"sockopt": map[string]interface{}{
				"mark": 1,
			},
		},
	}
	block := map[string]interface{}{
		"tag":      "block",
		"protocol": "blackhole",
		"settings": map[string]interface{}{},
	}

	outboundsRaw = append(outboundsRaw, direct, block)
	config["outbounds"] = outboundsRaw

	result, err := json.Marshal(config)
	if err != nil {
		return configJSON, err
	}
	return result, nil
}

func splitAndTrim(s string) []string {
	parts := strings.Split(s, ",")
	var result []string
	for _, p := range parts {
		p = strings.TrimSpace(p)
		if p != "" {
			result = append(result, p)
		}
	}
	return result
}

// ReloadRouting recompiles the routing configuration and restarts xray-core
// with the new rules in-place, without dropping connection state or resetting
// the TUN device. If the proxy is disconnected or running under hysteria2,
// ReloadRouting is a no-op.
func (p *ProxyManager) ReloadRouting(tunName string) error {
	p.mu.Lock()
	defer p.mu.Unlock()

	if p.status.Connection != "connected" || p.core == nil || !p.core.IsRunning() {
		return nil
	}

	profile := p.status.Profile
	if isHysteriaProtocol(profile.Protocol) {
		logger.Info("routing: profile is hysteria2, skipping xray routing reload")
		return nil
	}

	app_config := appconfig.GetConfig()
	xray_config, err := lib.ParseUri(profile.Uri, app_config.SocksPort, app_config.HttpPort)
	if err != nil {
		return fmt.Errorf("failed to parse profile uri: %w", err)
	}

	apiPort := p.statsApiPort
	if apiPort == 0 && p.portPool != nil {
		allocated, apiErr := p.portPool.GetPort()
		if apiErr == nil {
			apiPort = allocated
		}
	}

	xray_config, err = injectStatsConfig(xray_config, apiPort)
	if err != nil {
		logger.Warnf("failed to inject stats config: %v", err)
	}

	if p.DB != nil {
		prevOutboundsCount := countOutbounds(xray_config)
		xray_config, err = injectRoutingConfig(xray_config, p.DB)
		if err != nil {
			return fmt.Errorf("failed to inject routing config: %w", err)
		}
		afterOutboundsCount := countOutbounds(xray_config)
		logger.Infof("routing: reload outbounds %d → %d", prevOutboundsCount, afterOutboundsCount)
	}

	// Pre-flight test the new configuration before terminating the active core.
	// If xray detects an invalid rule (e.g. malformed geoip/geosite or syntax),
	// the running connection is preserved intact.
	if xraybin, err := utils.GetXrayBin(); err == nil {
		ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
		cmd := exec.CommandContext(ctx, xraybin, "run", "-test")
		if ad := geo.AssetDir(); ad != "" {
			cmd.Env = append(os.Environ(), "XRAY_LOCATION_ASSET="+ad)
		}
		cmd.Stdin = bytes.NewReader(xray_config)
		out, testErr := cmd.CombinedOutput()
		cancel()
		if testErr != nil {
			return fmt.Errorf("xray test failed: %s", strings.TrimSpace(string(out)))
		}
	}

	// Retire the active exit-watcher so it doesn't fire on intentional Stop()
	p.retireExitWatcher()

	p.core.Stop()
	time.Sleep(30 * time.Millisecond)

	newCore := &xray.XrayCore{Exited: make(chan error, 1)}
	if err := newCore.Start(xray_config); err != nil {
		p.core = newCore
		p.status = structs.ProxyStatus{
			Connection: "disconnected",
		}
		select {
		case p.StatusChanged <- p.status:
		default:
		}
		return fmt.Errorf("failed to restart xray: %w", err)
	}

	p.core = newCore

	// Restart stats collector with the new core instance
	if p.statsCancel != nil {
		close(p.statsCancel)
		p.statsCancel = nil
	}
	p.statsCancel = make(chan struct{})
	p.statsApiPort = apiPort
	go p.collectStats(app_config.SocksPort, p.statsApiPort, tunName, p.statsCancel, p.StatsChanged)

	p.startExitWatcher()

	logger.Infof("routing: reloaded successfully for %s", profile.Name)
	return nil
}
