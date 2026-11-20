package tunmode

import (
	"context"
	"net"
	"net/netip"
	"strings"
	"sync"
	"time"

	"whoisthat-core/lib/logger"
)

// Direct-rule exceptions for the kill-switch.
//
// The armed chain drops every locally-generated packet except the allowed
// classes. A destination the user excluded on purpose (`--outbound direct`)
// still needs to leave, but the pinned Xray's freedom outbound does not apply
// sockopt.mark, so those packets never match the mark-based accept. They are
// accepted by address instead, and pinned to the physical gateway in routes.go
// so they do not fall back into the TUN.
//
// Targets are resolved at arm time with the system resolver, which honours
// /etc/hosts, so the address accepted is the address the excluded host gets.

var (
	directSourceMu sync.RWMutex
	directSource   func() []string

	// lookupDirectHost is a test seam; it returns every address (v4 and v6)
	// the name resolves to.
	lookupDirectHost = func(ctx context.Context, host string) ([]net.IP, error) {
		addrs, err := net.DefaultResolver.LookupIPAddr(ctx, host)
		if err != nil {
			return nil, err
		}
		ips := make([]net.IP, 0, len(addrs))
		for _, a := range addrs {
			ips = append(ips, a.IP)
		}
		return ips, nil
	}
)

// maxDirectExceptions caps both the resolution work and the ruleset. The
// per-prefix floor judges each entry alone, so a provider-supplied list could
// otherwise carry many broad prefixes that together cover a large part of the
// address space.
const maxDirectExceptions = 64

// directResolutionBudget bounds the DNS work: resolveDirectExceptions runs
// while the arm holds the connection lock.
const directResolutionBudget = 15 * time.Second

// SetDirectTargetSource registers the provider of direct-rule targets (plain
// hostnames, IPs or CIDRs). It is called at every arm, so it must return the
// current rules.
func SetDirectTargetSource(fn func() []string) {
	directSourceMu.Lock()
	directSource = fn
	directSourceMu.Unlock()
}

// currentDirectExceptions resolves the registered source, if any.
func currentDirectExceptions() []string {
	directSourceMu.RLock()
	fn := directSource
	directSourceMu.RUnlock()
	if fn == nil {
		return nil
	}
	return resolveDirectExceptions(fn())
}

// resolveDirectExceptions turns direct-rule targets into IP/CIDR literals.
// A name that does not resolve is skipped with a warning: the switch stays
// fail-closed for it rather than failing to arm or spending the whole budget
// on one dead name.
func resolveDirectExceptions(targets []string) []string {
	ctx, cancel := context.WithTimeout(context.Background(), directResolutionBudget)
	defer cancel()

	var out []string
	seen := map[string]struct{}{}
	// Only look at the first 2x cap entries: a provider list of dead names
	// must not consume the whole resolution budget before the real rules.
	for i, raw := range targets {
		if i >= 2*maxDirectExceptions || len(out) >= maxDirectExceptions {
			break
		}
		raw = strings.TrimSpace(raw)
		if raw == "" {
			continue
		}
		var vals []string
		if ip := net.ParseIP(raw); ip != nil {
			vals = []string{ip.String()}
		} else if _, n, err := net.ParseCIDR(raw); err == nil {
			vals = []string{n.String()}
		} else {
			ips, err := lookupDirectHost(ctx, raw)
			if err != nil {
				logger.Warnf("kill-switch: cannot resolve direct target %q: %v", raw, err)
				continue
			}
			for _, ip := range ips {
				vals = append(vals, ip.String())
			}
		}
		for _, v := range vals {
			// Distinct names often share one CDN address; duplicates must
			// not fill the cap on their own.
			if _, dup := seen[v]; dup {
				continue
			}
			seen[v] = struct{}{}
			out = append(out, v)
		}
	}
	if len(out) > maxDirectExceptions {
		out = out[:maxDirectExceptions]
	}
	return out
}

// fixedAllowlistPrefixes is the set the kill-switch always accepts (loopback,
// private, link-local, multicast). A direct rule covering one of these needs no
// exception or pin: the accept already exists, and a pin would only shadow a
// LAN or VPN route.
var fixedAllowlistPrefixes = func() []netip.Prefix {
	var out []netip.Prefix
	for _, s := range append(append([]string{}, killSwitchV4...), killSwitchV6...) {
		if p, err := netip.ParsePrefix(s); err == nil {
			out = append(out, p)
		}
	}
	return out
}()

func insideFixedAllowlist(p netip.Prefix) bool {
	for _, a := range fixedAllowlistPrefixes {
		if a.Addr().Is4() != p.Addr().Is4() {
			continue
		}
		if a.Bits() <= p.Bits() && a.Contains(p.Addr()) {
			return true
		}
	}
	return false
}

// splitNetsValid splits IP/CIDR literals into de-duplicated v4 and v6 lists.
// Like splitIPsValid it is an injection boundary: only values that parse are
// ever embedded into the root script, and they are emitted in canonical form.
// IPv4-mapped prefixes are normalized to their v4 form before they reach the
// nft/iptables rule, where a v6 spelling of a v4 address would not match.
func splitNetsValid(items []string) (v4, v6 []string) {
	seen := make(map[string]struct{}, len(items))
	for _, raw := range items {
		raw = strings.TrimSpace(raw)
		var key string
		var p netip.Prefix
		var is4, wasNet bool
		if ip := net.ParseIP(raw); ip != nil {
			addr, ok := netip.AddrFromSlice(ip)
			if !ok {
				continue
			}
			addr = addr.Unmap()
			bits := 128
			if addr.Is4() {
				bits = 32
			}
			p = netip.PrefixFrom(addr, bits)
			key, is4 = addr.String(), addr.Is4()
		} else if parsed, err := netip.ParsePrefix(raw); err == nil {
			wasNet = true
			if parsed.Addr().Is4In6() {
				if parsed.Bits() < 96 {
					logger.Warnf("kill-switch: direct exception %q is too broad, ignoring", raw)
					continue
				}
				parsed = netip.PrefixFrom(parsed.Addr().Unmap(), parsed.Bits()-96)
			}
			p = parsed.Masked()
			key, is4 = p.String(), p.Addr().Is4()
		} else {
			if raw != "" {
				logger.Warnf("kill-switch: ignoring invalid direct exception %q", raw)
			}
			continue
		}
		if insideFixedAllowlist(p) {
			continue
		}
		// Re-checked here as well as in the extractor: this is the boundary
		// that turns a string into a firewall rule.
		if wasNet && tooWidePrefix(key, is4) {
			logger.Warnf("kill-switch: direct exception %q is too broad, ignoring", key)
			continue
		}
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		if is4 {
			v4 = append(v4, key)
		} else {
			v6 = append(v6, key)
		}
	}
	if total := len(v4) + len(v6); total > maxDirectExceptions {
		logger.Warnf("kill-switch: %d direct exceptions, keeping the first %d", total, maxDirectExceptions)
		if len(v4) >= maxDirectExceptions {
			v4, v6 = v4[:maxDirectExceptions], nil
		} else {
			v6 = v6[:maxDirectExceptions-len(v4)]
		}
	}
	return v4, v6
}

// mappedV4Prefix is the IPv4-mapped range; see tooWide.
var mappedV4Prefix = netip.MustParsePrefix("::ffff:0:0/96")

// tooWidePrefix is the injection-boundary twin of mainproxy.tooWide: a v4
// prefix shorter than /8 (or a v6 one shorter than /16) accepts essentially a
// whole address family, so the switch would stop dropping anything. A v6
// prefix that overlaps the IPv4-mapped range without staying inside it opens
// the whole v4 space through the v6 family.
func tooWidePrefix(cidr string, is4 bool) bool {
	p, err := netip.ParsePrefix(cidr)
	if err != nil {
		return true
	}
	bits := p.Bits()
	if p.Addr().Is4In6() {
		if bits < 96 {
			return true
		}
		return bits-96 < 8
	}
	if bits < 96 && mappedV4Prefix.Overlaps(p) {
		return true
	}
	if is4 {
		return bits < 8
	}
	return bits < 16
}
