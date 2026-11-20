package tunmode

import (
	"context"
	"fmt"
	"net"
	"strings"
	"testing"
)

func TestKillSwitchScriptsAcceptDirectRuleTargets(t *testing.T) {
	const (
		v4 = "198.51.100.20"
		v6 = "2001:db8::20"
	)
	nft := buildKillSwitchNftScript([]string{"192.0.2.10"}, "tun0", nil, v4, v6, "203.0.113.0/24")
	ipt := buildKillSwitchIptablesScript([]string{"192.0.2.10"}, "tun0", nil, v4, v6, "203.0.113.0/24")

	for _, want := range []string{
		"ip daddr 198.51.100.20 accept",
		"ip6 daddr 2001:db8::20 accept",
		"ip daddr 203.0.113.0/24 accept",
	} {
		if !strings.Contains(nft, want) {
			t.Errorf("nft missing %q", want)
		}
	}
	for _, want := range []string{
		"iptables -A WHOISTHAT_KS -d 198.51.100.20 -j ACCEPT",
		"ip6tables -A WHOISTHAT_KS -d 2001:db8::20 -j ACCEPT",
	} {
		if !strings.Contains(ipt, want) {
			t.Errorf("iptables missing %q", want)
		}
	}

	// Unrelated public addresses stay dropped, and the accept is above the drop.
	if strings.Contains(nft, "198.51.100.21") {
		t.Errorf("unrelated address accepted:\n%s", nft)
	}
	accept := strings.Index(nft, "ip daddr 198.51.100.20 accept")
	drop := strings.Index(nft, "add rule inet whoisthat_ks output drop")
	if accept < 0 || drop < 0 || accept > drop {
		t.Errorf("direct accept must precede the drop:\n%s", nft)
	}
}

func TestKillSwitchDirectExceptionsRejectInjection(t *testing.T) {
	script := buildKillSwitchNftScript(nil, "", nil, "1.2.3.4; rm -rf /", "198.51.100.9")
	if strings.Contains(script, "rm -rf") || !strings.Contains(script, "ip daddr 198.51.100.9 accept") {
		t.Fatalf("injection boundary broken:\n%s", script)
	}
}

func TestResolveDirectExceptionsSkipsUnresolvableAndDedupes(t *testing.T) {
	old := lookupDirectHost
	lookupDirectHost = func(_ context.Context, host string) ([]net.IP, error) {
		if host != "api.example.test" {
			return nil, fmt.Errorf("nxdomain")
		}
		return []net.IP{net.ParseIP("198.51.100.20"), net.ParseIP("2001:db8::20")}, nil
	}
	t.Cleanup(func() { lookupDirectHost = old })

	got := resolveDirectExceptions([]string{
		"api.example.test", "198.51.100.20", "203.0.113.0/24", "gone.example.test",
	})
	want := []string{"198.51.100.20", "2001:db8::20", "203.0.113.0/24"}
	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Fatalf("got %v want %v", got, want)
	}
}

func TestDirectTargetSourceIsReadAtArmTime(t *testing.T) {
	old := lookupDirectHost
	lookupDirectHost = func(context.Context, string) ([]net.IP, error) {
		return []net.IP{net.ParseIP("198.51.100.20")}, nil
	}
	t.Cleanup(func() { lookupDirectHost = old })
	SetDirectTargetSource(func() []string { return []string{"api.example.test"} })
	t.Cleanup(func() { SetDirectTargetSource(nil) })

	if got := currentDirectExceptions(); len(got) != 1 || got[0] != "198.51.100.20" {
		t.Fatalf("got %v", got)
	}
}

func TestSplitNetsValidRejectsBroadAndInvalid(t *testing.T) {
	v4, v6 := splitNetsValid([]string{
		"0.0.0.0/0", "10.0.0.0/7", "::ffff:0:0/95", "::fffe:0:0/95", "not-a-cidr",
		"10.0.0.0/8", "192.168.3.10", "fe80::1", "::1", "224.0.0.1",
		"10.0.0.0/8", "2001:db8::/32", "198.51.100.20", " 198.51.100.20 ",
		"203.0.113.0/24", "::ffff:198.51.100.0/120",
	})
	if strings.Join(v4, ",") != "198.51.100.20,203.0.113.0/24,198.51.100.0/24" {
		t.Fatalf("v4 = %v", v4)
	}
	if strings.Join(v6, ",") != "2001:db8::/32" {
		t.Fatalf("v6 = %v", v6)
	}
}

// The fixed allowlist already covers private, link-local, loopback and
// multicast destinations; a direct rule for them must not add a pin that
// shadows the LAN/VPN route.
func TestSplitNetsValidSkipsFixedAllowlist(t *testing.T) {
	v4, v6 := splitNetsValid([]string{
		"10.1.2.0/24", "192.168.3.10", "127.0.0.1", "224.0.0.1",
		"fe80::1", "fc00::1", "::1",
	})
	if len(v4) != 0 || len(v6) != 0 {
		t.Fatalf("fixed allowlist leaked into exceptions: v4=%v v6=%v", v4, v6)
	}
}
