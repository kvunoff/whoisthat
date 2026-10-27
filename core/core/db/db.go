package db

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"sync"
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
	keyPath := db.GetKeyFilePath()
	legacyPath := filepath.Join(db.Path, ".key")

	data, err := os.ReadFile(keyPath)
	if err == nil {
		db.key = validateKey(keyPath, data)
		_ = os.Chmod(keyPath, 0600)
		return
	}
	if !os.IsNotExist(err) {
		logger.Fatal("failed to read key file " + keyPath + ": " + err.Error())
	}

	// Migrate the legacy co-located key on first start after the upgrade.
	// We intentionally keep the old file in place so a downgrade still works;
	// it can be removed in a later release.
	if keyPath != legacyPath {
		legacy, legacyErr := os.ReadFile(legacyPath)
		if legacyErr == nil {
			db.key = validateKey(legacyPath, legacy)
			if writeErr := utils.AtomicWriteFile(keyPath, legacy, 0600); writeErr != nil {
				logger.Fatal("failed to migrate key file to " + keyPath + ": " + writeErr.Error())
			}
			logger.Infof("db: migrated encryption key from %s to %s", legacyPath, keyPath)
			return
		}
		if !os.IsNotExist(legacyErr) {
			logger.Fatal("failed to read legacy key file " + legacyPath + ": " + legacyErr.Error())
		}
	}

	key, err := crypto.GenerateKey()
	if err != nil {
		logger.Fatal("failed to generate encryption key: " + err.Error())
	}
	if err := utils.AtomicWriteFile(keyPath, key, 0600); err != nil {
		logger.Fatal("failed to write key file " + keyPath + ": " + err.Error())
	}
	db.key = key
}

// validateKey refuses to silently replace a damaged key: doing so would make
// the existing encrypted database permanently unreadable.
func validateKey(path string, data []byte) []byte {
	if len(data) != encryptionKeySize {
		logger.Fatal("encryption key " + path + " has invalid length " +
			strconv.Itoa(len(data)) + " (expected 32 bytes); refusing to overwrite it. " +
			"Restore the key from a backup or remove the database and key together.")
	}
	return data
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
