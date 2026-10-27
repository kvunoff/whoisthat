package db

import (
	"os"
	"path/filepath"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

func (db *DB) Initialize() {
	if db.Path == "" {
		db.Path = filepath.Join(utils.DataDir(), "db")
		// Keep the production key outside the database directory so a
		// backup/copy of the data directory does not also contain the key
		// that decrypts it. Tests and tools that pass an explicit Path keep
		// the legacy per-directory key unless they set KeyPath themselves.
		if db.KeyPath == "" {
			db.KeyPath = filepath.Join(utils.ConfigDir(), "db.key")
		}
	}
	if err := os.MkdirAll(db.Path, 0700); err != nil {
		logger.Fatal("failed to create database directory " + db.Path + ": " + err.Error())
	}
	// Tighten directories created by older releases with a wider mode.
	_ = os.Chmod(db.Path, 0700)
	db.loadOrCreateKey()
	db.ensureDBConfigExistance()
	db.MigrateToEncrypted()
}

func (db *DB) ensureDBConfigExistance() {
	db_config_path := db.GetDBConfigFile()

	if _, err := os.Stat(db_config_path); err == nil {
		logger.Info("using existing DB config")
	} else if !os.IsNotExist(err) {
		logger.Fatal("error checking for database config " + db_config_path + ": " + err.Error())
	} else {
		db_config := structs.DBConfig{}
		if err := db.writeEncryptedJSON(db_config_path, db_config); err != nil {
			logger.Fatal("failed to write db config " + db_config_path + ": " + err.Error())
		}
	}
}
