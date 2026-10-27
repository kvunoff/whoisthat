package db

import (
	"os"
	"testing"
	"whoisthat-core/structs"
)

func routingConfigWithDomain(domain string) structs.RoutingConfig {
	return structs.RoutingConfig{
		DomainStrategy: "IPIfNonMatch",
		Rules: []structs.RoutingRule{
			{
				Type:        "field",
				Domain:      domain,
				OutboundTag: "proxy",
				Enabled:     true,
			},
		},
	}
}

func TestManualRouting_DefaultIsGlobal(t *testing.T) {
	database := setupTestDB(t)

	added, err := database.AddGroup("Group", "")
	if err != nil {
		t.Fatalf("AddGroup failed: %v", err)
	}

	global := routingConfigWithDomain("global.example")
	if err := database.SaveRouting(global); err != nil {
		t.Fatalf("SaveRouting failed: %v", err)
	}

	cfg, source, err := database.LoadEffectiveManualRouting(added.Id)
	if err != nil {
		t.Fatalf("LoadEffectiveManualRouting failed: %v", err)
	}
	if source != structs.RoutingSourceGlobal {
		t.Fatalf("expected global source, got %q", source)
	}
	if len(cfg.Rules) != 1 || cfg.Rules[0].Domain != "global.example" {
		t.Fatalf("expected global rules, got %+v", cfg)
	}
}

func TestManualRouting_CustomSeedsFromGlobalAndPreservesOnInherit(t *testing.T) {
	database := setupTestDB(t)

	added, err := database.AddGroup("Group", "")
	if err != nil {
		t.Fatalf("AddGroup failed: %v", err)
	}

	global := routingConfigWithDomain("global.example")
	if err := database.SaveRouting(global); err != nil {
		t.Fatalf("SaveRouting failed: %v", err)
	}

	if err := database.SetManualRoutingSource(added.Id, structs.ManualRoutingCustom); err != nil {
		t.Fatalf("SetManualRoutingSource(custom) failed: %v", err)
	}

	seeded, err := database.LoadGroupRouting(added.Id)
	if err != nil {
		t.Fatalf("LoadGroupRouting failed: %v", err)
	}
	if len(seeded.Rules) != 1 || seeded.Rules[0].Domain != "global.example" {
		t.Fatalf("custom rules were not seeded from global: %+v", seeded)
	}

	custom := routingConfigWithDomain("custom.example")
	if err := database.SaveGroupRouting(added.Id, custom); err != nil {
		t.Fatalf("SaveGroupRouting failed: %v", err)
	}

	cfg, source, err := database.LoadEffectiveManualRouting(added.Id)
	if err != nil {
		t.Fatalf("LoadEffectiveManualRouting failed: %v", err)
	}
	if source != structs.RoutingSourceCustom {
		t.Fatalf("expected custom source, got %q", source)
	}
	if len(cfg.Rules) != 1 || cfg.Rules[0].Domain != "custom.example" {
		t.Fatalf("expected custom rules, got %+v", cfg)
	}

	if err := database.SetManualRoutingSource(added.Id, structs.ManualRoutingInherit); err != nil {
		t.Fatalf("SetManualRoutingSource(inherit) failed: %v", err)
	}

	cfg, source, err = database.LoadEffectiveManualRouting(added.Id)
	if err != nil {
		t.Fatalf("LoadEffectiveManualRouting failed: %v", err)
	}
	if source != structs.RoutingSourceGlobal {
		t.Fatalf("expected global source after inherit, got %q", source)
	}
	if len(cfg.Rules) != 1 || cfg.Rules[0].Domain != "global.example" {
		t.Fatalf("expected global rules after inherit, got %+v", cfg)
	}

	stored, err := database.LoadGroupRoutingIfExists(added.Id)
	if err != nil {
		t.Fatalf("LoadGroupRoutingIfExists failed: %v", err)
	}
	if stored == nil || len(stored.Rules) != 1 || stored.Rules[0].Domain != "custom.example" {
		t.Fatalf("custom rules were not preserved on inherit: %+v", stored)
	}
}

func TestManualRouting_ResolveRoutingHonorsSource(t *testing.T) {
	database := setupTestDB(t)

	added, err := database.AddGroup("Group", "")
	if err != nil {
		t.Fatalf("AddGroup failed: %v", err)
	}

	if err := database.SaveRouting(routingConfigWithDomain("global.example")); err != nil {
		t.Fatalf("SaveRouting failed: %v", err)
	}
	if err := database.SetGroupRoutingMode(added.Id, "manual"); err != nil {
		t.Fatalf("SetGroupRoutingMode failed: %v", err)
	}

	selection, err := database.ResolveRouting(added.Id)
	if err != nil {
		t.Fatalf("ResolveRouting failed: %v", err)
	}
	if selection.Source != "manual" || len(selection.Config.Rules) != 1 || selection.Config.Rules[0].Domain != "global.example" {
		t.Fatalf("expected global manual selection, got %+v", selection)
	}

	if err := database.SetManualRoutingSource(added.Id, structs.ManualRoutingCustom); err != nil {
		t.Fatalf("SetManualRoutingSource failed: %v", err)
	}
	if err := database.SaveGroupRouting(added.Id, routingConfigWithDomain("custom.example")); err != nil {
		t.Fatalf("SaveGroupRouting failed: %v", err)
	}

	selection, err = database.ResolveRouting(added.Id)
	if err != nil {
		t.Fatalf("ResolveRouting failed: %v", err)
	}
	if len(selection.Config.Rules) != 1 || selection.Config.Rules[0].Domain != "custom.example" {
		t.Fatalf("expected custom manual selection, got %+v", selection)
	}
}

func TestManualRouting_DeleteGroupRemovesOverride(t *testing.T) {
	database := setupTestDB(t)

	added, err := database.AddGroup("Group", "")
	if err != nil {
		t.Fatalf("AddGroup failed: %v", err)
	}
	if err := database.SetManualRoutingSource(added.Id, structs.ManualRoutingCustom); err != nil {
		t.Fatalf("SetManualRoutingSource failed: %v", err)
	}

	path := database.GetGroupRoutingFilePath(added.Id)
	if _, err := os.Stat(path); err != nil {
		t.Fatalf("expected custom routing file to exist: %v", err)
	}

	if err := database.DeleteGroup(added.Id); err != nil {
		t.Fatalf("DeleteGroup failed: %v", err)
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Fatalf("expected custom routing file to be removed, err=%v", err)
	}
}
