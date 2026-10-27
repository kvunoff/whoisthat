package cmd

import (
	"encoding/json"
	"testing"
	"time"
	"whoisthat-core/structs"
)

func routingUpdateFromMessage(t *testing.T, raw []byte) structs.RoutingUpdated {
	t.Helper()
	var envelope struct {
		Msg  string                 `json:"msg"`
		Data structs.RoutingUpdated `json:"data"`
	}
	if err := json.Unmarshal(raw, &envelope); err != nil {
		t.Fatalf("failed to unmarshal routing-updated: %v", err)
	}
	return envelope.Data
}

func TestRouting_GetRoutingUsesGroupAndGlobalPages(t *testing.T) {
	cmd, _, msgChan := newTestCmd(t)

	added, err := cmd.DB.AddGroup("Group", "")
	if err != nil {
		t.Fatalf("AddGroup failed: %v", err)
	}
	if err := cmd.DB.SaveRouting(structs.RoutingConfig{
		DomainStrategy: "IPIfNonMatch",
		Rules: []structs.RoutingRule{
			{Type: "field", Domain: "global.example", OutboundTag: "direct", Enabled: true},
		},
	}); err != nil {
		t.Fatalf("SaveRouting failed: %v", err)
	}

	cmd.GetRouting(structs.GetRoutingData{GroupId: 0})
	global := routingUpdateFromMessage(t, waitForMessage(t, msgChan, "routing-updated", time.Second))
	if global.GroupId != 0 || global.Source != structs.RoutingSourceGlobal {
		t.Fatalf("unexpected global payload: %+v", global)
	}
	if len(global.Config.Rules) != 1 || global.Config.Rules[0].Domain != "global.example" {
		t.Fatalf("unexpected global rules: %+v", global.Config)
	}
	if global.Custom != nil {
		t.Fatalf("global page must not carry a custom override: %+v", global.Custom)
	}

	cmd.GetRouting(structs.GetRoutingData{GroupId: added.Id})
	inherited := routingUpdateFromMessage(t, waitForMessage(t, msgChan, "routing-updated", time.Second))
	if inherited.GroupId != added.Id || inherited.Source != structs.RoutingSourceGlobal {
		t.Fatalf("unexpected inherited payload: %+v", inherited)
	}
	if inherited.Config.Rules[0].Domain != "global.example" {
		t.Fatalf("inheriting group must use global rules: %+v", inherited.Config)
	}
}

func TestRouting_UpdateRoutingAutoCreatesCustomOverride(t *testing.T) {
	cmd, pm, msgChan := newTestCmd(t)

	added, err := cmd.DB.AddGroup("Group", "")
	if err != nil {
		t.Fatalf("AddGroup failed: %v", err)
	}
	if err := cmd.DB.SaveRouting(structs.RoutingConfig{
		DomainStrategy: "IPIfNonMatch",
		Rules: []structs.RoutingRule{
			{Type: "field", Domain: "global.example", OutboundTag: "direct", Enabled: true},
		},
	}); err != nil {
		t.Fatalf("SaveRouting failed: %v", err)
	}

	custom := structs.RoutingConfig{
		DomainStrategy: "AsIs",
		Rules: []structs.RoutingRule{
			{Type: "field", Domain: "custom.example", OutboundTag: "proxy", Enabled: true},
		},
	}
	cmd.UpdateRouting(structs.UpdateRoutingData{GroupId: added.Id, Config: custom}, pm)

	var groupUpdated struct {
		Msg  string        `json:"msg"`
		Data structs.Group `json:"data"`
	}
	if err := json.Unmarshal(waitForMessage(t, msgChan, "group-updated", time.Second), &groupUpdated); err != nil {
		t.Fatalf("failed to unmarshal group-updated: %v", err)
	}
	if groupUpdated.Data.ManualRoutingMode != structs.ManualRoutingCustom {
		t.Fatalf("expected group to become custom, got %q", groupUpdated.Data.ManualRoutingMode)
	}

	updated := routingUpdateFromMessage(t, waitForMessage(t, msgChan, "routing-updated", time.Second))
	if updated.Source != structs.RoutingSourceCustom {
		t.Fatalf("expected custom source, got %q", updated.Source)
	}
	if len(updated.Config.Rules) != 1 || updated.Config.Rules[0].Domain != "custom.example" {
		t.Fatalf("unexpected saved custom rules: %+v", updated.Config)
	}

	cfg, source, err := cmd.DB.LoadEffectiveManualRouting(added.Id)
	if err != nil {
		t.Fatalf("LoadEffectiveManualRouting failed: %v", err)
	}
	if source != structs.RoutingSourceCustom || cfg.Rules[0].Domain != "custom.example" {
		t.Fatalf("custom rules not persisted: source=%s config=%+v", source, cfg)
	}

	global, err := cmd.DB.LoadRouting()
	if err != nil {
		t.Fatalf("LoadRouting failed: %v", err)
	}
	if global.Rules[0].Domain != "global.example" {
		t.Fatalf("global rules must remain untouched: %+v", global)
	}
}

func TestRouting_SetManualRoutingSourceKeepsCustomRules(t *testing.T) {
	cmd, pm, msgChan := newTestCmd(t)

	added, err := cmd.DB.AddGroup("Group", "")
	if err != nil {
		t.Fatalf("AddGroup failed: %v", err)
	}
	if err := cmd.DB.SaveRouting(structs.RoutingConfig{
		DomainStrategy: "IPIfNonMatch",
		Rules:          []structs.RoutingRule{{Type: "field", Domain: "global.example", OutboundTag: "direct", Enabled: true}},
	}); err != nil {
		t.Fatalf("SaveRouting failed: %v", err)
	}

	cmd.SetManualRoutingSource(structs.SetManualRoutingSourceData{
		GroupId: added.Id,
		Source:  structs.ManualRoutingCustom,
	}, pm)
	waitForMessage(t, msgChan, "group-updated", time.Second)
	waitForMessage(t, msgChan, "routing-updated", time.Second)

	if err := cmd.DB.SaveGroupRouting(added.Id, structs.RoutingConfig{
		DomainStrategy: "AsIs",
		Rules:          []structs.RoutingRule{{Type: "field", Domain: "saved.example", OutboundTag: "proxy", Enabled: true}},
	}); err != nil {
		t.Fatalf("SaveGroupRouting failed: %v", err)
	}

	cmd.SetManualRoutingSource(structs.SetManualRoutingSourceData{
		GroupId: added.Id,
		Source:  structs.ManualRoutingInherit,
	}, pm)

	var groupUpdated struct {
		Msg  string        `json:"msg"`
		Data structs.Group `json:"data"`
	}
	if err := json.Unmarshal(waitForMessage(t, msgChan, "group-updated", time.Second), &groupUpdated); err != nil {
		t.Fatalf("failed to unmarshal group-updated: %v", err)
	}
	if groupUpdated.Data.ManualRoutingMode != structs.ManualRoutingInherit {
		t.Fatalf("expected inherit mode, got %q", groupUpdated.Data.ManualRoutingMode)
	}

	updated := routingUpdateFromMessage(t, waitForMessage(t, msgChan, "routing-updated", time.Second))
	if updated.Source != structs.RoutingSourceGlobal || updated.Config.Rules[0].Domain != "global.example" {
		t.Fatalf("expected global rules after inherit, got %+v", updated)
	}
	if updated.Custom == nil || updated.Custom.Rules[0].Domain != "saved.example" {
		t.Fatalf("saved custom rules must be preserved in the payload: %+v", updated.Custom)
	}

	stored, err := cmd.DB.LoadGroupRoutingIfExists(added.Id)
	if err != nil {
		t.Fatalf("LoadGroupRoutingIfExists failed: %v", err)
	}
	if stored == nil || stored.Rules[0].Domain != "saved.example" {
		t.Fatalf("saved custom rules must stay on disk: %+v", stored)
	}
}
