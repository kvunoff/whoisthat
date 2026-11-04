package mainproxy

import (
	"encoding/json"
	"fmt"
	"time"

	"whoisthat-core/lib"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/proxy/xray"
	"whoisthat-core/structs"
)

func injectRoutingConfig(configJSON []byte, selection structs.RoutingSelection, tunActive bool) ([]byte, error) {
	var config map[string]interface{}
	if err := json.Unmarshal(configJSON, &config); err != nil {
		return configJSON, err
	}

	userRules := buildUserRules(selection.Config.Rules)
	happDNS := selection.Source == "happ" && selection.Config.DNS != nil
	fakeDNS := fakeDNSActive(selection, tunActive)

	var rules []map[string]interface{}
	if happDNS {
		dnsSection, hasDomestic, hasRemote := buildHappDNSSection(selection, fakeDNS)

		if _, hasAPI := config["api"]; hasAPI {
			rules = append(rules, apiRule())
		}
		if tunActive {
			rules = append(rules, dnsCaptureRule())
		}
		if hasDomestic {
			rules = append(rules, inboundRule("domestic-dns", "direct"))
		}
		if hasRemote {
			rules = append(rules, inboundRule("remote-dns", "proxy"))
		}

		config["dns"] = dnsSection
		if fakeDNS {
			config["fakedns"] = fakeDNSPools()
		}
	} else {
		// Legacy behaviour for manual routing and Happ profiles without DNS:
		// keep DNS queries away from user domain rules. With Happ DNS active the
		// dns-capture/dns-out pair replaces this rule.
		rules = append(rules, legacyDNSRule())
		if _, hasAPI := config["api"]; hasAPI {
			rules = append(rules, apiRule())
		}

		// The freedom (direct) outbound needs a resolver. Preserve an existing
		// dns section, otherwise fall back to the configured servers.
		if _, exists := config["dns"]; !exists {
			config["dns"] = map[string]interface{}{"servers": fallbackDNSServers()}
		}
	}

	rules = append(rules, userRules...)

	domainStrategy := selection.Config.DomainStrategy
	if domainStrategy == "" {
		domainStrategy = "IPIfNonMatch"
	}
	// Fake IPs break IP-based rules unless Xray resolves the real address after
	// sniffing. AsIs never does that, so force the next safest strategy.
	if fakeDNS && domainStrategy == "AsIs" {
		logger.Warn("fake DNS: forcing IPIfNonMatch instead of AsIs")
		domainStrategy = "IPIfNonMatch"
	}

	config["routing"] = map[string]interface{}{
		"domainStrategy": domainStrategy,
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
					if fakeDNS && inbound["tag"] == "tun-in" {
						addDestOverride(sniffing, "fakedns")
					}
				}
			}
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
	if happDNS && tunActive && !outboundTagExists(outboundsRaw, "dns-out") {
		outboundsRaw = append(outboundsRaw, map[string]interface{}{
			"tag":      "dns-out",
			"protocol": "dns",
			"settings": map[string]interface{}{},
		})
	}
	config["outbounds"] = outboundsRaw

	result, err := json.Marshal(config)
	if err != nil {
		return configJSON, err
	}
	return result, nil
}

func buildUserRules(configured []structs.RoutingRule) []map[string]interface{} {
	var rules []map[string]interface{}
	for _, r := range configured {
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
		if r.Network != "" {
			rule["network"] = r.Network
		}
		rules = append(rules, rule)
	}
	return rules
}

// apiRule routes xray StatsService gRPC traffic to the api outbound before any
// other rule. Only emitted when injectStatsConfig actually added the api
// service; otherwise it would reference a nonexistent outbound.
func apiRule() map[string]interface{} {
	return map[string]interface{}{
		"type":        "field",
		"outboundTag": "api",
		"inboundTag":  []string{"api"},
	}
}

// dnsCaptureRule intercepts DNS arriving through the TUN inbound and hands it to
// Xray's DNS module instead of proxying the raw query.
func dnsCaptureRule() map[string]interface{} {
	return map[string]interface{}{
		"type":        "field",
		"inboundTag":  []string{"tun-in"},
		"port":        "53",
		"network":     "tcp,udp",
		"outboundTag": "dns-out",
	}
}

func inboundRule(inboundTag, outboundTag string) map[string]interface{} {
	return map[string]interface{}{
		"type":        "field",
		"inboundTag":  []string{inboundTag},
		"outboundTag": outboundTag,
	}
}

func legacyDNSRule() map[string]interface{} {
	return map[string]interface{}{
		"type":        "field",
		"port":        "53",
		"network":     "udp",
		"outboundTag": "proxy",
	}
}

// buildHappDNSSection returns the dns object plus which tagged servers were
// actually emitted. Rules are only generated for existing servers so we never
// reference a missing tag.
const (
	fakeDNSPool4    = "198.19.0.0/16"
	fakeDNSPool6    = "fc00::/18"
	fakeDNSPoolSize = 32768
)

// fakeDNSActive decides whether fake IP allocation is safe for this selection.
// Fake DNS is only used when the tunnel is up, the profile explicitly requests
// it, all unmatched traffic is proxied, and split-include is not active.
func fakeDNSActive(selection structs.RoutingSelection, tunActive bool) bool {
	if !tunActive || selection.Source != "happ" || selection.Config.DNS == nil {
		return false
	}
	if !selection.Config.DNS.FakeDNSEnabled || !selection.GlobalProxy {
		return false
	}
	return appconfig.GetConfig().SplitTunnel.Mode != "include"
}

func fakeDNSPools() []map[string]interface{} {
	return []map[string]interface{}{
		{"ipPool": fakeDNSPool4, "poolSize": fakeDNSPoolSize},
		{"ipPool": fakeDNSPool6, "poolSize": fakeDNSPoolSize},
	}
}

func addDestOverride(sniffing map[string]interface{}, value string) {
	var overrides []interface{}
	if raw, ok := sniffing["destOverride"].([]interface{}); ok {
		overrides = append(overrides, raw...)
	}
	for _, item := range overrides {
		if existing, ok := item.(string); ok && existing == value {
			return
		}
	}
	sniffing["destOverride"] = append(overrides, value)
}

func (p *ProxyManager) ReloadRouting(tunName string) error {
	p.mu.Lock()
	defer p.mu.Unlock()

	if p.status.Connection != "connected" || p.core == nil || !p.core.IsRunning() {
		return nil
	}

	return p.reloadCoreLocked(tunName)
}

// reloadCoreLocked regenerates xray configuration with current routing and tun settings,
// pre-flight tests it, and swaps the active core instance. Caller must hold p.mu.
func (p *ProxyManager) reloadCoreLocked(tunName string) error {
	profile := p.status.Profile

	app_config := appconfig.GetConfig()
	tunArg := ""
	if p.tunEnabled {
		tunArg = tunName
	}
	apiPort := p.statsApiPort
	if apiPort == 0 && p.portPool != nil {
		allocated, apiErr := p.portPool.GetPort()
		if apiErr == nil {
			apiPort = allocated
		}
	}

	var xray_config []byte
	var selection structs.RoutingSelection
	statsTags := p.statsTags
	if len(statsTags) == 0 {
		statsTags = []string{"proxy"}
	}

	if profile.Managed && p.DB != nil {
		manual, _, _ := p.DB.LoadEffectiveManualRouting(profile.GroupId)
		geoDir, _ := p.DB.ResolveGroupGeoDir(profile.GroupId)
		selection = structs.RoutingSelection{
			Config:         manual,
			Source:         "manual",
			GeoDir:         geoDir,
			BootstrapHosts: p.bootstrapHosts,
		}
		cfg, tags, err := p.prepareManagedConfig(profile, app_config.SocksPort, app_config.HttpPort, tunArg, apiPort, geoDir, manual, p.bootstrapHosts)
		if err != nil {
			return fmt.Errorf("failed to build provider JSON config: %w", err)
		}
		xray_config = cfg
		if len(tags) > 0 {
			statsTags = tags
		}
		logger.Infof("routing: reload provider JSON (%d outbounds, %d manual rules, source=manual)",
			countOutbounds(xray_config), len(manual.Rules))
	} else {
		var err error
		xray_config, err = lib.ParseUriWithTun(profile.Uri, app_config.SocksPort, app_config.HttpPort, tunArg)
		if err != nil {
			return fmt.Errorf("failed to parse profile uri: %w", err)
		}

		xray_config, err = injectStatsConfig(xray_config, apiPort)
		if err != nil {
			logger.Warnf("failed to inject stats config: %v", err)
		}

		if p.DB != nil {
			var selErr error
			selection, selErr = p.DB.ResolveRouting(profile.GroupId)
			if selErr != nil {
				logger.Warnf("reloadCoreLocked: failed to resolve routing for group %d: %v", profile.GroupId, selErr)
			}
			selection.BootstrapHosts = bootstrapHosts(profile.Address, p.profileIPs)
		} else {
			selection = structs.RoutingSelection{Source: "manual"}
		}

		if p.DB != nil {
			prevOutboundsCount := countOutbounds(xray_config)
			xray_config, err = injectRoutingConfig(xray_config, selection, p.tunEnabled)
			if err != nil {
				return fmt.Errorf("failed to inject routing config: %w", err)
			}
			afterOutboundsCount := countOutbounds(xray_config)
			logger.Infof("routing: reload outbounds %d → %d (source=%s)", prevOutboundsCount, afterOutboundsCount, selection.Source)
		}
	}

	// Pre-flight test the new configuration before terminating the active core.
	// If xray detects an invalid rule (e.g. malformed geoip/geosite or a broken
	// DNS section), the running connection is preserved intact.
	validate := true
	testConfigBytes := xray_config
	if p.tunEnabled {
		var rawConfig json.RawMessage
		if profile.Managed && p.DB != nil {
			rawConfig, _ = p.DB.LoadProfileXrayConfig(profile.GroupId, profile.Id)
		}
		if testBase, buildErr := buildPreflightConfig(profile, app_config.SocksPort, app_config.HttpPort, apiPort, selection, p.DB != nil, true, rawConfig); buildErr != nil {
			logger.Warnf("preflight: could not build tun-less test config, skipping validation: %v", buildErr)
			validate = false
		} else {
			testConfigBytes = testBase
		}
	}
	if validate {
		if err := validateXrayConfig(testConfigBytes, selection.GeoDir); err != nil {
			return err
		}
	}

	// Retire the active exit-watcher so it doesn't fire on intentional Stop()
	p.retireExitWatcher()

	p.core.Stop()
	time.Sleep(30 * time.Millisecond)

	newCore := &xray.XrayCore{Exited: make(chan error, 1)}
	if err := newCore.Start(xray_config, selection.GeoDir); err != nil {
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
	p.statsTags = statsTags
	go p.collectStats(app_config.SocksPort, p.statsApiPort, statsTags, tunName, p.statsCancel, p.StatsChanged)

	p.startExitWatcher()

	logger.Infof("core reloaded successfully for %s (tun=%v)", profile.Name, p.tunEnabled)
	return nil
}
