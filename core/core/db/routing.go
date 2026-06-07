package db

import (
	"whoisthat-core/structs"
	"encoding/json"
	"log"
	"os"
	"path/filepath"
	"syscall"
)

func (db *DB) GetRoutingFilePath() string {
	return filepath.Join(db.Path, "routing.json")
}

func (db *DB) LoadRouting() (structs.RoutingConfig, error) {
	db.mu.Lock()
	defer db.mu.Unlock()

	var cfg structs.RoutingConfig

	path := db.GetRoutingFilePath()
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return defaultRouting(), nil
		}
		return cfg, err
	}

	if err := json.Unmarshal(data, &cfg); err != nil {
		log.Printf("routing config corrupted, using defaults: %v", err)
		return defaultRouting(), nil
	}

	return cfg, nil
}

func (db *DB) SaveRouting(cfg structs.RoutingConfig) error {
	oldUmask := syscall.Umask(0)
	defer syscall.Umask(oldUmask)

	db.mu.Lock()
	defer db.mu.Unlock()

	path := db.GetRoutingFilePath()
	data, err := json.MarshalIndent(cfg, "", "  ")
	if err != nil {
		return err
	}

	return os.WriteFile(path, data, 0666)
}

func defaultRouting() structs.RoutingConfig {
	return structs.RoutingConfig{
		DomainStrategy: "IPIfNonMatch",
		Rules: []structs.RoutingRule{
			{
				Type:        "field",
				IP:          "10.0.0.0/8,172.16.0.0/12,192.168.0.0/16",
				OutboundTag: "direct",
				Enabled:     true,
			},
		},
	}
}
