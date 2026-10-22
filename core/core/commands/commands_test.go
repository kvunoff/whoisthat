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
		Msg  string                    `json:"msg"`
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
		Msg  string                    `json:"msg"`
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

