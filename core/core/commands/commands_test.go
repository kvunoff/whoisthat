package cmd

import (
	"encoding/base64"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"
	"time"
	"whoisthat-core/db"
	"whoisthat-core/lib/proxy/mainproxy"
	"whoisthat-core/structs"
)

func newTestCmd(t *testing.T) (*Cmd, *mainproxy.ProxyManager, chan []byte) {
	t.Helper()
	testDB := &db.DB{Path: t.TempDir()}
	testDB.Initialize()

	pm := &mainproxy.ProxyManager{DB: testDB}
	pm.Init()

	msgChan := make(chan []byte, 100)
	var mu sync.Mutex
	broadcast := func(data []byte) {
		mu.Lock()
		defer mu.Unlock()
		select {
		case msgChan <- data:
		default:
		}
	}

	cmd := &Cmd{
		DB:        testDB,
		Broadcast: broadcast,
	}

	return cmd, pm, msgChan
}

func waitForMessage(t *testing.T, ch chan []byte, expectedMsg string, timeout time.Duration) []byte {
	t.Helper()
	deadline := time.After(timeout)
	for {
		select {
		case data := <-ch:
			var raw struct {
				Msg string `json:"msg"`
			}
			if err := json.Unmarshal(data, &raw); err == nil && raw.Msg == expectedMsg {
				return data
			}
		case <-deadline:
			t.Fatalf("timed out waiting for message %q", expectedMsg)
			return nil
		}
	}
}

func TestAddGroup_WithSubscriptionUrl_AutoUpdates(t *testing.T) {
	cmd, pm, msgChan := newTestCmd(t)

	// Mock subscription server returning a valid base64 subscription
	profileURI := "vless://11111111-2222-3333-4444-555555555555@1.2.3.4:443?type=tcp&security=none#AutoSubServer\n"
	subContent := base64.StdEncoding.EncodeToString([]byte(profileURI))

	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("subscription-userinfo", "upload=100; download=200; total=1000; expire=1700000000")
		w.WriteHeader(http.StatusOK)
		w.Write([]byte(subContent))
	}))
	defer ts.Close()

	cmd.AddGroup(structs.AddGroupData{
		Name:            "My Auto Sub",
		SubscriptionUrl: ts.URL,
	}, pm)

	// 1. Should broadcast "group-added"
	waitForMessage(t, msgChan, "group-added", 2*time.Second)

	// 2. Should automatically trigger update and broadcast "subscription-updated"
	subUpdatedBytes := waitForMessage(t, msgChan, "subscription-updated", 3*time.Second)

	var subUpdated struct {
		Msg  string                      `json:"msg"`
		Data structs.SubscriptionUpdated `json:"data"`
	}
	if err := json.Unmarshal(subUpdatedBytes, &subUpdated); err != nil {
		t.Fatalf("failed to unmarshal subscription-updated: %v", err)
	}

	if len(subUpdated.Data.Profiles) != 1 {
		t.Fatalf("expected 1 profile, got %d", len(subUpdated.Data.Profiles))
	}
	if subUpdated.Data.Profiles[0].Name != "AutoSubServer" {
		t.Errorf("expected profile name 'AutoSubServer', got %q", subUpdated.Data.Profiles[0].Name)
	}
	if subUpdated.Data.Group.SubTotal != 1000 {
		t.Errorf("expected sub total 1000, got %d", subUpdated.Data.Group.SubTotal)
	}
}

func TestAddGroup_WithoutSubscriptionUrl_NoAutoUpdate(t *testing.T) {
	cmd, pm, msgChan := newTestCmd(t)

	cmd.AddGroup(structs.AddGroupData{
		Name:            "Manual Group",
		SubscriptionUrl: "",
	}, pm)

	// Should broadcast "group-added"
	waitForMessage(t, msgChan, "group-added", 2*time.Second)

	// Should NOT broadcast "subscription-updated"
	select {
	case data := <-msgChan:
		var raw struct {
			Msg string `json:"msg"`
		}
		_ = json.Unmarshal(data, &raw)
		if raw.Msg == "subscription-updated" {
			t.Fatalf("unexpected subscription-updated for empty subscription URL")
		}
	case <-time.After(300 * time.Millisecond):
		// Expected timeout, no update fired
	}
}

func TestUpdateGroup_UrlChanged_AutoUpdates(t *testing.T) {
	cmd, pm, msgChan := newTestCmd(t)

	// First add an empty group
	cmd.AddGroup(structs.AddGroupData{
		Name:            "Initial Group",
		SubscriptionUrl: "",
	}, pm)
	waitForMessage(t, msgChan, "group-added", 2*time.Second)

	profileURI := "vless://22222222-3333-4444-5555-666666666666@5.6.7.8:443?type=tcp&security=none#UpdatedServer\n"
	subContent := base64.StdEncoding.EncodeToString([]byte(profileURI))

	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		w.Write([]byte(subContent))
	}))
	defer ts.Close()

	// Update the group with a new subscription URL
	cmd.UpdateGroup(structs.UpdateGroupData{
		Id:              1,
		Name:            "Renamed Group",
		SubscriptionUrl: ts.URL,
	}, pm)

	waitForMessage(t, msgChan, "group-updated", 2*time.Second)
	subUpdatedBytes := waitForMessage(t, msgChan, "subscription-updated", 3*time.Second)

	var subUpdated struct {
		Msg  string                      `json:"msg"`
		Data structs.SubscriptionUpdated `json:"data"`
	}
	if err := json.Unmarshal(subUpdatedBytes, &subUpdated); err != nil {
		t.Fatalf("failed to unmarshal subscription-updated: %v", err)
	}
	if len(subUpdated.Data.Profiles) != 1 {
		t.Fatalf("expected 1 profile, got %d", len(subUpdated.Data.Profiles))
	}
	if subUpdated.Data.Profiles[0].Name != "UpdatedServer" {
		t.Errorf("expected profile name 'UpdatedServer', got %q", subUpdated.Data.Profiles[0].Name)
	}
}

func TestUpdateSubscription_ExtractsHappRouting(t *testing.T) {
	cmd, pm, msgChan := newTestCmd(t)

	happJSON := `{"Name":"HappProvider","GlobalProxy":true,"LastUpdated":"1000","ProxySites":["google.com"]}`
	happB64 := base64.StdEncoding.EncodeToString([]byte(happJSON))
	happLink := "happ://routing/onadd/" + happB64

	profileURI := "vless://22222222-3333-4444-5555-666666666666@5.6.7.8:443?type=tcp&security=none#Server1\n"
	subContent := base64.StdEncoding.EncodeToString([]byte(profileURI))

	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Routing", happLink)
		w.WriteHeader(http.StatusOK)
		w.Write([]byte(subContent))
	}))
	defer ts.Close()

	cmd.AddGroup(structs.AddGroupData{
		Name:            "HappGroup",
		SubscriptionUrl: ts.URL,
	}, pm)
	waitForMessage(t, msgChan, "group-added", 2*time.Second)

	subUpdatedBytes := waitForMessage(t, msgChan, "subscription-updated", 3*time.Second)
	var subUpdated struct {
		Msg  string                      `json:"msg"`
		Data structs.SubscriptionUpdated `json:"data"`
	}
	if err := json.Unmarshal(subUpdatedBytes, &subUpdated); err != nil {
		t.Fatalf("failed to unmarshal subscription-updated: %v", err)
	}

	if len(subUpdated.Data.Profiles) != 1 {
		t.Fatalf("expected 1 proxy profile, got %d", len(subUpdated.Data.Profiles))
	}
	if len(subUpdated.Data.RoutingProfiles) != 1 {
		t.Fatalf("expected 1 routing profile, got %d", len(subUpdated.Data.RoutingProfiles))
	}
	rp := subUpdated.Data.RoutingProfiles[0]
	if rp.Name != "HappProvider" {
		t.Errorf("expected routing profile name 'HappProvider', got %q", rp.Name)
	}
	if !rp.Active {
		t.Errorf("expected routing profile to be active because of onadd, got active=%v", rp.Active)
	}
}

func TestHappRoutingCommands_CRUD_Lifecycle(t *testing.T) {
	cmd, pm, msgChan := newTestCmd(t)

	// 1. Add a group
	groupAdded, err := cmd.DB.AddGroup("TestGroup", "")
	if err != nil {
		t.Fatalf("failed to add group: %v", err)
	}
	gid := groupAdded.Id

	// 2. Import routing link via cmd.ImportHappRouting
	linkJSON := `{"Name":"CustomRoute","GlobalProxy":"true","DirectSites":["geosite:cn"],"ProxySites":["domain:google.com"]}`
	linkB64 := base64.StdEncoding.EncodeToString([]byte(linkJSON))
	happLink := "happ://routing/add/" + linkB64

	cmd.ImportHappRouting(structs.ImportHappRoutingData{
		GroupId: gid,
		Link:    happLink,
	}, pm)

	subUpdatedBytes := waitForMessage(t, msgChan, "subscription-updated", 2*time.Second)
	var subUpdated struct {
		Msg  string                      `json:"msg"`
		Data structs.SubscriptionUpdated `json:"data"`
	}
	if err := json.Unmarshal(subUpdatedBytes, &subUpdated); err != nil {
		t.Fatalf("failed to unmarshal subscription-updated: %v", err)
	}
	if len(subUpdated.Data.RoutingProfiles) != 1 {
		t.Fatalf("expected 1 routing profile, got %d", len(subUpdated.Data.RoutingProfiles))
	}
	rp := subUpdated.Data.RoutingProfiles[0]
	if rp.Name != "CustomRoute" {
		t.Errorf("expected CustomRoute, got %s", rp.Name)
	}
	if !rp.Active {
		t.Errorf("first add profile with no active profile should become active")
	}

	// 3. Set routing mode to "manual"
	cmd.SetGroupRoutingMode(structs.SetGroupRoutingModeData{
		GroupId: gid,
		Mode:    "manual",
	}, pm)

	grpUpdatedBytes := waitForMessage(t, msgChan, "group-updated", 2*time.Second)
	var grpUpdated struct {
		Msg  string        `json:"msg"`
		Data structs.Group `json:"data"`
	}
	if err := json.Unmarshal(grpUpdatedBytes, &grpUpdated); err != nil {
		t.Fatalf("failed to unmarshal group-updated: %v", err)
	}
	if grpUpdated.Data.RoutingMode != "manual" {
		t.Errorf("expected RoutingMode 'manual', got %q", grpUpdated.Data.RoutingMode)
	}

	// Invalid routing mode should trigger warn
	cmd.SetGroupRoutingMode(structs.SetGroupRoutingModeData{
		GroupId: gid,
		Mode:    "invalid-mode",
	}, pm)
	waitForMessage(t, msgChan, "warn", 2*time.Second)

	// 4. Deactivate routing profile with "off"
	cmd.SetActiveRoutingProfile(structs.SetActiveRoutingProfileData{
		GroupId:   gid,
		ProfileId: "off",
	}, pm)

	grpUpdatedBytes2 := waitForMessage(t, msgChan, "group-updated", 2*time.Second)
	var grpUpdated2 struct {
		Msg  string        `json:"msg"`
		Data structs.Group `json:"data"`
	}
	if err := json.Unmarshal(grpUpdatedBytes2, &grpUpdated2); err != nil {
		t.Fatalf("failed to unmarshal group-updated: %v", err)
	}
	if grpUpdated2.Data.ActiveRoutingProfileId != "" {
		t.Errorf("expected active routing profile to be empty after 'off', got %q", grpUpdated2.Data.ActiveRoutingProfileId)
	}

	// Reactivate profile
	cmd.SetActiveRoutingProfile(structs.SetActiveRoutingProfileData{
		GroupId:   gid,
		ProfileId: rp.Id,
	}, pm)
	grpUpdatedBytes3 := waitForMessage(t, msgChan, "group-updated", 2*time.Second)
	var grpUpdated3 struct {
		Msg  string        `json:"msg"`
		Data structs.Group `json:"data"`
	}
	if err := json.Unmarshal(grpUpdatedBytes3, &grpUpdated3); err != nil {
		t.Fatalf("failed to unmarshal group-updated: %v", err)
	}
	if grpUpdated3.Data.ActiveRoutingProfileId != rp.Id {
		t.Errorf("expected active routing profile to be %q, got %q", rp.Id, grpUpdated3.Data.ActiveRoutingProfileId)
	}

	// 5. Delete routing profile
	cmd.DeleteRoutingProfile(structs.DeleteRoutingProfileData{
		GroupId:   gid,
		ProfileId: rp.Id,
	}, pm)

	subUpdatedBytes2 := waitForMessage(t, msgChan, "subscription-updated", 2*time.Second)
	var subUpdated2 struct {
		Msg  string                      `json:"msg"`
		Data structs.SubscriptionUpdated `json:"data"`
	}
	if err := json.Unmarshal(subUpdatedBytes2, &subUpdated2); err != nil {
		t.Fatalf("failed to unmarshal subscription-updated: %v", err)
	}
	if len(subUpdated2.Data.RoutingProfiles) != 0 {
		t.Errorf("expected 0 routing profiles after delete, got %d", len(subUpdated2.Data.RoutingProfiles))
	}
	if subUpdated2.Data.Group.ActiveRoutingProfileId != "" {
		t.Errorf("expected active routing profile to be empty after delete, got %q", subUpdated2.Data.Group.ActiveRoutingProfileId)
	}
}
