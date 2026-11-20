package tunmode

import (
	"net/netip"
	"strings"
	"testing"
)

func TestCoveredByOnLink(t *testing.T) {
	onlink := []netip.Prefix{netip.MustParsePrefix("192.168.3.0/24")}
	cases := []struct {
		target string
		want   bool
	}{
		{"192.168.3.10/32", true}, // inside an on-link route
		{"192.168.0.0/16", true},  // would replace the on-link route
		{"192.168.3.0/24", true},  // same route
		{"198.51.100.20/32", false},
		{"2001:db8::1/128", false}, // different family
	}
	for _, c := range cases {
		if got := coveredByOnLink(netip.MustParsePrefix(c.target), onlink); got != c.want {
			t.Errorf("coveredByOnLink(%s) = %v, want %v", c.target, got, c.want)
		}
	}
}

func TestParseOnLink(t *testing.T) {
	out := `192.168.3.0/24 proto kernel scope link src 192.168.3.3
default dev wlan0 scope link
fe80::/64 proto kernel metric 256 pref medium
`
	got := parseOnLink(out)
	if len(got) != 2 || got[0].String() != "192.168.3.0/24" || got[1].String() != "fe80::/64" {
		t.Fatalf("got %v", got)
	}
}

func fakeDirectRoutes(t *testing.T) *[]string {
	t.Helper()
	old := directRouteRun
	t.Cleanup(func() { directRouteRun = old })
	var cmds []string
	directRouteRun = func(cmd string) (string, error) {
		cmds = append(cmds, cmd)
		v6 := strings.HasPrefix(cmd, "ip -6")
		switch {
		case strings.Contains(cmd, "route show default"):
			if v6 {
				return "default via fe80::1 dev wlan0 proto ra metric 600\n", nil
			}
			return "default via 192.168.3.1 dev wlan0 proto dhcp metric 600\n", nil
		case strings.Contains(cmd, "route show dev wlan0 scope link"):
			if v6 {
				return "fe80::/64 proto kernel metric 256 pref medium\n", nil
			}
			return "192.168.3.0/24 proto kernel scope link src 192.168.3.3\n", nil
		case strings.Contains(cmd, "route show exact 198.51.100.21"):
			return "198.51.100.21 via 192.168.3.1 dev wlan0\n", nil
		}
		return "", nil
	}
	return &cmds
}

func TestAddDirectHostRoutesPinsOnlySafeTargets(t *testing.T) {
	cmds := fakeDirectRoutes(t)
	err := addDirectHostRoutes(
		[]string{"198.51.100.20", "192.168.3.10", "198.51.100.21"},
		[]string{"2001:db8::20"},
		"whoisthattun",
	)
	if err != nil {
		t.Fatalf("addDirectHostRoutes: %v", err)
	}
	joined := strings.Join(*cmds, "\n")

	for _, want := range []string{
		"ip -4 route flush proto 198",
		"ip -6 route flush proto 198",
		"ip -4 route replace 198.51.100.20/32 via 192.168.3.1 dev wlan0 proto 198",
		"ip -6 route replace 2001:db8::20/128 via fe80::1 dev wlan0 proto 198",
	} {
		if !strings.Contains(joined, want) {
			t.Errorf("missing command %q in:\n%s", want, joined)
		}
	}
	for _, dontWant := range []string{
		"route replace 192.168.3.10",
		"route replace 198.51.100.21",
	} {
		if strings.Contains(joined, dontWant) {
			t.Errorf("pinned %q:\n%s", dontWant, joined)
		}
	}
}

func TestAddDirectHostRoutesSkipsProxyMode(t *testing.T) {
	cmds := fakeDirectRoutes(t)
	if err := addDirectHostRoutes([]string{"198.51.100.20"}, nil, ""); err != nil {
		t.Fatal(err)
	}
	if len(*cmds) != 0 {
		t.Fatalf("proxy mode touched routes: %v", *cmds)
	}
}

func TestRemoveDirectHostRoutesFlushesOnlyOwnProto(t *testing.T) {
	cmds := fakeDirectRoutes(t)
	removeDirectHostRoutes()
	joined := strings.Join(*cmds, "\n")
	if joined != "ip -4 route flush proto 198\nip -6 route flush proto 198" {
		t.Fatalf("unexpected teardown: %q", joined)
	}
}
