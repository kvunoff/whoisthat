package mainproxy

import (
	"errors"
	"strings"
	"testing"

	"whoisthat-core/structs"
)

func TestRearmKillSwitchPublishesRules(t *testing.T) {
	var p ProxyManager
	p.proxyIPs = []string{"192.0.2.1"}

	var seen []string
	p.SetPreStartHandler(func([]string) error {
		seen = p.DirectTargets()
		return nil
	})

	err := p.rearmKillSwitch([]structs.RoutingRule{
		{Type: "field", Domain: "api.example.test", OutboundTag: "direct", Enabled: true},
	})
	if err != nil {
		t.Fatalf("rearmKillSwitch: %v", err)
	}
	if len(seen) != 1 || seen[0] != "api.example.test" {
		t.Fatalf("handler saw %v, want the new direct rule", seen)
	}
}

func TestRearmKillSwitchErrorPropagates(t *testing.T) {
	var p ProxyManager
	p.proxyIPs = []string{"192.0.2.1"}
	p.SetPreStartHandler(func([]string) error { return errors.New("arm failed") })

	err := p.rearmKillSwitch(nil)
	if err == nil || !strings.Contains(err.Error(), "could not be re-armed") {
		t.Fatalf("want a re-arm error, got %v", err)
	}
}

func TestRearmKillSwitchSkipsWithoutProxyIPs(t *testing.T) {
	var p ProxyManager
	called := false
	p.SetPreStartHandler(func([]string) error {
		called = true
		return nil
	})

	if err := p.rearmKillSwitch([]structs.RoutingRule{
		{Type: "field", IP: "198.51.100.7", OutboundTag: "direct", Enabled: true},
	}); err != nil {
		t.Fatalf("rearmKillSwitch: %v", err)
	}
	if called {
		t.Fatal("handler called without proxy IPs")
	}
	if got := p.DirectTargets(); len(got) != 1 || got[0] != "198.51.100.7" {
		t.Fatalf("targets not published: %v", got)
	}
}
