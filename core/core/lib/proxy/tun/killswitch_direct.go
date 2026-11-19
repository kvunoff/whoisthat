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
// The armed ruleset only lets through traffic leaving via the tun, so a
// destination the user excluded on purpose (`--outbound direct` rules, a
// /etc/hosts pin plus a host route via the LAN device) would be dropped. The
// route, not the firewall, decides the path for those, so a bare accept per
// address is enough. Targets are resolved at arm time (and therefore again on
// every re-arm: connect, reload, rotation) with the system resolver, which
// honours /etc/hosts, so the address that is accepted is the address the
// excluded host really gets.

// Debugging traps (both look like a broken datapath while routing is fine):
//   - A stray metric-0 `default via <lan>` (e.g. a manual restore) shadows the
//     tun's metric-1 default. Restore with `metric 600 proto dhcp` instead.
//   - A dead profile is indistinguishable from a broken datapath. Test the link
//     itself (`curl --socks5 127.0.0.1:<port>`) before blaming routing.

var (
	directSourceMu sync.RWMutex
	directSource   func() []string

	// lookupDirectHost is a test seam; it returns every address (v4 and v6)
	// the name resolves to.
	lookupDirectHost = func(host string) ([]net.IP, error) {
		ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
		defer cancel()
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

// SetDirectTargetSource registers the provider of direct-rule targets (plain
// hostnames, IPs or CIDRs). It is called at every arm, so it must return the
// current rules.
func SetDirectTargetSource(fn func() []string) {
	directSourceMu.Lock()
	directSource = fn
	directSourceMu.Unlock()
}

// resolveDirectExceptions turns direct-rule targets into IP/CIDR literals.
// A name that does not resolve is skipped with a warning: the switch stays
// fail-closed for it rather than failing to arm.
//
// Names are resolved in waves of eight: this runs while the switch is being
// armed, under both ConnectionMutex and p.mu, so dead names stall every command.
// Resolution stops once the cap is filled, so a list of dead names up front does
// not use up the slots valid entries later on need, and it gives up after the
// first 2×cap entries (16 waves, ~48s with a fully dead list).
func resolveDirectExceptions(targets []string) []string {
	const wave = 8
	var out []string
	seen := map[string]struct{}{}
	for start := 0; start < len(targets) && start < 2*maxDirectExceptions && len(out) < maxDirectExceptions; start += wave {
		end := min(start+wave, len(targets))
		batch := targets[start:end]
		resolved := make([][]string, len(batch))
		var wg sync.WaitGroup
		for i, t := range batch {
			t = strings.TrimSpace(t)
			if t == "" {
				continue
			}
			if ip := net.ParseIP(t); ip != nil {
				resolved[i] = []string{ip.String()}
				continue
			}
			if _, n, err := net.ParseCIDR(t); err == nil {
				resolved[i] = []string{n.String()}
				continue
			}
			wg.Add(1)
			go func(i int, host string) {
				defer wg.Done()
				ips, err := lookupDirectHost(host)
				if err != nil {
					logger.Warnf("kill-switch: cannot resolve direct target %q: %v", host, err)
					return
				}
				vals := make([]string, 0, len(ips))
				for _, ip := range ips {
					vals = append(vals, ip.String())
				}
				resolved[i] = vals
			}(i, t)
		}
		wg.Wait()
		for _, set := range resolved {
			for _, v := range set {
				// Dedupe here too: distinct names often share one CDN address,
				// and duplicate entries must not fill the cap on their own.
				if _, dup := seen[v]; dup {
					continue
				}
				seen[v] = struct{}{}
				out = append(out, v)
			}
		}
	}
	if len(out) > maxDirectExceptions {
		logger.Warnf("kill-switch: %d exceptions resolved, keeping the first %d", len(out), maxDirectExceptions)
		out = out[:maxDirectExceptions]
	}
	return out
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

// maxDirectExceptions caps the total ruleset. tooWidePrefix judges each prefix
// on its own, so a provider-supplied list can carry many broad prefixes that
// together cover a large part of the address space; this bounds how far that
// can go. It also bounds the resolution work done while the arm holds the locks.
//
// Two cuts: resolution stops at the cap and considers only the first 2×cap
// entries, and the resolved set is cut again at the split, where v4 is kept and
// v6 is trimmed.
const maxDirectExceptions = 64

// splitNetsValid splits IP/CIDR literals into de-duplicated v4 and v6 lists.
// Like splitIPsValid it is an injection boundary: only values that parse are
// ever embedded into the root script, and they are emitted in canonical form.
func splitNetsValid(items []string) (v4, v6 []string) {
	seen := make(map[string]struct{}, len(items))
	for _, raw := range items {
		raw = strings.TrimSpace(raw)
		key := ""
		is4 := false
		wasNet := false
		if ip := net.ParseIP(raw); ip != nil {
			key, is4 = ip.String(), ip.To4() != nil
		} else if _, n, err := net.ParseCIDR(raw); err == nil {
			key, is4, wasNet = n.String(), n.IP.To4() != nil, true
		} else {
			if raw != "" {
				logger.Warnf("kill-switch: ignoring invalid direct exception %q", raw)
			}
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

// tooWidePrefix is the injection-boundary twin of mainproxy.tooWide: a v4
// prefix shorter than /8 (or a v6 one shorter than /16) accepts essentially a
// whole address family, so the switch would stop dropping anything.
func tooWidePrefix(cidr string, is4 bool) bool {
	p, err := netip.ParsePrefix(cidr)
	if err != nil {
		return true
	}
	if is4 {
		return p.Bits() < 8
	}
	return p.Bits() < 16
}
