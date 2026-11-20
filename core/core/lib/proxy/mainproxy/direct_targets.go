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
// rules routed to "direct" in the currently applied routing. The kill-switch
// resolves and accepts them (see tunmode.SetDirectTargetSource).
func (p *ProxyManager) DirectTargets() []string { return p.directTargets.get() }

// mappedV4 is the IPv4-mapped IPv6 range. A prefix that overlaps it without
// staying inside it accepts the whole v4 space through the v6 family.
var mappedV4 = netip.MustParsePrefix("::ffff:0:0/96")

// tooWide reports whether an exception prefix is so broad that accepting it
// effectively switches the kill-switch off. Rules can come from the provider
// (Source == "happ"), so this is not a purely local decision.
//
// On the pinned Xray the freedom outbound does not apply sockopt.mark, so a
// direct rule's traffic is not covered by the mark-based accept and needs its
// own exception. Anything shorter than /8 (v4) or /16 (v6) is too broad to be
// a meaningful exception. IPv4-mapped prefixes are judged at their mapped
// depth: ::ffff:0:0/96 is the whole v4 space.
func tooWide(v string) bool {
	if !strings.Contains(v, "/") {
		return false
	}
	prefix, err := netip.ParsePrefix(v)
	if err != nil {
		return true
	}
	bits := prefix.Bits()
	if prefix.Addr().Is4In6() {
		if bits < 96 {
			return true
		}
		return bits-96 < 8
	}
	if bits < 96 && mappedV4.Overlaps(prefix) {
		return true
	}
	if prefix.Addr().Is4() {
		return bits < 8
	}
	return bits < 16
}

// directTargetsFromRules extracts literal kill-switch exceptions. Matchers that
// cannot be turned into an address (geosite:, geoip:, regexp:, keyword:, ext:,
// wildcards) are skipped: the firewall can only accept concrete addresses.
//
// The rule type is not consulted: every writer stores Type "field" and puts
// the match in Domain or IP, so keying off the type string silently produced
// an empty exception set.
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
