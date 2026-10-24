package happ

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
	"whoisthat-core/structs"
)

func TestEnsureProfileGeo_Success(t *testing.T) {
	database := newTestDB(t)
	addedGroup, err := database.AddGroup("GeoGroup", "")
	if err != nil {
		t.Fatalf("AddGroup failed: %v", err)
	}
	groupID := addedGroup.Id

	// Generate 1.1 MB dummy content (above 1 MB minimum)
	dummyData := make([]byte, 1100*1024)
	for i := range dummyData {
		dummyData[i] = 'A'
	}

	ts := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write(dummyData)
	}))
	defer ts.Close()

	// Configure http.DefaultTransport to trust test server's TLS certificate
	http.DefaultTransport = ts.Client().Transport

	profileRaw := HappProfile{
		Name:       "TestGeoProfile",
		Geoipurl:   ts.URL + "/geoip.dat?token=secret123",
		Geositeurl: ts.URL + "/geosite.dat?token=secret456",
	}
	rawBytes, _ := json.Marshal(profileRaw)

	profile := structs.RoutingProfile{
		Id:        "p_geo1",
		GroupId:   groupID,
		Name:      "TestGeoProfile",
		Source:    "happ",
		RawJSON:   rawBytes,
		State:     "updating",
		CreatedAt: time.Now().Unix(),
	}
	if err := database.SaveRoutingProfile(profile); err != nil {
		t.Fatalf("SaveRoutingProfile failed: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	err = EnsureProfileGeo(ctx, database, &profile)
	if err != nil {
		t.Fatalf("EnsureProfileGeo failed: %v", err)
	}

	if profile.State != "ready" {
		t.Errorf("expected state ready, got %s", profile.State)
	}
	if profile.GeoDir == "" {
		t.Errorf("expected non-empty GeoDir")
	}

	absGeoDir := filepath.Join(database.Path, profile.GeoDir)
	if _, err := os.Stat(filepath.Join(absGeoDir, "geoip.dat")); err != nil {
		t.Errorf("geoip.dat missing: %v", err)
	}
	if _, err := os.Stat(filepath.Join(absGeoDir, "geosite.dat")); err != nil {
		t.Errorf("geosite.dat missing: %v", err)
	}

	// Test weekly gate: running EnsureProfileGeo again should skip download
	geoUpdatedInitial := profile.GeoUpdatedAt
	err = EnsureProfileGeo(ctx, database, &profile)
	if err != nil {
		t.Fatalf("second EnsureProfileGeo failed: %v", err)
	}
	if profile.GeoUpdatedAt != geoUpdatedInitial {
		t.Errorf("expected weekly gate to skip update, but GeoUpdatedAt changed")
	}
}

func TestEnsureProfileGeo_RejectsHTTP(t *testing.T) {
	database := newTestDB(t)
	addedGroup, _ := database.AddGroup("GeoGroup", "")
	groupID := addedGroup.Id

	profileRaw := HappProfile{
		Name:     "InsecureProfile",
		Geoipurl: "http://insecure.example.com/geoip.dat?key=secret",
	}
	rawBytes, _ := json.Marshal(profileRaw)

	profile := structs.RoutingProfile{
		Id:        "p_insecure",
		GroupId:   groupID,
		Name:      "InsecureProfile",
		Source:    "happ",
		RawJSON:   rawBytes,
		State:     "updating",
		CreatedAt: time.Now().Unix(),
	}
	_ = database.SaveRoutingProfile(profile)

	ctx := context.Background()
	err := EnsureProfileGeo(ctx, database, &profile)
	if err == nil {
		t.Fatalf("expected error for http URL")
	}

	if profile.State != "error" {
		t.Errorf("expected state error, got %s", profile.State)
	}
	// Check that sensitive query token is masked
	if strings.Contains(profile.LastError, "secret") {
		t.Errorf("LastError contains unmasked token: %s", profile.LastError)
	}
}

func TestEnsureProfileGeo_RejectsSmallFile(t *testing.T) {
	database := newTestDB(t)
	addedGroup, _ := database.AddGroup("GeoGroup", "")
	groupID := addedGroup.Id

	// Only 500 bytes (below 1 KB min)
	smallData := make([]byte, 500)

	ts := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write(smallData)
	}))
	defer ts.Close()
	http.DefaultTransport = ts.Client().Transport

	profileRaw := HappProfile{
		Name:     "SmallFileProfile",
		Geoipurl: ts.URL + "/geoip.dat",
	}
	rawBytes, _ := json.Marshal(profileRaw)

	profile := structs.RoutingProfile{
		Id:        "p_small",
		GroupId:   groupID,
		Name:      "SmallFileProfile",
		Source:    "happ",
		RawJSON:   rawBytes,
		State:     "updating",
		CreatedAt: time.Now().Unix(),
	}
	_ = database.SaveRoutingProfile(profile)

	ctx := context.Background()
	err := EnsureProfileGeo(ctx, database, &profile)
	if err == nil {
		t.Fatalf("expected error for small file (< 1 KB)")
	}
	if profile.State != "error" {
		t.Errorf("expected state error, got %s", profile.State)
	}
}

func TestEnsureProfileGeo_RejectsHTMLFile(t *testing.T) {
	database := newTestDB(t)
	addedGroup, _ := database.AddGroup("GeoGroup", "")
	groupID := addedGroup.Id

	htmlData := []byte("<!DOCTYPE html><html><body>Error 404 Not Found</body></html>")

	ts := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write(htmlData)
	}))
	defer ts.Close()
	http.DefaultTransport = ts.Client().Transport

	profileRaw := HappProfile{
		Name:     "HTMLProfile",
		Geoipurl: ts.URL + "/geoip.dat",
	}
	rawBytes, _ := json.Marshal(profileRaw)

	profile := structs.RoutingProfile{
		Id:        "p_html",
		GroupId:   groupID,
		Name:      "HTMLProfile",
		Source:    "happ",
		RawJSON:   rawBytes,
		State:     "updating",
		CreatedAt: time.Now().Unix(),
	}
	_ = database.SaveRoutingProfile(profile)

	ctx := context.Background()
	err := EnsureProfileGeo(ctx, database, &profile)
	if err == nil {
		t.Fatalf("expected error for HTML file")
	}
	if profile.State != "error" {
		t.Errorf("expected state error, got %s", profile.State)
	}
}
