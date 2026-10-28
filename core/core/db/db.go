package db

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"sync"
	"time"
	"whoisthat-core/lib/crypto"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

type DB struct {
	Path string
	// KeyPath overrides the encryption-key location. When empty the legacy
	// per-database path (<Path>/.key) is used, which keeps tests and tools
	// that construct a bare DB working.
	KeyPath string
	key     []byte
	mu      sync.Mutex
}

func (db *DB) saveDBConfig(db_config structs.DBConfig) error {
	return db.writeEncryptedJSON(db.GetDBConfigFile(), db_config)
}

func (db *DB) loadDBConfig() (structs.DBConfig, error) {
	var db_config_data structs.DBConfig
	if err := db.readEncryptedJSON(db.GetDBConfigFile(), &db_config_data); err != nil {
		return db_config_data, err
	}
	return db_config_data, nil
}

const encryptionKeySize = 32

func (db *DB) loadOrCreateKey() {
	preferred := db.GetKeyFilePath()
	legacy := filepath.Join(db.Path, ".key")

	prefKey, prefValid, prefExists := readKeyFile(preferred)
	legacyKey, legacyValid, legacyExists := readKeyFile(legacy)

	// Legacy per-database layout (tests, custom DB paths): the two paths are
	// the same file, so there is no migration to perform.
	if preferred == legacy {
		if prefValid {
			db.key = prefKey
			_ = os.Chmod(preferred, 0600)
			return
		}
		if prefExists {
			logger.Fatal("encryption key " + preferred + " is invalid; refusing to overwrite it")
		}
		db.generateKey(preferred)
		return
	}

	switch {
	case prefValid && legacyValid:
		if bytes.Equal(prefKey, legacyKey) {
			db.key = prefKey
			db.removeLegacyKey(legacy)
			return
		}
		// Both files exist but disagree. This happens after a failed migration
		// or a stale key created by an older build. Pick the key that can
		// actually decrypt the existing database instead of silently showing
		// an empty profile list.
		prefMatches, checked := db.keyMatchesDatabase(prefKey)
		if prefMatches {
			db.key = prefKey
			_ = os.Chmod(preferred, 0600)
			db.removeLegacyKey(legacy)
			return
		}
		legacyMatches, _ := db.keyMatchesDatabase(legacyKey)
		if legacyMatches {
			db.adoptLegacyKey(preferred, prefKey, legacyKey)
			db.key = legacyKey
			db.removeLegacyKey(legacy)
			return
		}
		if checked {
			// Keep both keys: the database is unreadable, and the legacy file
			// may still be the only way to recover it.
			logger.Warnf("db: neither %s nor %s decrypts the existing database; keeping both keys", preferred, legacy)
		} else {
			logger.Warnf("db: %s and %s differ but no encrypted database was found; keeping %s", preferred, legacy, preferred)
			db.removeLegacyKey(legacy)
		}
		db.key = prefKey

	case prefValid:
		db.key = prefKey
		_ = os.Chmod(preferred, 0600)
		if legacyExists {
			db.removeLegacyKey(legacy)
		}

	case legacyValid:
		// First start after the key moved out of the DB directory.
		if err := utils.AtomicWriteFile(preferred, legacyKey, 0600); err != nil {
			logger.Fatal("failed to migrate key file to " + preferred + ": " + err.Error())
		}
		logger.Infof("db: migrated encryption key from %s to %s", legacy, preferred)
		db.key = legacyKey
		db.removeLegacyKey(legacy)

	case prefExists || legacyExists:
		logger.Fatal("encryption key is invalid; refusing to overwrite it. " +
			"Restore the key from a backup or remove the database and key together.")

	default:
		db.generateKey(preferred)
	}
}

// readKeyFile returns the key bytes, whether the length is valid, and whether
// the file exists at all. Unreadable files are fatal: silently generating a
// replacement would make the existing database permanently unreadable.
func readKeyFile(path string) ([]byte, bool, bool) {
	data, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return nil, false, false
	}
	if err != nil {
		logger.Fatal("failed to read key file " + path + ": " + err.Error())
	}
	if len(data) != encryptionKeySize {
		logger.Warnf("db: key file %s has invalid length %d (expected %d)", path, len(data), encryptionKeySize)
		return data, false, true
	}
	return data, true, true
}

// keyMatchesDatabase reports whether key decrypts an existing encrypted DB
// file, and whether any encrypted file was available to test.
func (db *DB) keyMatchesDatabase(key []byte) (bool, bool) {
	checked := false
	for _, path := range []string{db.GetDBConfigFile(), db.GetRoutingFilePath()} {
		data, err := os.ReadFile(path)
		if err != nil {
			continue
		}
		var wrapper struct {
			Ciphertext string `json:"ciphertext"`
		}
		if json.Unmarshal(data, &wrapper) != nil || wrapper.Ciphertext == "" {
			continue
		}
		checked = true
		if _, err := crypto.DecryptBase64(wrapper.Ciphertext, key); err == nil {
			return true, true
		}
	}
	return false, checked
}

// removeLegacyKey removes the old co-located key after it has been migrated,
// so only one authoritative key remains on disk.
func (db *DB) removeLegacyKey(path string) {
	if path == "" || path == db.GetKeyFilePath() {
		return
	}
	if err := os.Remove(path); err != nil {
		if !os.IsNotExist(err) {
			logger.Warnf("db: failed to remove legacy key %s: %v", path, err)
		}
		return
	}
	logger.Infof("db: removed legacy key %s", path)
}

func (db *DB) adoptLegacyKey(preferred string, staleKey, legacyKey []byte) {
	backup := preferred + ".stale-" + time.Now().Format("20060102-150405")
	if len(staleKey) > 0 {
		_ = os.WriteFile(backup, staleKey, 0600)
	}
	if err := utils.AtomicWriteFile(preferred, legacyKey, 0600); err != nil {
		logger.Fatal("failed to restore legacy key to " + preferred + ": " + err.Error())
	}
	logger.Warnf("db: %s did not decrypt the database; restored the legacy key (stale key saved to %s)", preferred, backup)
}

func (db *DB) generateKey(path string) {
	key, err := crypto.GenerateKey()
	if err != nil {
		logger.Fatal("failed to generate encryption key: " + err.Error())
	}
	if err := utils.AtomicWriteFile(path, key, 0600); err != nil {
		logger.Fatal("failed to write key file " + path + ": " + err.Error())
	}
	db.key = key
}

func (db *DB) GetKeyFilePath() string {
	if db.KeyPath != "" {
		return db.KeyPath
	}
	return filepath.Join(db.Path, ".key")
}

func (db *DB) readEncryptedJSON(path string, target any) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return err
	}

	var wrapper struct {
		Ciphertext string `json:"ciphertext"`
	}
	if json.Unmarshal(data, &wrapper) == nil && wrapper.Ciphertext != "" {
		plain, err := crypto.DecryptBase64(wrapper.Ciphertext, db.key)
		if err != nil {
			return fmt.Errorf("decrypt %s: %w", path, err)
		}
		return json.Unmarshal(plain, target)
	}

	return json.Unmarshal(data, target)
}

func (db *DB) writeEncryptedJSON(path string, data any) error {
	jsonData, err := json.MarshalIndent(data, "", " ")
	if err != nil {
		return err
	}

	encoded, err := crypto.EncryptToBase64(jsonData, db.key)
	if err != nil {
		return fmt.Errorf("encrypt: %w", err)
	}

	wrapperJSON, err := json.Marshal(struct {
		Ciphertext string `json:"ciphertext"`
	}{encoded})
	if err != nil {
		return err
	}

	_ = os.Chmod(filepath.Dir(path), 0700)
	return utils.AtomicWriteFile(path, wrapperJSON, 0600)
}

func (db *DB) GetDBConfigFile() string {
	return filepath.Join(db.Path, "config.json")
}

func (db *DB) GetGroupsDirPath() string {
	return filepath.Join(db.Path, "groups")
}

func (db *DB) GetGroupDirPath(group_id int) string {
	return filepath.Join(db.Path, "groups", strconv.Itoa(group_id))
}

func (db *DB) GetGroupConfigFilePath(group_id int) string {
	return filepath.Join(db.Path, "groups", strconv.Itoa(group_id), "group_config.json")
}

func (db *DB) GetProfileFilePath(group_id int, profile_id int) string {
	return filepath.Join(db.Path, "groups", strconv.Itoa(group_id), fmt.Sprintf("%d.json", profile_id))
}

func (db *DB) isFileEncrypted(path string) bool {
	f, err := os.Open(path)
	if err != nil {
		return false
	}
	defer f.Close()
	head := make([]byte, 14)
	n, _ := f.Read(head)
	return n == 14 && string(head) == `{"ciphertext":`
}

func (db *DB) migrateFileToEncrypted(path string) {
	if db.isFileEncrypted(path) {
		return
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		// A missing file is normal: stale or partially deleted group
		// directories do not contain every legacy file.
		if errors.Is(err, os.ErrNotExist) {
			return
		}
		logger.Warnf("migration: failed to read %s: %v", path, err)
		return
	}
	encoded, err := crypto.EncryptToBase64(raw, db.key)
	if err != nil {
		logger.Warnf("migration: failed to encrypt %s: %v", path, err)
		return
	}
	wrapperJSON, err := json.Marshal(struct {
		Ciphertext string `json:"ciphertext"`
	}{encoded})
	if err != nil {
		logger.Warnf("migration: failed to marshal wrapper for %s: %v", path, err)
		return
	}
	if err := utils.AtomicWriteFile(path, wrapperJSON, 0600); err != nil {
		logger.Warnf("migration: failed to write %s: %v", path, err)
	}
}

func (db *DB) MigrateToEncrypted() {
	db.mu.Lock()
	defer db.mu.Unlock()

	logger.Info("migrating database to encrypted format")

	db.migrateFileToEncrypted(db.GetDBConfigFile())

	routingPath := db.GetRoutingFilePath()
	if _, err := os.Stat(routingPath); err == nil {
		db.migrateFileToEncrypted(routingPath)
	}

	entries, err := os.ReadDir(db.GetGroupsDirPath())
	if err != nil {
		if !os.IsNotExist(err) {
			logger.Warnf("migration: cannot read groups dir: %v", err)
		}
		return
	}
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		group_id, err := strconv.Atoi(entry.Name())
		if err != nil {
			continue
		}
		group_cfg_path := db.GetGroupConfigFilePath(group_id)
		db.migrateFileToEncrypted(group_cfg_path)

		group_entries, err := os.ReadDir(db.GetGroupDirPath(group_id))
		if err != nil {
			logger.Warnf("migration: cannot read group dir %d: %v", group_id, err)
			continue
		}
		for _, ge := range group_entries {
			if ge.IsDir() || ge.Name() == "group_config.json" {
				continue
			}
			profile_path := filepath.Join(db.GetGroupDirPath(group_id), ge.Name())
			db.migrateFileToEncrypted(profile_path)
		}
	}

	logger.Info("migration complete")
}
