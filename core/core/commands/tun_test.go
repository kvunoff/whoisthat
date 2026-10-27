package cmd

import (
	"testing"

	appconfig "whoisthat-core/lib/AppConfig"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
)

func TestDnsHijackForFallsBackToLegacy(t *testing.T) {
	cmd, _, _ := newTestCmd(t)

	plan := cmd.dnsHijackFor(structs.Profile{GroupId: 0})
	if !plan.Enabled || !plan.Legacy || plan.Happ {
		t.Fatalf("expected legacy hijack, got %+v", plan)
	}
	if plan.Sink4 == "" {
		t.Error("legacy plan must carry a v4 resolver")
	}
}

func TestDnsHijackForUsesSinkForHappDNS(t *testing.T) {
	cmd, _, _ := newTestCmd(t)
	group, err := cmd.DB.AddGroup("DNSGroup", "")
	if err != nil {
		t.Fatalf("AddGroup failed: %v", err)
	}

	profile := structs.RoutingProfile{
		Id:      "dns",
		GroupId: group.Id,
		Name:    "DNS",
		Source:  "happ",
		State:   "ready",
		Rules: []structs.RoutingRule{
			{Type: "field", Domain: "example.com", OutboundTag: "direct", Enabled: true},
		},
		DNS: &structs.HappDNSConfig{
			RemoteDNSType: "DoU",
			RemoteDNSIP:   "1.1.1.1",
		},
	}
	if err := cmd.DB.SaveRoutingProfile(profile); err != nil {
		t.Fatalf("SaveRoutingProfile failed: %v", err)
	}
	if err := cmd.DB.SetActiveRoutingProfile(group.Id, profile.Id); err != nil {
		t.Fatalf("SetActiveRoutingProfile failed: %v", err)
	}

	plan := cmd.dnsHijackFor(structs.Profile{GroupId: group.Id})
	if !plan.Enabled || !plan.Happ || plan.Legacy {
		t.Fatalf("expected Happ hijack, got %+v", plan)
	}
	if plan.Sink4 != tunmode.DNSSink4 || plan.Sink6 != tunmode.DNSSink6 {
		t.Errorf("unexpected sinks: %+v", plan)
	}
	if plan.ScopeMark != 1 {
		t.Errorf("expected mark-1 scope, got %d", plan.ScopeMark)
	}

	// include mode must keep the Happ plan but disable the hijack.
	appconfig.SetSplitTunnelMode("include")
	t.Cleanup(func() { appconfig.SetSplitTunnelMode("off") })
	plan = cmd.dnsHijackFor(structs.Profile{GroupId: group.Id})
	if plan.Enabled {
		t.Errorf("include mode must disable the hijack: %+v", plan)
	}
	if !plan.Happ || plan.Sink4 != tunmode.DNSSink4 {
		t.Errorf("include mode must preserve the Happ plan: %+v", plan)
	}
}

func TestDnsHijackForManualModeUsesLegacy(t *testing.T) {
	cmd, _, _ := newTestCmd(t)
	group, err := cmd.DB.AddGroup("ManualGroup", "")
	if err != nil {
		t.Fatalf("AddGroup failed: %v", err)
	}

	profile := structs.RoutingProfile{
		Id:      "dns",
		GroupId: group.Id,
		Name:    "DNS",
		Source:  "happ",
		State:   "ready",
		Rules: []structs.RoutingRule{
			{Type: "field", Domain: "example.com", OutboundTag: "direct", Enabled: true},
		},
		DNS: &structs.HappDNSConfig{
			RemoteDNSType: "DoU",
			RemoteDNSIP:   "1.1.1.1",
		},
	}
	if err := cmd.DB.SaveRoutingProfile(profile); err != nil {
		t.Fatalf("SaveRoutingProfile failed: %v", err)
	}
	if err := cmd.DB.SetGroupRoutingMode(group.Id, "manual"); err != nil {
		t.Fatalf("SetGroupRoutingMode failed: %v", err)
	}

	plan := cmd.dnsHijackFor(structs.Profile{GroupId: group.Id})
	if !plan.Legacy || plan.Happ {
		t.Errorf("manual routing must use the legacy hijack: %+v", plan)
	}
}
