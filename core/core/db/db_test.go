package db

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func newTestDB(t *testing.T) *DB {
	t.Helper()
	db := &DB{Path: t.TempDir()}
	db.loadOrCreateKey()
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
