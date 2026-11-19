package tunmode

import (
	"errors"
	"strings"
	"testing"
)

// fakeIPs answers the `ip` invocations the pinning code makes and records the
// mutating ones. `exact` answers `ip route show exact <prefix>` (what a user
// route for that very prefix would print); `inside` answers any other
// `route show <prefix>` (more specific routes listed under it).
func fakeIPs(t *testing.T, onlink string, exact, inside map[string]string) (*[]string, func()) {
	t.Helper()
	old := runScript
	var calls []string
	runScript = func(script string) (string, error) {
		calls = append(calls, script)
		switch {
		case strings.Contains(script, "route show default"):
			return "default via 192.168.1.1 dev wlan0 proto dhcp metric 600", nil
		case strings.Contains(script, "scope link"):
			return onlink, nil
		case strings.Contains(script, "route show exact"):
			for prefix, out := range exact {
				if strings.Contains(script, prefix) {
					return out, nil
				}
			}
			return "", nil
		case strings.Contains(script, "route show"):
			for prefix, out := range inside {
				if strings.Contains(script, prefix) {
					return out, nil
				}
			}
			return "", nil
		}
		return "", nil
	}
	return &calls, func() { runScript = old }
}

func countPrefix(calls []string, verb, prefix string) int {
	n := 0
	for _, c := range calls {
		if strings.Contains(c, verb) && strings.Contains(c, prefix) && !strings.Contains(c, "route show") {
			n++
		}
	}
	return n
}

func countFlush(calls []string) int {
	n := 0
	for _, c := range calls {
		if strings.Contains(c, "route flush proto "+pinProto) {
			n++
		}
	}
	return n
}

func TestAddDirectHostRoutesPinsBareIP(t *testing.T) {
	calls, restore := fakeIPs(t, "192.168.1.0/24 proto kernel scope link src 192.168.1.10", nil, nil)
	defer restore()

	if err := addDirectHostRoutes([]string{"198.51.100.20"}, nil, "tun0"); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	// Ownership is carried by the tag, so the arm first clears its own pins —
	// including any that a crash left behind.
	if n := countFlush(*calls); n != 2 {
		t.Fatalf("flush calls = %d, want 2 (v4 + v6); calls=%v", n, *calls)
	}
	if n := countPrefix(*calls, "replace", "198.51.100.20/32"); n != 1 {
		t.Fatalf("replace calls for the bare IP = %d, want 1; calls=%v", n, *calls)
	}
	tagged := false
	for _, c := range *calls {
		if strings.Contains(c, "replace") && strings.Contains(c, "proto "+pinProto) {
			tagged = true
		}
	}
	if !tagged {
		t.Fatalf("pin was not tagged: %v", *calls)
	}
}

func TestAddDirectHostRoutesSkipsOnLinkAndUserRoute(t *testing.T) {
	calls, restore := fakeIPs(t, "192.168.1.0/24 proto kernel scope link src 192.168.1.10",
		map[string]string{"203.0.113.7/32": "203.0.113.7 via 192.168.1.1 dev wlan0"}, nil)
	defer restore()

	if err := addDirectHostRoutes([]string{"192.168.0.0/16", "203.0.113.7"}, nil, "tun0"); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if n := countPrefix(*calls, "replace", "192.168.0.0/16"); n != 0 {
		t.Fatalf("on-link prefix was replaced %d times", n)
	}
	if n := countPrefix(*calls, "replace", "203.0.113.7/32"); n != 0 {
		t.Fatalf("user route was replaced %d times", n)
	}
	// The user route carries no tag, so the next flush leaves it standing.
	if strings.Contains(strings.Join(*calls, "\n"), "replace 203.0.113.7/32") {
		t.Fatalf("user route was re-tagged as ours: %v", *calls)
	}
}

// `route show <prefix>` also lists more specifics; a narrower route under the
// prefix must not make the prefix itself look taken.
func TestAddDirectHostRoutesUsesExactLookup(t *testing.T) {
	calls, restore := fakeIPs(t, "192.168.1.0/24 proto kernel", nil,
		map[string]string{"198.18.0.0/16": "198.18.5.0/24 via 192.168.1.1 dev wlan0"})
	defer restore()

	if err := addDirectHostRoutes([]string{"198.18.0.0/16"}, nil, "tun0"); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	exact := false
	for _, c := range *calls {
		if strings.Contains(c, "route show exact 198.18.0.0/16") {
			exact = true
		}
	}
	if !exact {
		t.Fatalf("no exact lookup issued: %v", *calls)
	}
	// Real `show exact` is empty when only a /24 under the /16 exists, so the
	// /16 must still be pinned — a non-exact implementation would skip it.
	if n := countPrefix(*calls, "replace", "198.18.0.0/16"); n != 1 {
		t.Fatalf("prefix with only a more-specific route was not pinned")
	}
}

func TestAddDirectHostRoutesIPv6(t *testing.T) {
	old := runScript
	defer func() { runScript = old }()
	var calls []string
	runScript = func(script string) (string, error) {
		calls = append(calls, script)
		switch {
		case strings.Contains(script, "-6 route show default"):
			return "default via fe80::1 dev wlan0 proto ra metric 600", nil
		case strings.Contains(script, "scope link"):
			return "2001:db8:1::/64 proto kernel metric 256", nil
		}
		return "", nil
	}

	if err := addDirectHostRoutes(nil, []string{"2001:db8:2::5"}, "tun0"); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if n := countPrefix(calls, "replace", "2001:db8:2::5/128"); n != 1 {
		t.Fatalf("v6 pin calls = %d, want 1; calls=%v", n, calls)
	}
}

func TestRemoveDirectHostRoutesFlushesOurTag(t *testing.T) {
	calls, restore := fakeIPs(t, "192.168.1.0/24 proto kernel", nil, nil)
	defer restore()

	removeDirectHostRoutes()
	if n := countFlush(*calls); n != 2 {
		t.Fatalf("flush calls = %d, want 2 (v4 + v6); calls=%v", n, *calls)
	}
}

// Pins run before the firewall step; a failed arm must not leave them behind,
// since nothing else clears them until the next disarm.
func TestSetupRollsBackPinsOnFirewallFailure(t *testing.T) {
	oldProbe, oldNft, oldArmed := probeFirewall, setupKillSwitchBlockNftables, killSwitchArmedProbe
	defer func() { probeFirewall, setupKillSwitchBlockNftables, killSwitchArmedProbe = oldProbe, oldNft, oldArmed }()
	probeFirewall = func() firewallBackend { return firewallNftables }
	setupKillSwitchBlockNftables = func(ips []string, tunName string, dns []string, exc ...string) error {
		return errors.New("nft exploded")
	}
	killSwitchArmedProbe = func() (bool, error) { return false, nil }
	SetDirectTargetSource(func() []string { return []string{"198.51.100.20"} })
	defer SetDirectTargetSource(nil)

	calls, restore := fakeIPs(t, "192.168.1.0/24 proto kernel scope link", nil, nil)
	defer restore()

	if err := SetupKillSwitchBlock([]string{"203.0.113.9"}, "tun0", []string{"1.1.1.1"}); err == nil {
		t.Fatal("want the setup error back")
	}
	if n := countPrefix(*calls, "replace", "198.51.100.20/32"); n != 1 {
		t.Fatalf("pin calls = %d, want 1", n)
	}
	// The arm flushes once per family, then the pin, then the rollback flushes.
	lastReplace := -1
	var flushes []int
	for i, c := range *calls {
		switch {
		case strings.Contains(c, "replace"):
			lastReplace = i
		case strings.Contains(c, "route flush proto "+pinProto):
			flushes = append(flushes, i)
		}
	}
	if len(flushes) != 4 {
		t.Fatalf("flush calls = %d, want 4; calls=%v", len(flushes), *calls)
	}
	for _, i := range flushes[2:] {
		if i < lastReplace {
			t.Fatalf("rollback flush at %d ran before the pin at %d", i, lastReplace)
		}
	}
}

// A failed re-arm leaves the previous ruleset in force (the batch is a single
// transaction), so the fresh pins must survive with it.
func TestFailedRearmKeepsPins(t *testing.T) {
	oldProbe, oldNft, oldArmed := probeFirewall, setupKillSwitchBlockNftables, killSwitchArmedProbe
	defer func() { probeFirewall, setupKillSwitchBlockNftables, killSwitchArmedProbe = oldProbe, oldNft, oldArmed }()
	probeFirewall = func() firewallBackend { return firewallNftables }
	setupKillSwitchBlockNftables = func([]string, string, []string, ...string) error { return errors.New("nft exploded") }
	killSwitchArmedProbe = func() (bool, error) { return true, nil }
	SetDirectTargetSource(func() []string { return []string{"198.51.100.20"} })
	defer SetDirectTargetSource(nil)

	calls, restore := fakeIPs(t, "192.168.1.0/24 proto kernel scope link", nil, nil)
	defer restore()

	if err := SetupKillSwitchBlock([]string{"203.0.113.9"}, "tun0", nil); err == nil {
		t.Fatal("want the setup error back")
	}
	// Only the arm's own flush; no rollback flush.
	if n := countFlush(*calls); n != 2 {
		t.Fatalf("flush calls = %d, want 2 (arm only, no rollback); calls=%v", n, *calls)
	}
}

// A failed armed-probe must be read as "maybe armed": the old ruleset may still
// be there, so the pins have to survive the failed arm.
func TestFailedRearmProbeErrorKeepsPins(t *testing.T) {
	oldProbe, oldNft, oldArmed := probeFirewall, setupKillSwitchBlockNftables, killSwitchArmedProbe
	defer func() { probeFirewall, setupKillSwitchBlockNftables, killSwitchArmedProbe = oldProbe, oldNft, oldArmed }()
	probeFirewall = func() firewallBackend { return firewallNftables }
	setupKillSwitchBlockNftables = func([]string, string, []string, ...string) error { return errors.New("nft exploded") }
	killSwitchArmedProbe = func() (bool, error) { return false, errors.New("probe failed") }
	SetDirectTargetSource(func() []string { return []string{"198.51.100.20"} })
	defer SetDirectTargetSource(nil)

	calls, restore := fakeIPs(t, "192.168.1.0/24 proto kernel scope link", nil, nil)
	defer restore()

	_ = SetupKillSwitchBlock([]string{"203.0.113.9"}, "tun0", nil)
	if n := countFlush(*calls); n != 2 {
		t.Fatalf("flush calls = %d, want 2 (no rollback on a probe error); calls=%v", n, *calls)
	}
}
