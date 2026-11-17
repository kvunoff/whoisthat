package tunmode

import (
	"fmt"
	"net"
	"strings"
	"testing"
)

func TestKillSwitchAcceptsDirectRuleResolvedAddresses(t *testing.T) {
	orig := lookupDirectHost
	t.Cleanup(func() { lookupDirectHost = orig })
	lookupDirectHost = func(host string) ([]net.IP, error) {
		if host != "api.example.test" {
			return nil, fmt.Errorf("unexpected lookup %q", host)
		}
		return []net.IP{net.ParseIP("198.51.100.20"), net.ParseIP("2001:db8:1::20")}, nil
	}

	direct := resolveDirectExceptions([]string{"api.example.test", "203.0.113.0/24"})
	nft := buildKillSwitchNftScript([]string{"192.0.2.10"}, "tun0", nil, direct...)
	ipt := buildKillSwitchIptablesScript([]string{"192.0.2.10"}, "tun0", nil, direct...)

	for _, want := range []string{
		"ip daddr 198.51.100.20 accept",
		"ip6 daddr 2001:db8:1::20 accept",
		"ip daddr 203.0.113.0/24 accept",
	} {
		if !strings.Contains(nft, want) {
			t.Errorf("nft missing %q", want)
		}
	}
	for _, want := range []string{
		"iptables -A WHOISTHAT_KS -d 198.51.100.20 -j ACCEPT",
		"ip6tables -A WHOISTHAT_KS -d 2001:db8:1::20 -j ACCEPT",
	} {
		if !strings.Contains(ipt, want) {
			t.Errorf("iptables missing %q", want)
		}
	}
	// Unrelated public addresses stay dropped; the drop is still last.
	if strings.Contains(nft, "198.51.100.21") || strings.Contains(nft, "2001:db8:2::") {
		t.Errorf("unrelated address accepted:\n%s", nft)
	}
	if !dropIsLastBatchRule(nft) {
		t.Errorf("drop must stay the last rule:\n%s", nft)
	}
}

func TestKillSwitchDirectExceptionsRejectInjectionAndSurviveLookupFailure(t *testing.T) {
	orig := lookupDirectHost
	t.Cleanup(func() { lookupDirectHost = orig })
	lookupDirectHost = func(string) ([]net.IP, error) { return nil, fmt.Errorf("nxdomain") }

	if got := resolveDirectExceptions([]string{"gone.example.test"}); len(got) != 0 {
		t.Fatalf("unresolvable name must yield no exception, got %v", got)
	}
	script := buildKillSwitchNftScript(nil, "", nil, "1.2.3.4; rm -rf /", "198.51.100.9")
	if strings.Contains(script, "rm -rf") || !strings.Contains(script, "ip daddr 198.51.100.9 accept") {
		t.Fatalf("injection boundary broken:\n%s", script)
	}
}
