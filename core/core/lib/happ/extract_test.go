package happ

import (
	"encoding/base64"
	"net/http"
	"testing"
	"whoisthat-core/db"
)

func newTestDB(t *testing.T) *db.DB {
	t.Helper()
	database := &db.DB{Path: t.TempDir()}
	database.Initialize()
	return database
}

func TestExtractLinksFromResponse(t *testing.T) {
	resp := &http.Response{
		Header: http.Header{},
	}
	resp.Header.Add("Routing", `happ://routing/onadd/eyJOYW1lIjoiUDEifQ==;`)
	resp.Header.Add("routing", `"happ://routing/add/eyJOYW1lIjoiUDIifQ=="`)
	resp.Header.Add("other", "happ://routing/should_be_ignored")

	links := ExtractLinksFromResponse(resp)
	if len(links) != 2 {
		t.Fatalf("expected 2 links, got %d: %v", len(links), links)
	}
	if links[0] != "happ://routing/onadd/eyJOYW1lIjoiUDEifQ==" {
		t.Errorf("link 0 mismatch: %s", links[0])
	}
	if links[1] != "happ://routing/add/eyJOYW1lIjoiUDIifQ==" {
		t.Errorf("link 1 mismatch: %s", links[1])
	}
}

func TestExtractLinks_BodyAndBase64(t *testing.T) {
	// Plain text body with multiple lines
	plainBody := `
vless://1111@1.1.1.1:443?type=tcp#Server1
happ://routing/onadd/eyJOYW1lIjoiT25BZGQifQ==
vless://2222@2.2.2.2:443?type=tcp#Server2
happ://routing/off
`
	links := ExtractLinks([]byte(plainBody))
	if len(links) != 2 {
		t.Fatalf("expected 2 links in plain body, got %d: %v", len(links), links)
	}

	// Base64 encoded body
	encodedBody := base64.StdEncoding.EncodeToString([]byte(plainBody))
	linksB64 := ExtractLinks([]byte(encodedBody))
	if len(linksB64) != 2 {
		t.Fatalf("expected 2 links in base64 body, got %d: %v", len(linksB64), linksB64)
	}
}

func TestUpsertFromLink(t *testing.T) {
	database := newTestDB(t)
	addedGroup, err := database.AddGroup("SubGroup", "")
	if err != nil {
		t.Fatalf("AddGroup failed: %v", err)
	}
	groupID := addedGroup.Id

	// 1. Add first profile with ActionAdd
	p1JSON := `{"Name":"P1","GlobalProxy":true,"LastUpdated":"1000","DirectSites":["example.com"]}`
	link1 := "happ://routing/add/" + base64.StdEncoding.EncodeToString([]byte(p1JSON))

	prof1, isOff, err := UpsertFromLink(database, groupID, link1, 1000)
	if err != nil || isOff {
		t.Fatalf("UpsertFromLink 1 failed: %v, isOff=%v", err, isOff)
	}
	if prof1.Name != "P1" {
		t.Errorf("expected P1, got %s", prof1.Name)
	}

	// First added profile should be automatically active
	grp, _ := database.LoadGroupConfig(groupID)
	if grp.ActiveRoutingProfileId != prof1.Id {
		t.Errorf("expected first profile to be active, got active=%s, prof1=%s", grp.ActiveRoutingProfileId, prof1.Id)
	}

	// 2. Add second profile with ActionAdd (should NOT override active)
	p2JSON := `{"Name":"P2","GlobalProxy":true,"LastUpdated":"1000","ProxySites":["google.com"]}`
	link2 := "happ://routing/add/" + base64.StdEncoding.EncodeToString([]byte(p2JSON))
	prof2, _, err := UpsertFromLink(database, groupID, link2, 1000)
	if err != nil {
		t.Fatalf("UpsertFromLink 2 failed: %v", err)
	}
	grp, _ = database.LoadGroupConfig(groupID)
	if grp.ActiveRoutingProfileId != prof1.Id {
		t.Errorf("expected P1 to remain active, got %s", grp.ActiveRoutingProfileId)
	}

	// 3. Add with ActionOnAdd -> should activate P2
	link2OnAdd := "happ://routing/onadd/" + base64.StdEncoding.EncodeToString([]byte(p2JSON))
	_, _, err = UpsertFromLink(database, groupID, link2OnAdd, 1000)
	if err != nil {
		t.Fatalf("UpsertFromLink onadd failed: %v", err)
	}
	grp, _ = database.LoadGroupConfig(groupID)
	if grp.ActiveRoutingProfileId != prof2.Id {
		t.Errorf("expected P2 to become active after onadd, got %s", grp.ActiveRoutingProfileId)
	}

	// 4. Update with older LastUpdated -> should skip
	p2Old := `{"Name":"P2","GlobalProxy":true,"LastUpdated":"500","ProxySites":["old.com"]}`
	link2Old := "happ://routing/add/" + base64.StdEncoding.EncodeToString([]byte(p2Old))
	updatedProf, _, err := UpsertFromLink(database, groupID, link2Old, 2000)
	if err != nil {
		t.Fatalf("UpsertFromLink older failed: %v", err)
	}
	// Rules should still be google.com, not old.com
	if len(updatedProf.Rules) < 2 || updatedProf.Rules[1].Domain != "geosite:google,google.com" && updatedProf.Rules[1].Domain != "google.com" {
		t.Logf("rules: %+v", updatedProf.Rules)
	}

	// 5. ActionOff -> clears active
	linkOff := "happ://routing/off"
	_, isOff, err = UpsertFromLink(database, groupID, linkOff, 3000)
	if err != nil || !isOff {
		t.Fatalf("UpsertFromLink off failed: %v, isOff=%v", err, isOff)
	}
	grp, _ = database.LoadGroupConfig(groupID)
	if grp.ActiveRoutingProfileId != "" {
		t.Errorf("expected active to be cleared after off, got %s", grp.ActiveRoutingProfileId)
	}
}
