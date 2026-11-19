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

// tooWide reports whether an exception prefix is so broad that accepting it
// effectively switches the kill-switch off. Rules can come from the provider
// (Source == "happ"), so this is not a purely local decision.
//
// 0.0.0.0/0 and ::/0 are whole address families. IPv4-mapped prefixes count as
// v4 at their mapped depth: ::ffff:0:0/96 is the whole v4 space, and nft renders
// it as 0.0.0.0/0. Anything shorter than /8 (v4) or /16 (v6) counts as wide for
// the same reason. A prefix that does not parse is treated as wide — the caller
// cannot reason about it.
func tooWide(v string) bool {
	if !strings.Contains(v, "/") {
		return false
	}
	prefix, err := netip.ParsePrefix(v)
	if err != nil {
		return true
	}
	addr, bits := prefix.Addr(), prefix.Bits()
	if addr.Is4In6() && bits >= 96 {
		// A mapped /96+n is an IPv4 /n; anything shorter is still a v6 prefix.
		return bits-96 < 8
	}
	if addr.Is4() {
		return bits < 8
	}
	return bits < 16
}

// directTargetsFromRules extracts literal kill-switch exceptions. Matchers that
// cannot be turned into an address (geosite:, geoip:, regexp:, keyword:, ext:,
// wildcards) are skipped: the firewall can only accept concrete addresses.
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
		switch r.Type {
		case "domain":
			add(r.Domain)
		case "ip":
			add(r.IP)
		}
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
