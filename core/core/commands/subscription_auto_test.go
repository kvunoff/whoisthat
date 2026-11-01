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
	"whoisthat-core/lib"
	"whoisthat-core/structs"
)

func boolPtr(v bool) *bool    { return &v }
func int64Ptr(v int64) *int64 { return &v }

func TestParseUpdateIntervalValue(t *testing.T) {
	cases := []struct {
		in     string
		want   int64
		action updateIntervalAction
	}{
		{"6", 6 * 3600, updateIntervalSet},
		{" 12 ", 12 * 3600, updateIntervalSet},
		{`"24"`, 24 * 3600, updateIntervalSet},
		{"0", 0, updateIntervalClear},
		{"-1", 0, updateIntervalKeep},
		{"8761", 0, updateIntervalKeep},
		{"abc", 0, updateIntervalKeep},
		{"", 0, updateIntervalKeep},
	}
	for _, c := range cases {
		got, action := parseUpdateIntervalValue(c.in)
		if got != c.want || action != c.action {
			t.Errorf("parseUpdateIntervalValue(%q) = (%d,%v), want (%d,%v)", c.in, got, action, c.want, c.action)
		}
	}
}

func TestParseProfileUpdateIntervalHeaderAndBody(t *testing.T) {
	withHeader := &http.Response{Header: http.Header{}}
	withHeader.Header.Set("profile-update-interval", "6")
	if got, action := parseProfileUpdateInterval(withHeader, ""); got != 6*3600 || action != updateIntervalSet {
		t.Errorf("header interval = (%d,%v), want (21600,set)", got, action)
	}

	zeroHeader := &http.Response{Header: http.Header{}}
	zeroHeader.Header.Set("profile-update-interval", "0")
	if got, action := parseProfileUpdateInterval(zeroHeader, ""); got != 0 || action != updateIntervalClear {
		t.Errorf("zero header = (%d,%v), want (0,clear)", got, action)
	}

	invalid := &http.Response{Header: http.Header{}}
	invalid.Header.Set("profile-update-interval", "junk")

	body := "#profile-title: Test\n#profile-update-interval: 12\nvless://x\n"
	if got, action := parseProfileUpdateInterval(nil, body); got != 12*3600 || action != updateIntervalSet {
		t.Errorf("body interval = (%d,%v), want (43200,set)", got, action)
	}
	if got, action := parseProfileUpdateInterval(withHeader, body); got != 6*3600 || action != updateIntervalSet {
		t.Errorf("header must win over body: got (%d,%v), want (21600,set)", got, action)
	}
	if got, action := parseProfileUpdateInterval(invalid, body); got != 12*3600 || action != updateIntervalSet {
		t.Errorf("invalid header must fall back to body: got (%d,%v), want (43200,set)", got, action)
	}

	encoded := base64.StdEncoding.EncodeToString([]byte(body))
	if got, action := parseProfileUpdateInterval(nil, encoded); got != 12*3600 || action != updateIntervalSet {
		t.Errorf("base64 body interval = (%d,%v), want (43200,set)", got, action)
	}

	if _, action := parseProfileUpdateInterval(nil, ""); action != updateIntervalClear {
		t.Errorf("absent interval action = %v, want clear", action)
	}

	if _, action := parseProfileUpdateInterval(invalid, ""); action != updateIntervalKeep {
		t.Errorf("invalid header action = %v, want keep", action)
	}
}

func TestSubscriptionDue(t *testing.T) {
	now := time.Unix(1_000_000, 0)
	base := structs.Group{Id: 1, SubscriptionUrl: "https://example.test/sub", SubUpdateInterval: 3600}

	if subscriptionDue(structs.Group{}, now) {
		t.Error("group without URL must not be due")
	}
	disabled := base
	disabled.SubAutoUpdateDisabled = true
	if subscriptionDue(disabled, now) {
		t.Error("disabled group must not be due")
	}
	noInterval := base
	noInterval.SubUpdateInterval = 0
	if subscriptionDue(noInterval, now) {
		t.Error("group without interval must not be due")
	}
	fresh := base
	fresh.SubLastUpdated = now.Unix() - 1800
	if subscriptionDue(fresh, now) {
		t.Error("group refreshed 30m ago must not be due for a 1h interval")
	}
	due := base
	due.SubLastUpdated = now.Unix() - 3600
	if !subscriptionDue(due, now) {
		t.Error("group refreshed 1h ago must be due")
	}
	never := base
	never.SubLastUpdated = 0
	if !subscriptionDue(never, now) {
		t.Error("group never refreshed must be due when interval is known")
	}
	override := base
	override.SubUpdateInterval = 24 * 3600
	override.SubUpdateIntervalOverride = 3600
	override.SubLastUpdated = now.Unix() - 2*3600
	if !subscriptionDue(override, now) {
		t.Error("manual override must win over provider interval")
	}
}

func TestBackoffDuration(t *testing.T) {
	cases := map[int]time.Duration{
		1:  30 * time.Minute,
		2:  time.Hour,
		3:  2 * time.Hour,
		4:  4 * time.Hour,
		5:  6 * time.Hour,
		20: 6 * time.Hour,
	}
	for failures, want := range cases {
		if got := backoffDuration(failures); got != want {
			t.Errorf("backoffDuration(%d) = %v, want %v", failures, got, want)
		}
	}
}

func TestPlanAutoUpdate(t *testing.T) {
	now := time.Unix(1_000_000, 0)
	groups := []structs.Group{
		{Id: 1, SubscriptionUrl: "u", SubUpdateInterval: 3600, SubLastUpdated: now.Unix() - 7200},
		{Id: 2, SubscriptionUrl: "u", SubLastUpdated: now.Unix() - 7200},
		{Id: 3, SubscriptionUrl: "u", SubAutoUpdateDisabled: true},
		{Id: 4, SubscriptionUrl: "u", SubLastUpdated: 0},
		{Id: 5, SubscriptionUrl: "u", SubUpdateInterval: 3600, SubLastUpdated: now.Unix() - 7200},
	}
	probes := map[int]bool{2: true}
	retry := map[int]retryState{5: {failures: 1, next: now.Add(time.Minute)}}

	targets := planAutoUpdate(groups, now, probes, retry)
	byID := map[int]updateTarget{}
	for _, target := range targets {
		byID[target.groupID] = target
	}
	if byID[1].probe {
		t.Error("normal due group must not be a probe")
	}
	if !byID[2].probe {
		t.Error("unknown interval group must be probed")
	}
	if _, ok := byID[3]; ok {
		t.Error("disabled group must not be updated")
	}
	if !byID[4].probe {
		t.Error("never-refreshed unknown group must be probed")
	}
	if _, ok := byID[5]; ok {
		t.Error("group in backoff must not be updated")
	}

	// A successful probe with no interval leaves SubLastUpdated set and the
	// group is no longer in the probe set: it must stop probing until restart.
	probes = map[int]bool{}
	targets = planAutoUpdate([]structs.Group{{Id: 9, SubscriptionUrl: "u", SubLastUpdated: now.Unix()}}, now, probes, map[int]retryState{})
	if len(targets) != 0 {
		t.Errorf("unknown interval after a successful probe must not re-probe, got %v", targets)
	}
}

func TestUpdateSubscriptionStoresIntervalAndTimestamp(t *testing.T) {
	cmd, pm, msgChan := newTestCmd(t)
	profileURI := "vless://11111111-2222-3333-4444-555555555555@1.2.3.4:443?type=tcp&security=none#S\n"
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("profile-update-interval", "6")
		w.Write([]byte(base64.StdEncoding.EncodeToString([]byte(profileURI))))
	}))
	defer ts.Close()

	added, err := cmd.DB.AddGroup("", ts.URL)
	if err != nil {
		t.Fatal(err)
	}
	cmd.UpdateSubscription(structs.UpdateSubscriptionData{GroupId: added.Id}, pm)

	waitForMessage(t, msgChan, "subscription-updated", 3*time.Second)
	group, err := cmd.DB.LoadGroupConfig(added.Id)
	if err != nil {
		t.Fatal(err)
	}
	if group.SubUpdateInterval != 6*3600 {
		t.Errorf("SubUpdateInterval = %d, want %d", group.SubUpdateInterval, 6*3600)
	}
	if group.SubLastUpdated == 0 {
		t.Error("SubLastUpdated must be set on success")
	}
}

func TestAutoUpdateBroadcastMarksBackground(t *testing.T) {
	cmd, pm, msgChan := newTestCmd(t)
	profileURI := "vless://11111111-2222-3333-4444-555555555555@1.2.3.4:443?type=tcp&security=none#S\n"
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(base64.StdEncoding.EncodeToString([]byte(profileURI))))
	}))
	defer ts.Close()

	added, err := cmd.DB.AddGroup("", ts.URL)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := cmd.DB.MergeSubscriptionFields(added.Id, db.SubscriptionFields{ProviderInterval: int64Ptr(3600)}); err != nil {
		t.Fatal(err)
	}
	ok, attempted := cmd.AutoUpdateSubscription(structs.UpdateSubscriptionData{GroupId: added.Id}, pm, false)
	if !ok || !attempted {
		t.Fatalf("AutoUpdateSubscription = (ok=%v, attempted=%v), want (true,true)", ok, attempted)
	}
	raw := waitForMessage(t, msgChan, "subscription-updated", 3*time.Second)
	var updated struct {
		Data structs.SubscriptionUpdated `json:"data"`
	}
	if err := json.Unmarshal(raw, &updated); err != nil {
		t.Fatal(err)
	}
	if !updated.Data.Auto {
		t.Error("background refresh must set auto=true")
	}
}

func TestUpdateSubscriptionFailureKeepsTimestampAndProfiles(t *testing.T) {
	cmd, pm, msgChan := newTestCmd(t)
	profileURI := "vless://11111111-2222-3333-4444-555555555555@1.2.3.4:443?type=tcp&security=none#S\n"
	body := base64.StdEncoding.EncodeToString([]byte(profileURI))
	var modeMu sync.Mutex
	mode := "ok"
	setMode := func(m string) {
		modeMu.Lock()
		mode = m
		modeMu.Unlock()
	}
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		modeMu.Lock()
		current := mode
		modeMu.Unlock()
		switch current {
		case "fail":
			w.WriteHeader(http.StatusInternalServerError)
			return
		case "empty":
			w.Write([]byte(""))
			return
		}
		w.Header().Set("profile-update-interval", "6")
		w.Write([]byte(body))
	}))
	defer ts.Close()

	added, err := cmd.DB.AddGroup("", ts.URL)
	if err != nil {
		t.Fatal(err)
	}
	cmd.UpdateSubscription(structs.UpdateSubscriptionData{GroupId: added.Id}, pm)
	waitForMessage(t, msgChan, "subscription-updated", 3*time.Second)
	group, _ := cmd.DB.LoadGroupConfig(added.Id)
	oldTimestamp := group.SubLastUpdated
	oldInterval := group.SubUpdateInterval
	if oldTimestamp == 0 {
		t.Fatal("expected a successful first update")
	}

	setMode("fail")
	cmd.UpdateSubscription(structs.UpdateSubscriptionData{GroupId: added.Id}, pm)
	group, _ = cmd.DB.LoadGroupConfig(added.Id)
	if group.SubLastUpdated != oldTimestamp || group.SubUpdateInterval != oldInterval {
		t.Errorf("failed fetch changed metadata: last=%d interval=%d", group.SubLastUpdated, group.SubUpdateInterval)
	}

	setMode("empty")
	cmd.UpdateSubscription(structs.UpdateSubscriptionData{GroupId: added.Id}, pm)
	group, _ = cmd.DB.LoadGroupConfig(added.Id)
	if group.SubLastUpdated != oldTimestamp {
		t.Errorf("0-profile refresh advanced timestamp: got %d, want %d", group.SubLastUpdated, oldTimestamp)
	}
	if group.SubUpdateInterval != 0 {
		t.Errorf("provider silence must clear interval, got %d", group.SubUpdateInterval)
	}
	groupData, err := cmd.DB.GetGroupDataAndProfiles(added.Id)
	if err != nil {
		t.Fatal(err)
	}
	if len(groupData.Profiles) != 1 {
		t.Errorf("0-profile refresh removed old profiles: got %d", len(groupData.Profiles))
	}
}

func TestUpdateGroupPersistsAutoUpdateAndOverride(t *testing.T) {
	cmd, pm, _ := newTestCmd(t)
	added, err := cmd.DB.AddGroup("Name", "https://example.test/sub")
	if err != nil {
		t.Fatal(err)
	}

	cmd.UpdateGroup(structs.UpdateGroupData{
		Id:              added.Id,
		Name:            "Name",
		SubscriptionUrl: "https://example.test/sub",
		AutoUpdate:      boolPtr(false),
		UpdateInterval:  int64Ptr(3600),
	}, pm)
	group, _ := cmd.DB.LoadGroupConfig(added.Id)
	if !group.SubAutoUpdateDisabled {
		t.Error("auto-update must be disabled")
	}
	if group.SubUpdateIntervalOverride != 3600 {
		t.Errorf("override = %d, want 3600", group.SubUpdateIntervalOverride)
	}

	cmd.UpdateGroup(structs.UpdateGroupData{
		Id:              added.Id,
		Name:            "Name",
		SubscriptionUrl: "https://example.test/sub",
		UpdateInterval:  int64Ptr(0),
	}, pm)
	group, _ = cmd.DB.LoadGroupConfig(added.Id)
	if group.SubUpdateIntervalOverride != 0 {
		t.Errorf("override must be cleared, got %d", group.SubUpdateIntervalOverride)
	}
}

func TestSubscriptionHeaderFromBody(t *testing.T) {
	if got, ok := lib.SubscriptionHeader("#profile-update-interval: 3\n", "profile-update-interval"); !ok || got != "3" {
		t.Errorf("plain body header = (%q,%v), want (3,true)", got, ok)
	}
	encoded := base64.StdEncoding.EncodeToString([]byte("#profile-update-interval: 24\n"))
	if got, ok := lib.SubscriptionHeader(encoded, "profile-update-interval"); !ok || got != "24" {
		t.Errorf("base64 body header = (%q,%v), want (24,true)", got, ok)
	}
	if _, ok := lib.SubscriptionHeader("no headers here", "profile-update-interval"); ok {
		t.Error("missing header must not be found")
	}
}
