package db

import (
	"whoisthat-core/lib/crypto"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"sync"
	"syscall"
)

type DB struct {
	Path string
	key  []byte
	mu   sync.Mutex
}

func (db *DB) saveDBConfig(db_config structs.DBConfig) error {
	oldUmask := syscall.Umask(0)
	defer syscall.Umask(oldUmask)

	return db.writeEncryptedJSON(db.GetDBConfigFile(), db_config)
}

func (db *DB) loadDBConfig() (structs.DBConfig, error) {
	var db_config_data structs.DBConfig
	if err := db.readEncryptedJSON(db.GetDBConfigFile(), &db_config_data); err != nil {
		return db_config_data, err
	}
	return db_config_data, nil
}

func (db *DB) loadOrCreateKey() {
	keyPath := db.GetKeyFilePath()
	data, err := os.ReadFile(keyPath)
	if err == nil && len(data) == 32 {
		db.key = data
		return
	}
	if err != nil && !os.IsNotExist(err) {
		logger.Fatal("failed to read key file " + keyPath + ": " + err.Error())
	}
	key, err := crypto.GenerateKey()
	if err != nil {
		logger.Fatal("failed to generate encryption key: " + err.Error())
	}
	if err := os.WriteFile(keyPath, key, 0600); err != nil {
		logger.Fatal("failed to write key file " + keyPath + ": " + err.Error())
	}
	db.key = key
}

func (db *DB) GetKeyFilePath() string {
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

	return os.WriteFile(path, wrapperJSON, 0666)
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
	if err := os.WriteFile(path, wrapperJSON, 0666); err != nil {
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
