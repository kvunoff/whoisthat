package mainproxy

import (
	"encoding/json"
	"fmt"
	"net/url"
	"strings"
	"time"
	"whoisthat-core/lib"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/proxy/xray"
	"whoisthat-core/structs"
)

// injectRoutingConfig merges the effective routing rules and the optional Happ
// DNS configuration into an Xray config. tunActive controls only the DNS
// capture rule: the DNS section itself is emitted whenever the selection comes
// from a Happ profile with DNS settings, because the freedom outbound may need
// it in proxy mode too.
func injectRoutingConfig(configJSON []byte, selection structs.RoutingSelection, tunActive bool) ([]byte, error) {
	var config map[string]interface{}
	if err := json.Unmarshal(configJSON, &config); err != nil {
		return configJSON, err
	}

	userRules := buildUserRules(selection.Config.Rules)
	happDNS := selection.Source == "happ" && selection.Config.DNS != nil

	var rules []map[string]interface{}
	if happDNS {
		dnsSection, hasDomestic, hasRemote := buildHappDNSSection(selection)

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
func buildHappDNSSection(selection structs.RoutingSelection) (map[string]interface{}, bool, bool) {
	cfg := selection.Config.DNS
	domesticAddr := dnsServerAddress(cfg.DomesticDNSType, cfg.DomesticDNSDomain, cfg.DomesticDNSIP)
	remoteAddr := dnsServerAddress(cfg.RemoteDNSType, cfg.RemoteDNSDomain, cfg.RemoteDNSIP)

	directDomains := domainsForOutbound(selection.Config.Rules, "direct")
	proxyDomains := domainsForOutbound(selection.Config.Rules, "proxy")

	var servers []map[string]interface{}
	hasDomestic := false
	hasRemote := false

	if selection.GlobalProxy {
		// Everything not explicitly direct goes through the proxy. The domestic
		// server is only useful for direct domains; remote is the fallback.
		if domesticAddr != "" && len(directDomains) > 0 {
			servers = append(servers, dnsServer("domestic-dns", domesticAddr, directDomains, true))
			hasDomestic = true
		}
		if remoteAddr != "" {
			servers = append(servers, dnsServer("remote-dns", remoteAddr, nil, false))
			hasRemote = true
		}
	} else {
		// GlobalProxy=false: everything is direct by default. Domestic is the
		// fallback resolver; remote is used only for explicit proxy domains.
		if domesticAddr != "" {
			servers = append(servers, dnsServer("domestic-dns", domesticAddr, nil, false))
			hasDomestic = true
		}
		if remoteAddr != "" {
			if domesticAddr == "" {
				servers = append(servers, dnsServer("remote-dns", remoteAddr, nil, false))
				hasRemote = true
			} else if len(proxyDomains) > 0 {
				servers = append(servers, dnsServer("remote-dns", remoteAddr, proxyDomains, true))
				hasRemote = true
			}
		}
	}

	// Unmatched queries need at least one server without a domains filter.
	if !hasFallbackServer(servers) {
		servers = append(servers, fallbackDNSServers()...)
	}
	if servers == nil {
		servers = fallbackDNSServers()
	}

	section := map[string]interface{}{
		"servers": servers,
	}
	if hosts := buildDNSHosts(selection); len(hosts) > 0 {
		section["hosts"] = hosts
	}
	return section, hasDomestic, hasRemote
}

func dnsServerAddress(kind, domain, ip string) string {
	switch kind {
	case "DoH":
		return domain
	case "DoU":
		return ip
	}
	if domain != "" {
		return domain
	}
	return ip
}

func dnsServer(tag, address string, domains []string, skipFallback bool) map[string]interface{} {
	server := map[string]interface{}{
		"address": address,
		"tag":     tag,
	}
	if len(domains) > 0 {
		server["domains"] = domains
		server["skipFallback"] = skipFallback
	}
	return server
}

func fallbackDNSServers() []map[string]interface{} {
	configured := appconfig.GetConfig().DnsServers
	servers := make([]map[string]interface{}, 0, len(configured))
	for _, address := range configured {
		servers = append(servers, map[string]interface{}{"address": address})
	}
	return servers
}

func hasFallbackServer(servers []map[string]interface{}) bool {
	for _, server := range servers {
		if _, hasDomains := server["domains"]; !hasDomains {
			return true
		}
	}
	return false
}

// buildDNSHosts merges static hosts from the profile, the DoH/DoU bootstrap
// entries, and the already resolved VPN server addresses. Earlier entries win.
func buildDNSHosts(selection structs.RoutingSelection) map[string]interface{} {
	cfg := selection.Config.DNS
	hosts := make(map[string]interface{})

	for domain, ip := range cfg.Hosts {
		addDNSHost(hosts, domain, ip)
	}
	if cfg.RemoteDNSDomain != "" && cfg.RemoteDNSIP != "" {
		addDNSHost(hosts, hostFromDNSAddress(cfg.RemoteDNSDomain), cfg.RemoteDNSIP)
	}
	if cfg.DomesticDNSDomain != "" && cfg.DomesticDNSIP != "" {
		addDNSHost(hosts, hostFromDNSAddress(cfg.DomesticDNSDomain), cfg.DomesticDNSIP)
	}
	for domain, ips := range selection.BootstrapHosts {
		addDNSHost(hosts, domain, ips...)
	}

	return hosts
}

func addDNSHost(hosts map[string]interface{}, domain string, ips ...string) {
	domain = strings.TrimSpace(domain)
	if domain == "" {
		return
	}
	if _, exists := hosts[domain]; exists {
		return
	}

	values := make([]string, 0, len(ips))
	for _, ip := range ips {
		if ip = strings.TrimSpace(ip); ip != "" {
			values = append(values, ip)
		}
	}
	switch len(values) {
	case 0:
		return
	case 1:
		hosts[domain] = values[0]
	default:
		hosts[domain] = values
	}
}

func hostFromDNSAddress(address string) string {
	address = strings.TrimSpace(address)
	if address == "" {
		return ""
	}
	if u, err := url.Parse(address); err == nil && u.Hostname() != "" {
		return u.Hostname()
	}
	return address
}

// domainsForOutbound collects domain matchers from the compiled rules for one
// outbound tag. The same patterns can safely be reused by Xray's DNS server
// selection (including geosite:/regexp: forms).
func domainsForOutbound(rules []structs.RoutingRule, outboundTag string) []string {
	seen := make(map[string]struct{})
	var domains []string
	for _, rule := range rules {
		if !rule.Enabled || rule.OutboundTag != outboundTag || strings.TrimSpace(rule.Domain) == "" {
			continue
		}
		for _, domain := range splitAndTrim(rule.Domain) {
			key := strings.ToLower(domain)
			if _, exists := seen[key]; exists {
				continue
			}
			seen[key] = struct{}{}
			domains = append(domains, domain)
		}
	}
	return domains
}

func outboundTagExists(outbounds []interface{}, tag string) bool {
	for _, raw := range outbounds {
		outbound, ok := raw.(map[string]interface{})
		if !ok {
			continue
		}
		if outbound["tag"] == tag {
			return true
		}
	}
	return false
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
// ReloadRouting hot-reloads the running xray-core instance
// with the new rules in-place, without dropping connection state or resetting
// the TUN device. If the proxy is disconnected, ReloadRouting is a no-op.
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
	xray_config, err := lib.ParseUriWithTun(profile.Uri, app_config.SocksPort, app_config.HttpPort, tunArg)
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

	var selection structs.RoutingSelection
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

	// Pre-flight test the new configuration before terminating the active core.
	// If xray detects an invalid rule (e.g. malformed geoip/geosite or a broken
	// DNS section), the running connection is preserved intact.
	validate := true
	testConfigBytes := xray_config
	if p.tunEnabled {
		if testBase, buildErr := buildPreflightConfig(profile, app_config.SocksPort, app_config.HttpPort, apiPort, selection, p.DB != nil, true); buildErr != nil {
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
	go p.collectStats(app_config.SocksPort, p.statsApiPort, tunName, p.statsCancel, p.StatsChanged)

	p.startExitWatcher()

	logger.Infof("core reloaded successfully for %s (tun=%v)", profile.Name, p.tunEnabled)
	return nil
}
