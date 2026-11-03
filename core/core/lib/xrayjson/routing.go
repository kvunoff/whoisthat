package xrayjson

import (
	"fmt"
	"strings"

	"whoisthat-core/structs"
)

func mergeRouting(routing map[string]interface{}, outbounds []interface{}, proxyTags []string, opts Options) map[string]interface{} {
	balancers := asSlice(routing["balancers"])
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

	defaultTarget, hasDefault := defaultProxyTarget(providerRules, outbounds, balancers, proxyTags)
	userRules, extraBalancer := buildUserRules(
		opts.UserRules, opts.GlobalGeoExt, outbounds, balancers, proxyTags,
		defaultTarget, hasDefault,
	)

	rules := make([]interface{}, 0, len(structural)+len(userRules)+len(providerRules))
	rules = append(rules, structural...)
	rules = append(rules, userRules...)
	rules = append(rules, providerRules...)
	routing["rules"] = rules

	if extraBalancer != nil {
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

// proxyTarget is the provider outbound or balancer that manual "proxy" rules
// should use.
type proxyTarget struct {
	tag      string
	balancer bool
}

// defaultProxyTarget returns the target of the provider's first catch-all rule.
// Xray evaluates rules in order, so later catch-alls are unreachable. A
// catch-all that does not lead to a proxy outbound means the provider has no
// usable default proxy target.
func defaultProxyTarget(providerRules, outbounds, balancers []interface{}, proxyTags []string) (proxyTarget, bool) {
	for _, raw := range providerRules {
		rule, ok := raw.(map[string]interface{})
		if !ok || !isCatchAll(rule) {
			continue
		}
		// Xray prefers outboundTag when both target fields are present.
		if tag, _ := rule["outboundTag"].(string); tag != "" {
			if contains(proxyTags, tag) {
				return proxyTarget{tag: tag}, true
			}
			return proxyTarget{}, false
		}
		if tag, _ := rule["balancerTag"].(string); tag != "" {
			if balancerSelectsOnlyProxy(balancers, outbounds, tag, proxyTags) {
				return proxyTarget{tag: tag, balancer: true}, true
			}
			return proxyTarget{}, false
		}
		return proxyTarget{}, false
	}
	return proxyTarget{}, false
}

// isCatchAll reports whether a rule matches every request. The whitelist is
// deliberate: any unknown field may be a selector added by a future Xray
// release, so it fails safe and does not treat the rule as a catch-all.
func isCatchAll(rule map[string]interface{}) bool {
	for key, value := range rule {
		switch key {
		case "type", "ruleTag", "outboundTag", "balancerTag":
			// No match constraint.
		case "network":
			if !isFullNetwork(value) {
				return false
			}
		default:
			return false
		}
	}
	return true
}

// isFullNetwork accepts the string and array forms of Xray's network field.
func isFullNetwork(value interface{}) bool {
	set := map[string]bool{}
	add := func(raw string) {
		if part := strings.ToLower(strings.TrimSpace(raw)); part != "" {
			set[part] = true
		}
	}
	switch v := value.(type) {
	case string:
		for _, part := range strings.Split(v, ",") {
			add(part)
		}
	case []string:
		for _, part := range v {
			add(part)
		}
	case []interface{}:
		for _, raw := range v {
			part, ok := raw.(string)
			if !ok {
				return false
			}
			add(part)
		}
	default:
		return false
	}
	return len(set) == 2 && set["tcp"] && set["udp"]
}

// balancerSelectsOnlyProxy mirrors Xray's prefix matching for balancer
// selectors and rejects selectors that can pick a service outbound.
func balancerSelectsOnlyProxy(balancers, outbounds []interface{}, tag string, proxyTags []string) bool {
	var balancer map[string]interface{}
	for _, raw := range balancers {
		m, ok := raw.(map[string]interface{})
		if ok && m["tag"] == tag {
			balancer = m
			break
		}
	}
	if balancer == nil {
		return false
	}
	selectors := asStringSlice(balancer["selector"])
	if len(selectors) == 0 {
		return false
	}

	matched := 0
	for _, raw := range outbounds {
		ob, ok := raw.(map[string]interface{})
		if !ok {
			continue
		}
		obTag, _ := ob["tag"].(string)
		if obTag == "" {
			continue
		}
		for _, selector := range selectors {
			if selector == "" {
				return false
			}
			if strings.HasPrefix(obTag, selector) {
				if !contains(proxyTags, obTag) {
					return false
				}
				matched++
				break
			}
		}
	}
	return matched > 0
}

// proxyTagsWithoutDialerProxy drops chained outbounds from the fallback
// selector: their socket is dialed through another outbound, so picking them
// directly is not equivalent to picking a normal proxy node.
func proxyTagsWithoutDialerProxy(outbounds []interface{}, proxyTags []string) []string {
	kept := make([]string, 0, len(proxyTags))
	for _, tag := range proxyTags {
		if !outboundHasDialerProxy(outbounds, tag) {
			kept = append(kept, tag)
		}
	}
	return kept
}

func outboundHasDialerProxy(outbounds []interface{}, tag string) bool {
	for _, raw := range outbounds {
		ob, ok := raw.(map[string]interface{})
		if !ok || ob["tag"] != tag {
			continue
		}
		stream := asMap(ob["streamSettings"])
		sockopt := asMap(stream["sockopt"])
		proxy, _ := sockopt["dialerProxy"].(string)
		return proxy != ""
	}
	return false
}

// buildUserRules translates manual rules into the provider's outbound tags.
// Manual rules only know proxy/direct/block. For "proxy" we prefer the
// provider's own default target (tag "proxy", the only proxy outbound, or the
// first catch-all balancer/outbound) and generate a user balancer only as a
// last resort.
func buildUserRules(configured []structs.RoutingRule, globalGeoExt bool, outbounds, balancers []interface{}, proxyTags []string, defaultTarget proxyTarget, hasDefault bool) ([]interface{}, map[string]interface{}) {
	if len(configured) == 0 {
		return nil, nil
	}

	proxyOutbound := ""
	proxyBalancer := ""
	var fallbackTags []string
	switch {
	case contains(proxyTags, "proxy"):
		proxyOutbound = "proxy"
	case len(proxyTags) == 1:
		proxyOutbound = proxyTags[0]
	case hasDefault && defaultTarget.balancer:
		proxyBalancer = defaultTarget.tag
	case hasDefault:
		proxyOutbound = defaultTarget.tag
	default:
		// shortcut: fallback stays random over non-chained proxy outbounds;
		// upgrade when a provider without a catch-all shows the same symptom.
		proxyBalancer = uniqueBalancerTag(balancers, outbounds)
		fallbackTags = proxyTagsWithoutDialerProxy(outbounds, proxyTags)
		if len(fallbackTags) == 0 {
			fallbackTags = proxyTags
		}
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

	if proxyBalancer == "" || fallbackTags == nil {
		return rules, nil
	}
	return rules, map[string]interface{}{
		"tag":      proxyBalancer,
		"selector": fallbackTags,
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
