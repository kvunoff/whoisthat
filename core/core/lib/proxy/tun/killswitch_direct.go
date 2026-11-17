package tunmode

import (
	"context"
	"net"
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
func resolveDirectExceptions(targets []string) []string {
	var out []string
	for _, t := range targets {
		t = strings.TrimSpace(t)
		if t == "" {
			continue
		}
		if ip := net.ParseIP(t); ip != nil {
			out = append(out, ip.String())
			continue
		}
		if _, n, err := net.ParseCIDR(t); err == nil {
			out = append(out, n.String())
			continue
		}
		ips, err := lookupDirectHost(t)
		if err != nil {
			logger.Warnf("kill-switch: cannot resolve direct target %q: %v", t, err)
			continue
		}
		for _, ip := range ips {
			out = append(out, ip.String())
		}
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

// splitNetsValid splits IP/CIDR literals into de-duplicated v4 and v6 lists.
// Like splitIPsValid it is an injection boundary: only values that parse are
// ever embedded into the root script, and they are emitted in canonical form.
func splitNetsValid(items []string) (v4, v6 []string) {
	seen := make(map[string]struct{}, len(items))
	for _, raw := range items {
		raw = strings.TrimSpace(raw)
		key := ""
		is4 := false
		if ip := net.ParseIP(raw); ip != nil {
			key, is4 = ip.String(), ip.To4() != nil
		} else if _, n, err := net.ParseCIDR(raw); err == nil {
			key, is4 = n.String(), n.IP.To4() != nil
		} else {
			if raw != "" {
				logger.Warnf("kill-switch: ignoring invalid direct exception %q", raw)
			}
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
	return v4, v6
}
