package appconfig

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"
)

// setupConfigDir isolates XDG_CONFIG_HOME and restores the package globals.
func setupConfigDir(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	configDir := filepath.Join(dir, "whoisthat")
	if err := os.MkdirAll(configDir, 0700); err != nil {
		t.Fatal(err)
	}
	oldCfg, oldStatus := application_configuration, loadStatus
	t.Cleanup(func() {
		application_configuration, loadStatus = oldCfg, oldStatus
	})
	return configDir
}

func TestLoadConfigKeepsGoodFieldsFromPartiallyCorruptFile(t *testing.T) {
	dir := setupConfigDir(t)
	path := filepath.Join(dir, "config.json")
	raw := []byte(`{"tun-name":"mytun","autoconnect-enabled":true,"dns-servers":"not-an-array"}`)
	if err := os.WriteFile(path, raw, 0600); err != nil {
		t.Fatal(err)
	}

	LoadConfig()

	if got := GetConfig().TunName; got != "mytun" {
		t.Errorf("TunName = %q, want mytun (a bad sibling field must not reset it)", got)
	}
	if !GetConfig().AutoconnectEnabled {
		t.Error("AutoconnectEnabled = false, want true")
	}
	if got := Status().State; got != "corrupt" {
		t.Errorf("Status().State = %q, want corrupt", got)
	}
	after, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(after, raw) {
		t.Error("config.json must not be rewritten when it could not be fully parsed")
	}
}

func TestLoadConfigRestoresLastKnownGoodCopy(t *testing.T) {
	dir := setupConfigDir(t)
	path := filepath.Join(dir, "config.json")
	backup := []byte(`{"tun-name":"from-backup"}`)
	if err := os.WriteFile(path+".bak", backup, 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("not json at all"), 0600); err != nil {
		t.Fatal(err)
	}

	LoadConfig()

	if got := Status().State; got != "repaired" {
		t.Errorf("Status().State = %q, want repaired", got)
	}
	if got := GetConfig().TunName; got != "from-backup" {
		t.Errorf("TunName = %q, want from-backup", got)
	}
	after, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(after, backup) {
		t.Error("config.json should be replaced by the backup")
	}
}

func TestLoadConfigCreatesConfigOnFirstRun(t *testing.T) {
	dir := setupConfigDir(t)

	LoadConfig()

	// The fresh file is written and immediately usable, so a first start is
	// not reported as a problem.
	if got := Status().State; got != "ok" {
		t.Errorf("Status().State = %q, want ok", got)
	}
	if _, err := os.Stat(filepath.Join(dir, "config.json")); err != nil {
		t.Errorf("config.json was not created on first run: %v", err)
	}
}

func TestLoadConfigRefreshesBackupOnEveryGoodLoad(t *testing.T) {
	dir := setupConfigDir(t)
	path := filepath.Join(dir, "config.json")
	raw := []byte(`{"tun-name":"good"}`)
	if err := os.WriteFile(path, raw, 0600); err != nil {
		t.Fatal(err)
	}

	LoadConfig()

	saved, err := os.ReadFile(path + ".bak")
	if err != nil {
		t.Fatalf("backup not written: %v", err)
	}
	if !bytes.Equal(saved, raw) {
		t.Errorf("backup = %q, want %q", saved, raw)
	}
}

func TestRestoreFromBackupReportsMissingBackup(t *testing.T) {
	setupConfigDir(t)

	if err := RestoreFromBackup(); err == nil {
		t.Error("RestoreFromBackup should fail when no backup exists")
	}
}
