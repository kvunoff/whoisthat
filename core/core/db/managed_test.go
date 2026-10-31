package db

import (
	"encoding/json"
	"os"
	"testing"

	"whoisthat-core/structs"
)

func managedProfile(key, name, address string) structs.DBAddProfileData {
	raw := json.RawMessage(`{"remarks":"` + name + `","outbounds":[{"tag":"proxy","protocol":"vless","settings":{"vnext":[{"address":"` + address + `","port":443}]}}]}`)
	return structs.DBAddProfileData{
		GroupId:     1,
		Name:        name,
		Protocol:    "vless",
		Address:     address,
		Port:        443,
		Config:      raw,
		ProviderKey: key,
		Managed:     true,
	}
}

func TestUpdateGroupAndProfilesManagedReplacesConfigsByProviderKey(t *testing.T) {
	db := newTestDB(t)
	if err := db.SaveGroup(structs.Group{Id: 1, Name: "json"}); err != nil {
		t.Fatalf("SaveGroup: %v", err)
	}

	first, err := db.UpdateGroupAndProfiles(1, []structs.DBAddProfileData{
		managedProfile("Berlin", "Berlin", "berlin.example"),
		managedProfile("Paris", "Paris", "paris.example"),
	}, 0)
	if err != nil {
		t.Fatalf("first refresh: %v", err)
	}
	ids := map[string]int{}
	for _, p := range first {
		ids[p.ProviderKey] = p.Id
		if !p.Managed {
			t.Errorf("profile %s is not marked managed", p.Name)
		}
		raw, err := db.LoadProfileXrayConfig(1, p.Id)
		if err != nil || len(raw) == 0 {
			t.Fatalf("config for %s: %v", p.Name, err)
		}
	}

	// Reorder, rename the display name, and change the provider config.
	second, err := db.UpdateGroupAndProfiles(1, []structs.DBAddProfileData{
		managedProfile("Paris", "Paris (new)", "paris2.example"),
		managedProfile("Berlin", "Berlin", "berlin.example"),
	}, 0)
	if err != nil {
		t.Fatalf("second refresh: %v", err)
	}
	if len(second) != 2 {
		t.Fatalf("second refresh = %d profiles, want 2", len(second))
	}
	for _, p := range second {
		if p.Id != ids[p.ProviderKey] {
			t.Errorf("profile %s id = %d, want stable %d", p.ProviderKey, p.Id, ids[p.ProviderKey])
		}
	}

	parisRaw, err := db.LoadProfileXrayConfig(1, ids["Paris"])
	if err != nil {
		t.Fatalf("load Paris config: %v", err)
	}
	var parsed struct {
		Outbounds []struct {
			Settings struct {
				VNext []struct {
					Address string `json:"address"`
				} `json:"vnext"`
			} `json:"settings"`
		} `json:"outbounds"`
	}
	if err := json.Unmarshal(parisRaw, &parsed); err != nil {
		t.Fatalf("unmarshal Paris config: %v", err)
	}
	if parsed.Outbounds[0].Settings.VNext[0].Address != "paris2.example" {
		t.Errorf("provider config was not replaced: %s", parisRaw)
	}

	// Removing a provider entry must remove its stored config too.
	third, err := db.UpdateGroupAndProfiles(1, []structs.DBAddProfileData{
		managedProfile("Berlin", "Berlin", "berlin.example"),
	}, 0)
	if err != nil {
		t.Fatalf("third refresh: %v", err)
	}
	if len(third) != 1 {
		t.Fatalf("third refresh = %d profiles, want 1", len(third))
	}
	if _, err := os.Stat(db.GetProfileXrayConfigPath(1, ids["Paris"])); !os.IsNotExist(err) {
		t.Errorf("stale xray config for removed profile still exists: %v", err)
	}
}

func TestDeleteProfileRemovesManagedConfig(t *testing.T) {
	db := newTestDB(t)
	if err := db.SaveGroup(structs.Group{Id: 1, Name: "json"}); err != nil {
		t.Fatalf("SaveGroup: %v", err)
	}
	profiles, err := db.UpdateGroupAndProfiles(1, []structs.DBAddProfileData{
		managedProfile("Berlin", "Berlin", "berlin.example"),
	}, 0)
	if err != nil {
		t.Fatalf("refresh: %v", err)
	}
	if err := db.DeleteProfile(1, profiles[0].Id); err != nil {
		t.Fatalf("DeleteProfile: %v", err)
	}
	if _, err := os.Stat(db.GetProfileXrayConfigPath(1, profiles[0].Id)); !os.IsNotExist(err) {
		t.Errorf("xray config survived profile deletion: %v", err)
	}
}
