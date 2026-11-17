package mainproxy

import (
	"net/netip"
	"strings"
	"sync/atomic"

	"whoisthat-core/structs"
)

// directTargetStore holds the destinations of the active `--outbound direct`
// rules for the kill-switch. It is atomic, not under p.mu, because the arm
// handler runs while p.mu is held and reads it back.
type directTargetStore struct{ v atomic.Value }

func (s *directTargetStore) set(t []string) { s.v.Store(t) }
func (s *directTargetStore) get() []string {
	t, _ := s.v.Load().([]string)
	return t
}

// DirectTargets returns the plain hostnames, IPs and CIDRs of the enabled
// domain/ip rules routed to "direct" in the currently applied routing. The
// kill-switch resolves and accepts them (see tunmode.SetDirectTargetSource).
func (p *ProxyManager) DirectTargets() []string { return p.directTargets.get() }

// directTargetsFromRules extracts literal kill-switch exceptions. Matchers that
// cannot be turned into an address (geosite:, geoip:, regexp:, keyword:, ext:,
// wildcards) are skipped: the firewall can only accept concrete addresses.
// tooWide reports whether an exception prefix is so broad that accepting it
// effectively switches the kill-switch off. Rules can come from the provider
// (Source == "happ"), so this is not a purely local decision.
//
// 0.0.0.0/0 and ::/0 are whole address families; ::ffff:0:0/96 is the mapped
// range, which nft renders as 0.0.0.0/0. Anything shorter than /8 (v4) or /16
// (v6) is treated as wide for the same reason.
func tooWide(v string) bool {
	if !strings.Contains(v, "/") {
		return false
	}
	// The mapped range: nft renders it as 0.0.0.0/0 and netip does not treat it
	// as Is4In6 as a prefix.
	if strings.EqualFold(v, "::ffff:0:0/96") {
		return true
	}
	prefix, err := netip.ParsePrefix(v)
	if err != nil {
		return false
	}
	bits := prefix.Bits()
	if prefix.Addr().Is4() || prefix.Addr().Is4In6() {
		return bits < 8
	}
	return bits < 16
}

func directTargetsFromRules(rules []structs.RoutingRule) []string {
	seen := map[string]struct{}{}
	var out []string
	add := func(raw string) {
		for _, v := range splitAndTrim(raw) {
			v = strings.TrimPrefix(strings.TrimPrefix(v, "full:"), "domain:")
			if v == "" || strings.ContainsAny(v, "*?") || hasMatcherPrefix(v) {
				continue
			}
			if tooWide(v) {
				continue
			}
			key := strings.ToLower(v)
			if _, ok := seen[key]; ok {
				continue
			}
			seen[key] = struct{}{}
			out = append(out, v)
		}
	}
	for _, r := range rules {
		if !r.Enabled || r.OutboundTag != "direct" {
			continue
		}
		// Match on the populated field rather than on the type name: the
		// stored rules are all type "field", with Domain or IP set, so
		// switching on "domain"/"ip" alone skipped every manual rule.
		add(r.IP)
		add(r.Domain)
	}
	return out
}

func hasMatcherPrefix(v string) bool {
	for _, p := range []string{"geosite:", "geoip:", "regexp:", "keyword:", "ext:", "ext-domain:", "ext-ip:"} {
		if strings.HasPrefix(v, p) {
			return true
		}
	}
	return false
}
