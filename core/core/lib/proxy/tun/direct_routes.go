package tunmode

import (
	"fmt"
	"net"
	"net/netip"
	"regexp"
	"strings"

	"whoisthat-core/lib/logger"
)

// Host routes for destinations excluded from the tunnel.
//
// Accepting a direct-rule destination in the firewall is not enough: without a
// mark (the pinned Xray does not apply sockopt.mark on freedom) the packet
// still follows the TUN's default route and comes back into Xray. Pinning the
// address to the physical gateway gives it the path the direct rule asked for.
//
// Ownership is the route protocol tag, not in-memory state: every arm flushes
// our own tag first, so a crash cannot leave a stale pin that is mistaken for
// the user's route, and teardown is one flush.

const directPinProto = 198

// validIfaceRe guards the interface name that is interpolated into a root
// shell script. It is broader than validTunNameRe because physical names may
// contain dots (vlan), colons and '@'.
var validIfaceRe = regexp.MustCompile(`^[A-Za-z0-9_.:@-]+$`)

// directRouteRun is a test seam.
var directRouteRun = runScriptWithSh

// physicalDefault returns the gateway and device of the default route that is
// not the TUN. Empty values mean there is nothing to pin to.
func physicalDefault(family string, tunName string) (string, string) {
	out, err := directRouteRun("ip " + family + " route show default")
	if err != nil {
		return "", ""
	}
	for _, line := range strings.Split(out, "\n") {
		fields := strings.Fields(line)
		var gw, dev string
		for i, f := range fields {
			if f == "via" && i+1 < len(fields) {
				gw = fields[i+1]
			}
			if f == "dev" && i+1 < len(fields) {
				dev = fields[i+1]
			}
		}
		if gw == "" || dev == "" || dev == tunName {
			continue
		}
		return gw, dev
	}
	return "", ""
}

// parseOnLink extracts the prefixes of `ip route show dev <dev> scope link`.
func parseOnLink(out string) []netip.Prefix {
	var prefixes []netip.Prefix
	for _, line := range strings.Split(out, "\n") {
		fields := strings.Fields(line)
		if len(fields) == 0 {
			continue
		}
		p, err := netip.ParsePrefix(fields[0])
		if err != nil {
			continue
		}
		prefixes = append(prefixes, p)
	}
	return prefixes
}

// coveredByOnLink reports whether pinning target would either be shadowed by
// an on-link route or replace one. Both directions matter: a /16 direct rule
// must not take a /24 LAN route with it.
func coveredByOnLink(target netip.Prefix, onlink []netip.Prefix) bool {
	for _, o := range onlink {
		if o.Addr().Is4() != target.Addr().Is4() {
			continue
		}
		if o.Bits() <= target.Bits() && o.Contains(target.Addr()) {
			return true
		}
		if target.Bits() <= o.Bits() && target.Contains(o.Addr()) {
			return true
		}
	}
	return false
}

// addDirectHostRoutes pins the resolved direct-rule targets to the physical
// gateway. Best-effort: a failed pin only costs the host its direct path, the
// accept rule above the drop still keeps it reachable (through the tunnel once,
// which is why a missing pin is logged rather than fatal).
//
// Skipped when tunName is empty: in proxy mode there is no TUN default route,
// so the physical default is already the only path.
func addDirectHostRoutes(v4, v6 []string, tunName string) error {
	if tunName == "" {
		return nil
	}
	for _, family := range []string{"-4", "-6"} {
		_, _ = directRouteRun(fmt.Sprintf("ip %s route flush proto %d", family, directPinProto))
	}

	var errs []string
	seen := map[string]struct{}{}
	for _, set := range []struct {
		family string
		flag   string
		nets   []string
	}{{"-4", "/32", v4}, {"-6", "/128", v6}} {
		gw, dev := physicalDefault(set.family, tunName)
		// Both values are interpolated into a root shell unquoted; `ip` output
		// is not a trust boundary, so validate before they get there.
		if gw == "" || dev == "" || net.ParseIP(gw) == nil || !validIfaceRe.MatchString(dev) {
			continue
		}
		onlinkOut, err := directRouteRun(fmt.Sprintf("ip %s route show dev %s scope link", set.family, dev))
		if err != nil {
			// Without the on-link view the guard cannot run. Skipping pinning
			// only costs the host its direct path; replacing blind could take
			// the LAN route with it.
			logger.Warnf("kill-switch: cannot read on-link routes on %s: %v", dev, err)
			continue
		}
		onlink := parseOnLink(onlinkOut)
		for _, n := range set.nets {
			cidr := n
			if !strings.Contains(n, "/") {
				cidr += set.flag
			}
			p, err := netip.ParsePrefix(cidr)
			if err != nil {
				continue
			}
			key := p.Masked().String()
			if _, dup := seen[key]; dup {
				continue
			}
			if coveredByOnLink(p.Masked(), onlink) {
				logger.Infof("kill-switch: %s is on-link via %s, not pinning it", key, dev)
				seen[key] = struct{}{}
				continue
			}
			// `exact`: a bare `route show <prefix>` also lists more specifics,
			// so a narrower user route would look like a route for this prefix.
			out, err := directRouteRun(fmt.Sprintf("ip %s route show exact %s", set.family, key))
			if err != nil {
				// Without the lookup the replace could take a user route with
				// it; skip instead of guessing.
				logger.Warnf("kill-switch: cannot inspect route %s: %v", key, err)
				continue
			}
			if strings.TrimSpace(out) != "" {
				// A route the user already had is theirs: it would have to be
				// deleted again on disarm, which loses their setup.
				logger.Infof("kill-switch: %s already has a route, leaving it in place", key)
				seen[key] = struct{}{}
				continue
			}
			if _, err := directRouteRun(fmt.Sprintf("ip %s route replace %s via %s dev %s proto %d", set.family, key, gw, dev, directPinProto)); err != nil {
				errs = append(errs, key)
				continue
			}
			seen[key] = struct{}{}
		}
	}
	if len(errs) > 0 {
		return fmt.Errorf("failed for %v", errs)
	}
	return nil
}

// removeDirectHostRoutes flushes only the routes this switch installed.
func removeDirectHostRoutes() {
	for _, family := range []string{"-4", "-6"} {
		_, _ = directRouteRun(fmt.Sprintf("ip %s route flush proto %d", family, directPinProto))
	}
}
