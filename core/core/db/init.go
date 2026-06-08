package db

import (
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
	"encoding/json"
	"os"
	"path/filepath"
	"syscall"
)

func (db *DB) Initialize() {
	oldUmask := syscall.Umask(0)
	defer syscall.Umask(oldUmask)
	homeDir, err := utils.GetHomeDir()
	if err != nil {
		logger.Fatal("cannot get user home directory")
	}
	var db_path = filepath.Join(homeDir, ".local", "share", "whoisthat", "db")
	db.Path = db_path
	if err := os.MkdirAll(db_path, 0777); err != nil {
		logger.Fatal("failed to create database directory "+db_path+": "+err.Error())
	}
	db.ensureDBConfigExistance()
}

func (db *DB) ensureDBConfigExistance() {
	oldUmask := syscall.Umask(0)
	defer syscall.Umask(oldUmask)

	db_config_path := db.GetDBConfigFile()

	if _, err := os.Stat(db_config_path); err == nil {
		logger.Info("using existing DB config")
	} else if !os.IsNotExist(err) {
		logger.Fatal("error checking for database config " + db_config_path + ": " + err.Error())
	} else {
		db_config := structs.DBConfig{}
		json_data, err := json.MarshalIndent(db_config, "", " ")

		if err != nil {
			logger.Fatal("failed to stringify db config")
		}

		if err := os.WriteFile(db_config_path, json_data, 0666); err != nil {
			logger.Fatal("failed to write to db config " + db_config_path + ": " + err.Error())
		}
	}

}
