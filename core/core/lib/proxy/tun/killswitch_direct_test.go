package tunmode

import (
	"errors"
	"fmt"
	"net"
	"strings"
	"sync/atomic"
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

// dropIsLastBatchRule reports whether the final line before the heredoc
// terminator is the catch-all drop.
func dropIsLastBatchRule(script string) bool {
	return strings.Contains(script, "add rule inet whoisthat_ks output drop\nWHOISTHAT_KS_EOF\n")
}

func TestSplitNetsValidRejectsWholeFamily(t *testing.T) {
	v4, v6 := splitNetsValid([]string{"0.0.0.0/0", "10.0.0.0/8", "198.51.100.20", "::/0", "2001:db8::/16"})
	if len(v4) != 2 || v4[0] != "10.0.0.0/8" || v4[1] != "198.51.100.20" {
		t.Fatalf("v4 = %v, want the /8 and the host", v4)
	}
	if len(v6) != 1 || v6[0] != "2001::/16" {
		t.Fatalf("v6 = %v, want only the /16", v6)
	}
}

func TestSplitNetsValidCapsTotal(t *testing.T) {
	var items []string
	for i := 0; i <= maxDirectExceptions+10; i++ {
		items = append(items, fmt.Sprintf("10.%d.0.0/16", i))
	}
	v4, v6 := splitNetsValid(items)
	if len(v4)+len(v6) != maxDirectExceptions {
		t.Fatalf("kept %d exceptions, want %d", len(v4)+len(v6), maxDirectExceptions)
	}
}

func TestSplitNetsValidCapKeepsOrderAcrossFamilies(t *testing.T) {
	var items []string
	for i := 0; i < maxDirectExceptions-1; i++ {
		items = append(items, fmt.Sprintf("10.%d.0.0/16", i))
	}
	items = append(items, "2001:db8::/32", "2001:db8:1::/48")
	v4, v6 := splitNetsValid(items)
	if len(v4)+len(v6) != maxDirectExceptions {
		t.Fatalf("kept %d, want the cap %d", len(v4)+len(v6), maxDirectExceptions)
	}
	// One slot is left when the v6 entries appear, so the split stage cuts one.
	if len(v6) != 1 {
		t.Fatalf("v6 kept %d, want 1 (the cap is reached)", len(v6))
	}
	if v4[0] != "10.0.0.0/16" {
		t.Fatalf("v4[0] = %q, want the first rule kept", v4[0])
	}
}

// Dead names must not consume the slots that later valid entries need: the
// resolve stage keeps working until it has the cap in hand.
func TestResolveDirectExceptionsSurvivesDeadNamesAhead(t *testing.T) {
	orig := lookupDirectHost
	defer func() { lookupDirectHost = orig }()
	lookupDirectHost = func(host string) ([]net.IP, error) {
		if strings.HasPrefix(host, "dead") {
			return nil, errors.New("nxdomain")
		}
		return []net.IP{net.ParseIP("198.51.100.9")}, nil
	}

	// More dead names than the cap, so a pre-cut to 64 would have missed it.
	var items []string
	for i := 0; i < 100; i++ {
		items = append(items, fmt.Sprintf("dead%d.example", i))
	}
	items = append(items, "alive.example")

	got := resolveDirectExceptions(items)
	if len(got) != 1 || got[0] != "198.51.100.9" {
		t.Fatalf("resolved = %v, want the live target behind 100 dead ones", got)
	}
}

// The work is bounded even when nothing resolves.
func TestResolveDirectExceptionsBoundsDeadWork(t *testing.T) {
	orig := lookupDirectHost
	defer func() { lookupDirectHost = orig }()
	var calls atomic.Int32
	lookupDirectHost = func(string) ([]net.IP, error) { calls.Add(1); return nil, errors.New("nxdomain") }

	var items []string
	for i := 0; i < 5*maxDirectExceptions; i++ {
		// A live name exactly at the work bound must still be out of reach.
		if i == 2*maxDirectExceptions {
			items = append(items, "beyond.example")
			continue
		}
		items = append(items, fmt.Sprintf("dead%d.example", i))
	}
	if got := resolveDirectExceptions(items); len(got) != 0 {
		t.Fatalf("resolved = %v, want none (the live name sits past the work bound)", got)
	}
	if got := calls.Load(); got != 2*maxDirectExceptions {
		t.Fatalf("lookups = %d, want %d", got, 2*maxDirectExceptions)
	}
}
