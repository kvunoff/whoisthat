package tunmode

import (
	"net/netip"
	"testing"
)

// A direct-rule prefix broader than the interface's own on-link route must not
// be `ip route replace`d: doing so replaces the LAN route for the whole prefix
// with a /32-style host route via the gateway and cuts the subnet off.
func TestCoveredByOnLink(t *testing.T) {
	onlink := []netip.Prefix{
		netip.MustParsePrefix("192.168.1.0/24"),
		netip.MustParsePrefix("2001:db8:1::/64"),
	}
	cases := []struct {
		cidr string
		want bool
	}{
		{"192.168.0.0/16", true},    // broader than the on-link /24
		{"192.168.1.0/24", true},    // exactly the on-link route
		{"192.168.1.7/32", true},    // inside it
		{"198.51.100.20/32", false}, // unrelated host
		{"10.0.0.0/8", false},       // broader, but not our subnet
		{"2001:db8:1::/64", true},
		{"2001:db8:2::/64", false},
	}
	for _, c := range cases {
		if got := coveredByOnLink(c.cidr, onlink); got != c.want {
			t.Errorf("coveredByOnLink(%q) = %v, want %v", c.cidr, got, c.want)
		}
	}
}

func TestParseOnLink(t *testing.T) {
	out := "192.168.1.0/24 proto kernel scope link src 192.168.1.10\ndefault via 192.168.1.1 dev wlan0"
	got := parseOnLink(out)
	if len(got) != 1 || got[0].String() != "192.168.1.0/24" {
		t.Fatalf("parseOnLink = %v, want only 192.168.1.0/24", got)
	}
}
