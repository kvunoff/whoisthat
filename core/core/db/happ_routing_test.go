package db

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strconv"
	"testing"
	"whoisthat-core/structs"
)

func setupTestDB(t *testing.T) *DB {
	t.Helper()
	database := &DB{Path: t.TempDir()}
	database.loadOrCreateKey()
	database.ensureDBConfigExistance()
	return database
}

func TestHappRouting_CRUD(t *testing.T) {
	database := setupTestDB(t)

	// Create group
	addedGroup, err := database.AddGroup("TestGroup", "https://example.com/sub")
	if err != nil {
		t.Fatalf("AddGroup failed: %v", err)
	}
	groupID := addedGroup.Id

	profile1 := structs.RoutingProfile{
		Id:             "p1",
		GroupId:        groupID,
		Name:           "Work",
		Source:         "happ",
		State:          "ready",
		DomainStrategy: "AsIs",
		GlobalProxy:    true,
		Rules: []structs.RoutingRule{
			{Type: "field", Domain: "work.com", OutboundTag: "proxy", Enabled: true},
		},
		CreatedAt: 100,
	}

	profile2 := structs.RoutingProfile{
		Id:             "p2",
		GroupId:        groupID,
		Name:           "Home",
		Source:         "happ",
		State:          "ready",
		DomainStrategy: "IPIfNonMatch",
		GlobalProxy:    false,
		Rules: []structs.RoutingRule{
			{Type: "field", Domain: "home.local", OutboundTag: "direct", Enabled: true},
		},
		CreatedAt: 200,
	}

	// Save
	if err := database.SaveRoutingProfile(profile1); err != nil {
		t.Fatalf("SaveRoutingProfile p1 failed: %v", err)
	}
	if err := database.SaveRoutingProfile(profile2); err != nil {
		t.Fatalf("SaveRoutingProfile p2 failed: %v", err)
	}

	// Load
	loaded, err := database.LoadRoutingProfile(groupID, "p1")
	if err != nil {
		t.Fatalf("LoadRoutingProfile p1 failed: %v", err)
	}
	if loaded.Name != "Work" || len(loaded.Rules) != 1 {
		t.Errorf("loaded profile mismatch: %+v", loaded)
	}

	// List
	list, err := database.ListRoutingProfiles(groupID)
	if err != nil {
		t.Fatalf("ListRoutingProfiles failed: %v", err)
	}
	if len(list) != 2 {
		t.Fatalf("expected 2 profiles, got %d", len(list))
	}
	if list[0].Id != "p1" || list[1].Id != "p2" {
		t.Errorf("expected sorted by CreatedAt: p1, p2; got %s, %s", list[0].Id, list[1].Id)
	}

	// FindByName (exact & case-insensitive)
	foundExact, err := database.FindRoutingProfileByName(groupID, "Work")
	if err != nil || foundExact.Id != "p1" {
		t.Errorf("FindRoutingProfileByName exact failed: %v, profile=%+v", err, foundExact)
	}
	foundCase, err := database.FindRoutingProfileByName(groupID, "hOmE")
	if err != nil || foundCase.Id != "p2" {
		t.Errorf("FindRoutingProfileByName case-insensitive failed: %v, profile=%+v", err, foundCase)
	}

	// Delete
	if err := database.DeleteRoutingProfile(groupID, "p1"); err != nil {
		t.Fatalf("DeleteRoutingProfile failed: %v", err)
	}
	listAfter, _ := database.ListRoutingProfiles(groupID)
	if len(listAfter) != 1 || listAfter[0].Id != "p2" {
		t.Errorf("expected only p2 after delete, got %+v", listAfter)
	}
}

func TestHappRouting_ModesAndResolveRouting(t *testing.T) {
	database := setupTestDB(t)

	// Save custom manual routing rule
	manualCfg := structs.RoutingConfig{
		DomainStrategy: "IPIfNonMatch",
		Rules: []structs.RoutingRule{
			{Type: "field", Domain: "manual-rule.com", OutboundTag: "proxy", Enabled: true},
		},
	}
	_ = database.SaveRouting(manualCfg)

	addedGroup, err := database.AddGroup("SubGroup", "https://example.com/sub")
	if err != nil {
		t.Fatalf("AddGroup failed: %v", err)
	}
	groupID := addedGroup.Id

	happProf := structs.RoutingProfile{
		Id:             "happ1",
		GroupId:        groupID,
		Name:           "HappProfile",
		Source:         "happ",
		State:          "ready",
		DomainStrategy: "AsIs",
		Rules: []structs.RoutingRule{
			{Type: "field", Domain: "happ-rule.com", OutboundTag: "proxy", Enabled: true},
		},
		CreatedAt: 100,
	}
	_ = database.SaveRoutingProfile(happProf)

	// Case 1: groupID = 0 -> always manual
	sel0, err := database.ResolveRouting(0)
	if err != nil || sel0.Source != "manual" || sel0.Config.Rules[0].Domain != "manual-rule.com" {
		t.Errorf("case group 0 failed: %+v", sel0)
	}

	// Case 2: default routing_mode is auto, but active profile is not set -> fallback to manual
	selAutoNoActive, err := database.ResolveRouting(groupID)
	if err != nil || selAutoNoActive.Source != "manual" || selAutoNoActive.Config.Rules[0].Domain != "manual-rule.com" {
		t.Errorf("case auto no active failed: %+v", selAutoNoActive)
	}

	// Set active profile
	if err := database.SetActiveRoutingProfile(groupID, "happ1"); err != nil {
		t.Fatalf("SetActiveRoutingProfile failed: %v", err)
	}

	// Case 3: auto with active ready profile -> uses Happ profile
	selAutoActive, err := database.ResolveRouting(groupID)
	if err != nil || selAutoActive.Source != "happ" || selAutoActive.Config.Rules[0].Domain != "happ-rule.com" {
		t.Errorf("case auto with active failed: %+v", selAutoActive)
	}

	// Case 4: manual mode -> ignores active Happ profile
	if err := database.SetGroupRoutingMode(groupID, "manual"); err != nil {
		t.Fatalf("SetGroupRoutingMode manual failed: %v", err)
	}
	selManual, err := database.ResolveRouting(groupID)
	if err != nil || selManual.Source != "manual" || selManual.Config.Rules[0].Domain != "manual-rule.com" {
		t.Errorf("case manual mode failed: %+v", selManual)
	}

	// Case 5: subscription mode with active Happ profile -> uses Happ
	if err := database.SetGroupRoutingMode(groupID, "subscription"); err != nil {
		t.Fatalf("SetGroupRoutingMode subscription failed: %v", err)
	}
	selSubActive, err := database.ResolveRouting(groupID)
	if err != nil || selSubActive.Source != "happ" || selSubActive.Config.Rules[0].Domain != "happ-rule.com" {
		t.Errorf("case subscription with active failed: %+v", selSubActive)
	}

	// Case 6: subscription mode without active Happ profile -> LAN only, no manual rules!
	_ = database.SetActiveRoutingProfile(groupID, "")
	selSubNoActive, err := database.ResolveRouting(groupID)
	if err != nil || selSubNoActive.Source != "happ" {
		t.Errorf("case subscription no active failed: %+v", selSubNoActive)
	}
	if len(selSubNoActive.Config.Rules) != 1 || selSubNoActive.Config.Rules[0].IP != structs.LANRuleIP {
		t.Errorf("expected LAN-only rule for subscription fallback, got: %+v", selSubNoActive.Config.Rules)
	}
}

func TestHappRouting_GeoDirValidation(t *testing.T) {
	database := setupTestDB(t)
	addedGroup, _ := database.AddGroup("GeoGroup", "")
	groupID := addedGroup.Id

	geoDirRel := filepath.Join("groups", strconv.Itoa(groupID), "routing", "geo", "pgeo", "1000")
	geoDirAbs := filepath.Join(database.Path, geoDirRel)

	prof := structs.RoutingProfile{
		Id:             "pgeo",
		GroupId:        groupID,
		Name:           "WithGeo",
		Source:         "happ",
		State:          "ready",
		GeoDir:         geoDirRel,
		DomainStrategy: "IPIfNonMatch",
		Rules: []structs.RoutingRule{
			{Type: "field", Domain: "geosite:cn", OutboundTag: "direct", Enabled: true},
		},
	}
	_ = database.SaveRoutingProfile(prof)

	// Activating when geo files don't exist must fail
	if err := database.SetActiveRoutingProfile(groupID, "pgeo"); err == nil {
		t.Error("expected SetActiveRoutingProfile to fail when geo files do not exist")
	}

	// Create dummy geoip.dat and geosite.dat
	if err := os.MkdirAll(geoDirAbs, 0700); err != nil {
		t.Fatalf("mkdir geo failed: %v", err)
	}
	_ = os.WriteFile(filepath.Join(geoDirAbs, "geoip.dat"), []byte("geo-ip-content"), 0600)
	_ = os.WriteFile(filepath.Join(geoDirAbs, "geosite.dat"), []byte("geo-site-content"), 0600)

	// Now activation must succeed
	if err := database.SetActiveRoutingProfile(groupID, "pgeo"); err != nil {
		t.Fatalf("SetActiveRoutingProfile failed after creating geo files: %v", err)
	}

	// ResolveRouting should now return the absolute GeoDir
	sel, err := database.ResolveRouting(groupID)
	if err != nil {
		t.Fatalf("ResolveRouting failed: %v", err)
	}
	if sel.GeoDir != geoDirAbs {
		t.Errorf("expected GeoDir %s, got %s", geoDirAbs, sel.GeoDir)
	}
}

func TestHappRouting_UpdateGroupAndProfilesPreservesRouting(t *testing.T) {
	database := setupTestDB(t)
	addedGroup, _ := database.AddGroup("SubGroup", "")
	groupID := addedGroup.Id

	prof := structs.RoutingProfile{
		Id:             "subprof",
		GroupId:        groupID,
		Name:           "SubProf",
		Source:         "happ",
		State:          "ready",
		DomainStrategy: "AsIs",
		Rules: []structs.RoutingRule{
			{Type: "field", Domain: "sub.com", OutboundTag: "proxy", Enabled: true},
		},
	}
	_ = database.SaveRoutingProfile(prof)

	// Call UpdateGroupAndProfiles with proxy profiles
	proxyProfiles := []structs.DBAddProfileData{
		{Uri: "vless://test@1.1.1.1:443", GroupId: groupID, Name: "Proxy1", Protocol: "vless"},
	}
	addedProxies, err := database.UpdateGroupAndProfiles(groupID, proxyProfiles, 0)
	if err != nil {
		t.Fatalf("UpdateGroupAndProfiles failed: %v", err)
	}
	if len(addedProxies) != 1 {
		t.Fatalf("expected 1 proxy added, got %d", len(addedProxies))
	}

	// Verify that the routing profile was NOT deleted by UpdateGroupAndProfiles
	loaded, err := database.LoadRoutingProfile(groupID, "subprof")
	if err != nil {
		t.Fatalf("routing profile was unexpectedly deleted by UpdateGroupAndProfiles: %v", err)
	}
	if loaded.Name != "SubProf" {
		t.Errorf("loaded profile corrupted: %+v", loaded)
	}
}

func TestHappRouting_ResolveRoutingPropagatesDNS(t *testing.T) {
	database := setupTestDB(t)
	addedGroup, err := database.AddGroup("DNSGroup", "")
	if err != nil {
		t.Fatalf("AddGroup failed: %v", err)
	}
	groupID := addedGroup.Id

	prof := structs.RoutingProfile{
		Id:             "pdns",
		GroupId:        groupID,
		Name:           "DNSProfile",
		Source:         "happ",
		State:          "ready",
		DomainStrategy: "AsIs",
		GlobalProxy:    false,
		Rules: []structs.RoutingRule{
			{Type: "field", Domain: "direct.example", OutboundTag: "direct", Enabled: true},
		},
		DNS: &structs.HappDNSConfig{
			FakeDNSEnabled:    true,
			RemoteDNSType:     "DoH",
			RemoteDNSDomain:   "https://cloudflare-dns.com/dns-query",
			RemoteDNSIP:       "1.1.1.1",
			DomesticDNSType:   "DoU",
			DomesticDNSIP:     "8.8.8.8",
			DomesticDNSDomain: "",
			Hosts:             map[string]string{"vpn.example.com": "203.0.113.7"},
		},
	}
	if err := database.SaveRoutingProfile(prof); err != nil {
		t.Fatalf("SaveRoutingProfile failed: %v", err)
	}
	if err := database.SetActiveRoutingProfile(groupID, prof.Id); err != nil {
		t.Fatalf("SetActiveRoutingProfile failed: %v", err)
	}

	sel, err := database.ResolveRouting(groupID)
	if err != nil {
		t.Fatalf("ResolveRouting failed: %v", err)
	}
	if sel.Source != "happ" {
		t.Fatalf("expected happ source, got %q", sel.Source)
	}
	if sel.GlobalProxy {
		t.Error("expected GlobalProxy=false to be propagated")
	}
	if sel.Config.DNS == nil {
		t.Fatal("expected DNS config in selection")
	}
	if !sel.Config.DNS.FakeDNSEnabled || sel.Config.DNS.RemoteDNSType != "DoH" || sel.Config.DNS.DomesticDNSType != "DoU" {
		t.Errorf("unexpected DNS config: %+v", sel.Config.DNS)
	}

	infos, err := database.ListRoutingProfileInfos(groupID, prof.Id)
	if err != nil {
		t.Fatalf("ListRoutingProfileInfos failed: %v", err)
	}
	if len(infos) != 1 {
		t.Fatalf("expected 1 info, got %d", len(infos))
	}
	if !infos[0].FakeDNSEnabled {
		t.Error("expected fake_dns_enabled=true in info")
	}
	if infos[0].RemoteDNS != "DoH cloudflare-dns.com" {
		t.Errorf("unexpected RemoteDNS label: %q", infos[0].RemoteDNS)
	}
	if infos[0].DomesticDNS != "DoU 8.8.8.8" {
		t.Errorf("unexpected DomesticDNS label: %q", infos[0].DomesticDNS)
	}
}

func TestHappRouting_BackfillsDNSFromRawJSON(t *testing.T) {
	database := setupTestDB(t)
	addedGroup, err := database.AddGroup("LegacyGroup", "")
	if err != nil {
		t.Fatalf("AddGroup failed: %v", err)
	}
	groupID := addedGroup.Id

	raw := json.RawMessage(`{
		"Name":"Legacy",
		"GlobalProxy":true,
		"RemoteDNSType":"DoH",
		"RemoteDNSDomain":"https://dns.google/dns-query",
		"RemoteDNSIP":"8.8.8.8",
		"DomesticDNSType":"DoU",
		"DomesticDNSIP":"1.1.1.1",
		"DnsHosts":{"vpn.example.com":"203.0.113.9"}
	}`)
	prof := structs.RoutingProfile{
		Id:             "legacy",
		GroupId:        groupID,
		Name:           "Legacy",
		Source:         "happ",
		State:          "ready",
		DomainStrategy: "IPIfNonMatch",
		GlobalProxy:    true,
		RawJSON:        raw,
		Rules: []structs.RoutingRule{
			{Type: "field", Domain: "legacy.example", OutboundTag: "proxy", Enabled: true},
		},
	}
	if err := database.SaveRoutingProfile(prof); err != nil {
		t.Fatalf("SaveRoutingProfile failed: %v", err)
	}
	if err := database.SetActiveRoutingProfile(groupID, prof.Id); err != nil {
		t.Fatalf("SetActiveRoutingProfile failed: %v", err)
	}

	sel, err := database.ResolveRouting(groupID)
	if err != nil {
		t.Fatalf("ResolveRouting failed: %v", err)
	}
	if sel.Config.DNS == nil {
		t.Fatal("expected DNS to be backfilled from RawJSON")
	}
	if sel.Config.DNS.RemoteDNSType != "DoH" || sel.Config.DNS.Hosts["vpn.example.com"] != "203.0.113.9" {
		t.Errorf("unexpected backfilled DNS: %+v", sel.Config.DNS)
	}

	infos, _ := database.ListRoutingProfileInfos(groupID, prof.Id)
	if len(infos) != 1 || infos[0].RemoteDNS != "DoH dns.google" {
		t.Errorf("unexpected info badges: %+v", infos)
	}
}
