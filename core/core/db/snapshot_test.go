package db

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"whoisthat-core/lib/crypto"
	"whoisthat-core/structs"
)

func TestUndecryptableDatabaseBecomesReadOnly(t *testing.T) {
	d := newTestDB(t)

	// Write a file that the database key cannot open.
	otherKey, err := crypto.GenerateKey()
	if err != nil {
		t.Fatal(err)
	}
	encoded, err := crypto.EncryptToBase64([]byte(`{"mode":"custom"}`), otherKey)
	if err != nil {
		t.Fatal(err)
	}
	wrapper, err := json.Marshal(struct {
		Ciphertext string `json:"ciphertext"`
	}{encoded})
	if err != nil {
		t.Fatal(err)
	}
	path := d.GetRoutingFilePath()
	if err := os.WriteFile(path, wrapper, 0600); err != nil {
		t.Fatal(err)
	}

	var cfg structs.RoutingConfig
	if err := d.readEncryptedJSON(path, &cfg); err == nil {
		t.Fatal("expected a decrypt error")
	}
	if got := d.Status().State; got != "read-only" {
		t.Fatalf("Status().State = %q, want read-only", got)
	}

	if err := d.SaveRouting(structs.RoutingConfig{}); err == nil {
		t.Fatal("SaveRouting must fail while the database is read-only")
	}
	after, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(after) != string(wrapper) {
		t.Error("the undecryptable file must be left untouched")
	}
}

func TestSnapshotDailyIsIdempotentAndRestorable(t *testing.T) {
	root := t.TempDir()
	d := &DB{Path: filepath.Join(root, "db"), KeyPath: filepath.Join(root, "db.key")}
	d.Initialize()

	day := d.Status().SnapshotAt
	if day == "" {
		t.Fatal("no daily snapshot created")
	}

	// The snapshot must not be re-created over an existing one.
	d.snapshotDaily()
	if got := d.Status().SnapshotAt; got != day {
		t.Fatalf("SnapshotAt = %q, want %q", got, day)
	}

	if err := os.RemoveAll(d.Path); err != nil {
		t.Fatal(err)
	}
	restored, err := d.RestoreSnapshot()
	if err != nil {
		t.Fatalf("RestoreSnapshot: %v", err)
	}
	if restored != day {
		t.Errorf("restored day = %q, want %q", restored, day)
	}
	if _, err := os.Stat(d.GetDBConfigFile()); err != nil {
		t.Errorf("db config was not restored: %v", err)
	}
}
