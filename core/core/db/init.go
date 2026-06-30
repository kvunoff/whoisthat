package db

import (
	"os"
	"path/filepath"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

func (db *DB) Initialize() {
	homeDir, err := utils.GetHomeDir()
	if err != nil {
		logger.Fatal("cannot get user home directory")
	}
	var db_path = filepath.Join(homeDir, ".local", "share", "whoisthat", "db")
	db.Path = db_path
	if err := os.MkdirAll(db_path, 0700); err != nil {
		logger.Fatal("failed to create database directory " + db_path + ": " + err.Error())
	}
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
