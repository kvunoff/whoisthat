package xrayjson

import (
	"encoding/json"
	"errors"
	"fmt"
	"strings"

	"whoisthat-core/structs"
)

// Options controls the whoisthat-specific adaptations applied to a provider's
// full Xray configuration. Everything not mentioned here (routing rules,
// balancers, DNS servers, outbound transports) is preserved verbatim.
type Options struct {
	SocksPort int
	HttpPort  int
	TunName   string
	APIPort   int

	DNSServers         []string
	BootstrapHosts     map[string][]string
	UserRules          []structs.RoutingRule
	UserDomainStrategy string
	// GlobalGeoExt is set when the runtime asset directory also contains
	// geoip-global.dat/geosite-global.dat next to the provider's own geo files.
	// Manual rules with global geo codes are then compiled against those copies.
	GlobalGeoExt bool
}

// Prepared is the runtime configuration plus metadata the proxy manager needs.
type Prepared struct {
	Config    []byte
	ProxyTags []string
}

// Prepare merges the manual (user) rules into the provider configuration and
// applies the structural changes whoisthat needs: its own inbounds, stats API,
// TUN DNS capture, fwmark on freedom outbounds and bootstrap DNS hosts.
//
// Rule priority is manual rules first, provider rules after, so a user override
// always wins while the provider configuration remains the default policy.
func Prepare(raw json.RawMessage, opts Options) (Prepared, error) {
	var config map[string]interface{}
	if err := json.Unmarshal(raw, &config); err != nil {
		return Prepared{}, fmt.Errorf("parse xray config: %w", err)
	}

	outbounds := asSlice(config["outbounds"])
	if len(outbounds) == 0 {
		return Prepared{}, errors.New("xray config has no outbounds")
	}
	proxyTags := collectProxyTags(outbounds)
	if len(proxyTags) == 0 {
		return Prepared{}, errors.New("xray config has no proxy outbound")
	}

	outbounds = ensureServiceOutbounds(outbounds)
	markFreedomOutbounds(outbounds)
	fixDanglingDialerProxy(outbounds, proxyTags)
	config["outbounds"] = outbounds

	config["inbounds"] = rewriteInbounds(config["inbounds"], opts)

	config["dns"] = mergeDNS(config["dns"], opts)

	routing := asMap(config["routing"])
	config["routing"] = mergeRouting(routing, outbounds, proxyTags, opts)

	result, err := json.Marshal(config)
	if err != nil {
		return Prepared{}, fmt.Errorf("marshal xray config: %w", err)
	}

	return Prepared{
		Config:    result,
		ProxyTags: proxyTags,
	}, nil
}

func rewriteInbounds(value interface{}, opts Options) []interface{} {
	var out []interface{}
	for _, raw := range asSlice(value) {
		inbound, ok := raw.(map[string]interface{})
		if !ok {
			continue
		}
		protocol, _ := inbound["protocol"].(string)
		switch protocol {
		case "socks":
			if opts.SocksPort <= 0 {
				continue
			}
			inbound["port"] = opts.SocksPort
			inbound["listen"] = "127.0.0.1"
			inbound["sniffing"] = sniffing(inbound["sniffing"])
			out = append(out, inbound)
		case "http":
			if opts.HttpPort <= 0 {
				continue
			}
			inbound["port"] = opts.HttpPort
			inbound["listen"] = "127.0.0.1"
			inbound["sniffing"] = sniffing(inbound["sniffing"])
			out = append(out, inbound)
		}
	}

	if opts.SocksPort > 0 && !hasInboundProtocol(out, "socks") {
		out = append(out, map[string]interface{}{
			"tag":      "socks-in",
			"protocol": "socks",
			"port":     opts.SocksPort,
			"listen":   "127.0.0.1",
			"settings": map[string]interface{}{"udp": true, "auth": "noauth"},
			"sniffing": sniffing(nil),
		})
	}
	if opts.HttpPort > 0 && !hasInboundProtocol(out, "http") {
		out = append(out, map[string]interface{}{
			"tag":      "http-in",
			"protocol": "http",
			"port":     opts.HttpPort,
			"listen":   "127.0.0.1",
			"sniffing": sniffing(nil),
		})
	}
	if opts.TunName != "" {
		out = append(out, map[string]interface{}{
			"tag":      "tun-in",
			"protocol": "tun",
			"settings": map[string]interface{}{"name": opts.TunName, "MTU": 1400},
			"sniffing": map[string]interface{}{
				"enabled":      true,
				"routeOnly":    false,
				"metadataOnly": false,
				"destOverride": []string{"http", "tls"},
			},
		})
	}
	return out
}

func sniffing(value interface{}) map[string]interface{} {
	s := asMap(value)
	s["enabled"] = true
	s["routeOnly"] = false
	if _, ok := s["destOverride"]; !ok {
		s["destOverride"] = []string{"http", "tls"}
	}
	return s
}

func hasInboundProtocol(inbounds []interface{}, protocol string) bool {
	for _, raw := range inbounds {
		if inbound, ok := raw.(map[string]interface{}); ok && inbound["protocol"] == protocol {
			return true
		}
	}
	return false
}

func mergeDNS(value interface{}, opts Options) map[string]interface{} {
	dns := asMap(value)
	if servers, ok := dns["servers"].([]interface{}); !ok || len(servers) == 0 {
		configured := opts.DNSServers
		if len(configured) == 0 {
			configured = []string{"1.1.1.1", "8.8.8.8"}
		}
		list := make([]interface{}, 0, len(configured))
		for _, address := range configured {
			list = append(list, map[string]interface{}{"address": address})
		}
		dns["servers"] = list
	}

	hosts := asMap(dns["hosts"])
	for domain, ips := range opts.BootstrapHosts {
		if _, exists := hosts[domain]; exists {
			continue
		}
		if len(ips) == 1 {
			hosts[domain] = ips[0]
		} else if len(ips) > 1 {
			hosts[domain] = ips
		}
	}
	if len(hosts) > 0 {
		dns["hosts"] = hosts
	}
	return dns
}

func mergeRouting(routing map[string]interface{}, outbounds []interface{}, proxyTags []string, opts Options) map[string]interface{} {
	providerRules := asSlice(routing["rules"])

	var structural []interface{}
	if opts.APIPort > 0 {
		structural = append(structural, map[string]interface{}{
			"type":        "field",
			"inboundTag":  []string{"api"},
			"outboundTag": "api",
		})
	}
	if opts.TunName != "" {
		structural = append(structural, map[string]interface{}{
			"type":        "field",
			"inboundTag":  []string{"tun-in"},
			"port":        "53",
			"network":     "tcp,udp",
			"outboundTag": "dns-out",
		})
	}

	userRules, extraBalancer := buildUserRules(opts.UserRules, opts.GlobalGeoExt, outbounds, asSlice(routing["balancers"]), proxyTags)

	rules := make([]interface{}, 0, len(structural)+len(userRules)+len(providerRules))
	rules = append(rules, structural...)
	rules = append(rules, userRules...)
	rules = append(rules, providerRules...)
	routing["rules"] = rules

	if extraBalancer != nil {
		balancers := asSlice(routing["balancers"])
		balancers = append(balancers, extraBalancer)
		routing["balancers"] = balancers
	}
	if _, ok := routing["domainStrategy"]; !ok {
		if opts.UserDomainStrategy != "" {
			routing["domainStrategy"] = opts.UserDomainStrategy
		} else {
			routing["domainStrategy"] = "IPIfNonMatch"
		}
	}
	return routing
}

// buildUserRules translates manual rules into the provider's outbound tags.
// Manual rules only know proxy/direct/block; provider configs often have no
// outbound literally tagged "proxy", so a user balancer is added when needed.
func buildUserRules(configured []structs.RoutingRule, globalGeoExt bool, outbounds, balancers []interface{}, proxyTags []string) ([]interface{}, map[string]interface{}) {
	if len(configured) == 0 {
		return nil, nil
	}

	proxyOutbound := ""
	proxyBalancer := ""
	if contains(proxyTags, "proxy") {
		proxyOutbound = "proxy"
	} else if len(proxyTags) == 1 {
		proxyOutbound = proxyTags[0]
	} else {
		proxyBalancer = uniqueBalancerTag(balancers, outbounds)
	}

	var rules []interface{}
	for _, r := range configured {
		if !r.Enabled {
			continue
		}
		rule := map[string]interface{}{
			"type": r.Type,
		}
		if rule["type"] == "" {
			rule["type"] = "field"
		}
		if r.Domain != "" {
			rule["domain"] = rewriteGeoList(splitAndTrim(r.Domain), globalGeoExt, false)
		}
		if r.IP != "" {
			rule["ip"] = rewriteGeoList(splitAndTrim(r.IP), globalGeoExt, true)
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

		if r.OutboundTag == "proxy" && proxyBalancer != "" {
			rule["balancerTag"] = proxyBalancer
		} else if r.OutboundTag == "proxy" && proxyOutbound != "" {
			rule["outboundTag"] = proxyOutbound
		} else {
			rule["outboundTag"] = r.OutboundTag
		}
		rules = append(rules, rule)
	}

	if proxyBalancer == "" {
		return rules, nil
	}
	return rules, map[string]interface{}{
		"tag":      proxyBalancer,
		"selector": proxyTags,
		"strategy": map[string]interface{}{"type": "random"},
	}
}

func uniqueBalancerTag(existing, outbounds []interface{}) string {
	taken := map[string]bool{}
	for _, raw := range existing {
		if m, ok := raw.(map[string]interface{}); ok {
			if tag, ok := m["tag"].(string); ok {
				taken[tag] = true
			}
		}
	}
	for _, raw := range outbounds {
		if m, ok := raw.(map[string]interface{}); ok {
			if tag, ok := m["tag"].(string); ok {
				taken[tag] = true
			}
		}
	}
	base := "whoisthat-user-proxy"
	tag := base
	for i := 2; taken[tag]; i++ {
		tag = fmt.Sprintf("%s-%d", base, i)
	}
	return tag
}

func collectProxyTags(outbounds []interface{}) []string {
	var tags []string
	for _, raw := range outbounds {
		ob, ok := raw.(map[string]interface{})
		if !ok {
			continue
		}
		protocol, _ := ob["protocol"].(string)
		if serviceProtocols[strings.ToLower(strings.TrimSpace(protocol))] {
			continue
		}
		tag, _ := ob["tag"].(string)
		if tag == "" {
			continue
		}
		tags = append(tags, tag)
	}
	return tags
}

func ensureServiceOutbounds(outbounds []interface{}) []interface{} {
	outbounds = ensureOutbound(outbounds, "direct", "freedom", map[string]interface{}{
		"domainStrategy": "UseIP",
	})
	outbounds = ensureOutbound(outbounds, "block", "blackhole", map[string]interface{}{})
	outbounds = ensureOutbound(outbounds, "dns-out", "dns", map[string]interface{}{})
	return outbounds
}

func ensureOutbound(outbounds []interface{}, tag, protocol string, settings map[string]interface{}) []interface{} {
	for _, raw := range outbounds {
		if m, ok := raw.(map[string]interface{}); ok && m["tag"] == tag {
			return outbounds
		}
	}
	return append(outbounds, map[string]interface{}{
		"tag":      tag,
		"protocol": protocol,
		"settings": settings,
	})
}

func markFreedomOutbounds(outbounds []interface{}) {
	for _, raw := range outbounds {
		ob, ok := raw.(map[string]interface{})
		if !ok || ob["protocol"] != "freedom" {
			continue
		}
		stream := asMap(ob["streamSettings"])
		sockopt := asMap(stream["sockopt"])
		sockopt["mark"] = 1
		stream["sockopt"] = sockopt
		ob["streamSettings"] = stream
	}
}

// fixDanglingDialerProxy repairs sockopt.dialerProxy values that point to an
// outbound tag the provider config does not contain (multik's auto configs
// reference "proxy", which only exists in its simple presets). The provider
// clearly intends DNS to go through the tunnel, so re-point the reference at
// the first proxy outbound instead of silently falling back to direct.
func fixDanglingDialerProxy(outbounds []interface{}, proxyTags []string) {
	tags := map[string]bool{}
	for _, raw := range outbounds {
		if m, ok := raw.(map[string]interface{}); ok {
			if tag, ok := m["tag"].(string); ok {
				tags[tag] = true
			}
		}
	}
	fallback := ""
	if len(proxyTags) > 0 {
		fallback = proxyTags[0]
	}
	for _, raw := range outbounds {
		ob, ok := raw.(map[string]interface{})
		if !ok {
			continue
		}
		stream := asMap(ob["streamSettings"])
		sockopt := asMap(stream["sockopt"])
		target, _ := sockopt["dialerProxy"].(string)
		if target == "" || tags[target] {
			continue
		}
		if fallback == "" || ob["tag"] == fallback {
			delete(sockopt, "dialerProxy")
		} else {
			sockopt["dialerProxy"] = fallback
		}
		if len(sockopt) == 0 {
			delete(stream, "sockopt")
		} else {
			stream["sockopt"] = sockopt
		}
		if len(stream) == 0 {
			delete(ob, "streamSettings")
		} else {
			ob["streamSettings"] = stream
		}
	}
}

func asMap(value interface{}) map[string]interface{} {
	if m, ok := value.(map[string]interface{}); ok {
		return m
	}
	return map[string]interface{}{}
}

func asSlice(value interface{}) []interface{} {
	if s, ok := value.([]interface{}); ok {
		return s
	}
	return nil
}

// rewriteGeoList points manual geo rules at the global copies that
// geo.EnsureGlobalCopies places next to the provider's own assets. Provider
// rules are left untouched and keep using geoip.dat/geosite.dat.
func rewriteGeoList(values []string, enabled, ip bool) []string {
	if !enabled {
		return values
	}
	out := make([]string, 0, len(values))
	for _, value := range values {
		out = append(out, rewriteGeo(value, ip))
	}
	return out
}

func rewriteGeo(value string, ip bool) string {
	if ip {
		switch {
		case strings.HasPrefix(value, "geoip:ext:"):
			return value
		case strings.HasPrefix(value, "geoip:"):
			return "ext-ip:geoip-global.dat:" + strings.TrimPrefix(value, "geoip:")
		}
		return value
	}
	switch {
	case strings.HasPrefix(value, "geosite:ext:"):
		return value
	case strings.HasPrefix(value, "geosite:"):
		return "ext-domain:geosite-global.dat:" + strings.TrimPrefix(value, "geosite:")
	}
	return value
}

func splitAndTrim(value string) []string {
	parts := strings.Split(value, ",")
	out := make([]string, 0, len(parts))
	for _, part := range parts {
		if part = strings.TrimSpace(part); part != "" {
			out = append(out, part)
		}
	}
	return out
}

func contains(list []string, value string) bool {
	for _, item := range list {
		if item == value {
			return true
		}
	}
	return false
}
