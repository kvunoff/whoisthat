package db

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"whoisthat-core/structs"
)

func newTestDB(t *testing.T) *DB {
	t.Helper()
	db := &DB{Path: t.TempDir()}
	db.loadOrCreateKey()
	db.ensureDBConfigExistance()
	return db
}

// --- Path helpers ---

func TestGetKeyFilePath(t *testing.T) {
	db := &DB{Path: "/tmp/testdb"}
	want := "/tmp/testdb/.key"
	if got := db.GetKeyFilePath(); got != want {
		t.Errorf("GetKeyFilePath() = %q, want %q", got, want)
	}
}

func TestGetGroupDirPath(t *testing.T) {
	db := &DB{Path: "/tmp/testdb"}
	want := filepath.Join("/tmp/testdb", "groups", "5")
	if got := db.GetGroupDirPath(5); got != want {
		t.Errorf("GetGroupDirPath(5) = %q, want %q", got, want)
	}
}

func TestGetGroupConfigFilePath(t *testing.T) {
	db := &DB{Path: "/tmp/testdb"}
	want := filepath.Join("/tmp/testdb", "groups", "3", "group_config.json")
	if got := db.GetGroupConfigFilePath(3); got != want {
		t.Errorf("GetGroupConfigFilePath(3) = %q, want %q", got, want)
	}
}

func TestGetProfileFilePath(t *testing.T) {
	db := &DB{Path: "/tmp/testdb"}
	want := filepath.Join("/tmp/testdb", "groups", "2", "42.json")
	if got := db.GetProfileFilePath(2, 42); got != want {
		t.Errorf("GetProfileFilePath(2, 42) = %q, want %q", got, want)
	}
}

// --- Encrypt/decrypt round-trip ---

func TestWriteReadEncryptedJSON(t *testing.T) {
	db := newTestDB(t)

	type payload struct {
		Name  string `json:"name"`
		Value int    `json:"value"`
	}
	original := payload{Name: "whoisthat", Value: 42}

	path := filepath.Join(db.Path, "test.json")
	if err := db.writeEncryptedJSON(path, original); err != nil {
		t.Fatalf("writeEncryptedJSON: %v", err)
	}

	var result payload
	if err := db.readEncryptedJSON(path, &result); err != nil {
		t.Fatalf("readEncryptedJSON: %v", err)
	}

	if result.Name != original.Name || result.Value != original.Value {
		t.Errorf("round-trip mismatch: got %+v, want %+v", result, original)
	}
}

func TestWriteEncryptedFileIsNotPlainJSON(t *testing.T) {
	db := newTestDB(t)

	type payload struct{ Secret string }
	path := filepath.Join(db.Path, "secret.json")
	_ = db.writeEncryptedJSON(path, payload{Secret: "hunter2"})

	raw, _ := os.ReadFile(path)
	// The file must be a ciphertext wrapper, not plain JSON with the secret
	if json.Valid(raw) {
		var wrapper struct {
			Ciphertext string `json:"ciphertext"`
		}
		_ = json.Unmarshal(raw, &wrapper)
		if wrapper.Ciphertext == "" {
			t.Error("expected ciphertext wrapper, got plain JSON")
		}
	}
}

// --- isFileEncrypted ---

func TestIsFileEncryptedPlainJSON(t *testing.T) {
	db := newTestDB(t)
	path := filepath.Join(db.Path, "plain.json")
	_ = os.WriteFile(path, []byte(`{"name":"test"}`), 0666)
	if db.isFileEncrypted(path) {
		t.Error("plain JSON file should not be detected as encrypted")
	}
}

func TestIsFileEncryptedCiphertext(t *testing.T) {
	db := newTestDB(t)
	path := filepath.Join(db.Path, "enc.json")
	_ = db.writeEncryptedJSON(path, map[string]string{"k": "v"})
	if !db.isFileEncrypted(path) {
		t.Error("encrypted file should be detected as encrypted")
	}
}

func TestIsFileEncryptedMissingFile(t *testing.T) {
	db := newTestDB(t)
	if db.isFileEncrypted(filepath.Join(db.Path, "nonexistent.json")) {
		t.Error("missing file should return false")
	}
}

// --- Key persistence ---

func TestLoadOrCreateKeyCreatesKeyFile(t *testing.T) {
	tmpDir := t.TempDir()
	db := &DB{Path: tmpDir}
	db.loadOrCreateKey()

	keyPath := db.GetKeyFilePath()
	data, err := os.ReadFile(keyPath)
	if err != nil {
		t.Fatalf("key file not created: %v", err)
	}
	if len(data) != 32 {
		t.Errorf("key file length = %d, want 32", len(data))
	}
}

func TestLoadOrCreateKeyReusesExistingKey(t *testing.T) {
	tmpDir := t.TempDir()

	db1 := &DB{Path: tmpDir}
	db1.loadOrCreateKey()
	key1 := make([]byte, 32)
	copy(key1, db1.key)

	db2 := &DB{Path: tmpDir}
	db2.loadOrCreateKey()

	for i := range key1 {
		if key1[i] != db2.key[i] {
			t.Fatal("second loadOrCreateKey() loaded a different key than the first")
		}
	}
}

// TestProfileRoundTripWithTestMetadata verifies the new Profile fields
// (TestedAt, LossPct, JitterMs) survive an encrypted write+read cycle.
// Backward-compat: older rows written without these fields decode cleanly
// into zero values.
func TestProfileRoundTripWithTestMetadata(t *testing.T) {
	db := newTestDB(t)

	original := structs.Profile{
		Id:         42,
		GroupId:    1,
		NanoID:     "abc123",
		Name:       "test",
		Protocol:   "vless",
		Uri:        "vless://example.com",
		Address:    "example.com",
		Host:       "example.com",
		TestResult: 120,
		TestedAt:   1700000000,
		LossPct:    33,
		JitterMs:   12,
	}

	if err := os.MkdirAll(db.GetGroupDirPath(1), 0700); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	path := db.GetProfileFilePath(1, 42)
	if err := db.writeEncryptedJSON(path, original); err != nil {
		t.Fatalf("writeEncryptedJSON: %v", err)
	}

	var got structs.Profile
	if err := db.readEncryptedJSON(path, &got); err != nil {
		t.Fatalf("readEncryptedJSON: %v", err)
	}
	if got.TestResult != 120 {
		t.Errorf("TestResult = %d, want 120", got.TestResult)
	}
	if got.TestedAt != 1700000000 {
		t.Errorf("TestedAt = %d, want 1700000000", got.TestedAt)
	}
	if got.LossPct != 33 {
		t.Errorf("LossPct = %d, want 33", got.LossPct)
	}
	if got.JitterMs != 12 {
		t.Errorf("JitterMs = %d, want 12", got.JitterMs)
	}
}

// TestProfileDecodeLegacyRowWithoutTestMetadata verifies the new fields
// default cleanly to zero when reading a JSON payload written by an
// older build that didn't know about TestedAt/LossPct/JitterMs.
func TestProfileDecodeLegacyRowWithoutTestMetadata(t *testing.T) {
	db := newTestDB(t)
	// Hand-written legacy payload (omitzero fields absent).
	legacy := `{"id":42,"group_id":1,"name":"old","protocol":"vmess","uri":"vmess://","test-result":80}`
	if err := os.MkdirAll(db.GetGroupDirPath(1), 0700); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	path := db.GetProfileFilePath(1, 42)
	// Write plaintext (pre-encryption migration path) to simulate legacy.
	if err := os.WriteFile(path, []byte(legacy), 0600); err != nil {
		t.Fatalf("write legacy: %v", err)
	}

	var got structs.Profile
	if err := db.readEncryptedJSON(path, &got); err != nil {
		t.Fatalf("readEncryptedJSON: %v", err)
	}
	if got.TestResult != 80 {
		t.Errorf("TestResult = %d, want 80", got.TestResult)
	}
	if got.TestedAt != 0 || got.LossPct != 0 || got.JitterMs != 0 {
		t.Errorf("expected zero rich metadata for legacy row, got TestedAt=%d LossPct=%d JitterMs=%d",
			got.TestedAt, got.LossPct, got.JitterMs)
	}
}

func TestReorderProfiles(t *testing.T) {
	db := newTestDB(t)

	groupAdded, err := db.AddGroup("Group A", "")
	if err != nil {
		t.Fatalf("AddGroup: %v", err)
	}
	gid := groupAdded.Id

	p1, err := db.AddProfile(structs.DBAddProfileData{GroupId: gid, Name: "P1", Protocol: "vless", Uri: "vless://1"})
	if err != nil {
		t.Fatalf("AddProfile p1: %v", err)
	}
	p2, err := db.AddProfile(structs.DBAddProfileData{GroupId: gid, Name: "P2", Protocol: "vless", Uri: "vless://2"})
	if err != nil {
		t.Fatalf("AddProfile p2: %v", err)
	}
	p3, err := db.AddProfile(structs.DBAddProfileData{GroupId: gid, Name: "P3", Protocol: "vless", Uri: "vless://3"})
	if err != nil {
		t.Fatalf("AddProfile p3: %v", err)
	}

	// Reverse order: p3, p1, p2
	newOrder := []int{p3.Id, p1.Id, p2.Id}
	if err := db.ReorderProfiles(gid, newOrder); err != nil {
		t.Fatalf("ReorderProfiles: %v", err)
	}

	groups, err := db.GetAllGroupsAndProfiles()
	if err != nil {
		t.Fatalf("GetAllGroupsAndProfiles: %v", err)
	}
	if len(groups) != 1 || len(groups[0].Profiles) != 3 {
		t.Fatalf("unexpected groups count or profiles count: %+v", groups)
	}

	if groups[0].Profiles[0].Id != p3.Id || groups[0].Profiles[1].Id != p1.Id || groups[0].Profiles[2].Id != p2.Id {
		t.Errorf("expected order [%d, %d, %d], got [%d, %d, %d]",
			p3.Id, p1.Id, p2.Id,
			groups[0].Profiles[0].Id, groups[0].Profiles[1].Id, groups[0].Profiles[2].Id)
	}
}

func TestReorderGroups(t *testing.T) {
	db := newTestDB(t)

	g1, err := db.AddGroup("G1", "")
	if err != nil {
		t.Fatalf("AddGroup 1: %v", err)
	}
	g2, err := db.AddGroup("G2", "")
	if err != nil {
		t.Fatalf("AddGroup 2: %v", err)
	}
	g3, err := db.AddGroup("G3", "")
	if err != nil {
		t.Fatalf("AddGroup 3: %v", err)
	}

	newOrder := []int{g3.Id, g1.Id, g2.Id}
	if err := db.ReorderGroups(newOrder); err != nil {
		t.Fatalf("ReorderGroups: %v", err)
	}

	groups, err := db.GetAllGroupsAndProfiles()
	if err != nil {
		t.Fatalf("GetAllGroupsAndProfiles: %v", err)
	}
	if len(groups) != 3 {
		t.Fatalf("expected 3 groups, got %d", len(groups))
	}

	if groups[0].Group.Id != g3.Id || groups[1].Group.Id != g1.Id || groups[2].Group.Id != g2.Id {
		t.Errorf("expected group order [%d, %d, %d], got [%d, %d, %d]",
			g3.Id, g1.Id, g2.Id,
			groups[0].Group.Id, groups[1].Group.Id, groups[2].Group.Id)
	}
}

func TestMoveProfile(t *testing.T) {
	db := newTestDB(t)

	g1, err := db.AddGroup("G1", "")
	if err != nil {
		t.Fatalf("AddGroup 1: %v", err)
	}
	g2, err := db.AddGroup("G2", "")
	if err != nil {
		t.Fatalf("AddGroup 2: %v", err)
	}

	p1, err := db.AddProfile(structs.DBAddProfileData{GroupId: g1.Id, Name: "Profile In G1", Protocol: "vless", Uri: "vless://p1"})
	if err != nil {
		t.Fatalf("AddProfile: %v", err)
	}

	moved, err := db.MoveProfile(g1.Id, g2.Id, p1.Id)
	if err != nil {
		t.Fatalf("MoveProfile: %v", err)
	}

	if moved.GroupId != g2.Id {
		t.Errorf("moved.GroupId = %d, want %d", moved.GroupId, g2.Id)
	}
	if moved.Name != "Profile In G1" {
		t.Errorf("moved.Name = %q, want %q", moved.Name, "Profile In G1")
	}

	// Verify old profile is gone
	groups, err := db.GetAllGroupsAndProfiles()
	if err != nil {
		t.Fatalf("GetAllGroupsAndProfiles: %v", err)
	}

	var g1Found, g2Found *structs.GroupWithProfiles
	for i := range groups {
		if groups[i].Group.Id == g1.Id {
			g1Found = &groups[i]
		}
		if groups[i].Group.Id == g2.Id {
			g2Found = &groups[i]
		}
	}

	if len(g1Found.Profiles) != 0 {
		t.Errorf("expected 0 profiles in g1, got %d", len(g1Found.Profiles))
	}
	if len(g2Found.Profiles) != 1 || g2Found.Profiles[0].Id != moved.Id {
		t.Errorf("expected 1 profile in g2 with id %d, got %+v", moved.Id, g2Found.Profiles)
	}
}

