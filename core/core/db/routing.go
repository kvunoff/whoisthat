package db

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
)

func (db *DB) GetRoutingFilePath() string {
	return filepath.Join(db.Path, "routing.json")
}

// GetGroupRoutingFilePath returns the per-group manual routing override.
// The file only exists for groups whose ManualRoutingMode is "custom".
func (db *DB) GetGroupRoutingFilePath(groupID int) string {
	return filepath.Join(db.GetRoutingDir(groupID), "manual.json")
}

func (db *DB) loadRouting() (structs.RoutingConfig, error) {
	var cfg structs.RoutingConfig

	path := db.GetRoutingFilePath()
	if err := db.readEncryptedJSON(path, &cfg); err != nil {
		if os.IsNotExist(err) {
			return defaultRouting(), nil
		}
		logger.Warnf("routing config corrupted, using defaults: %v", err)
		return defaultRouting(), nil
	}

	return cfg, nil
}

func (db *DB) LoadRouting() (structs.RoutingConfig, error) {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.loadRouting()
}

func (db *DB) SaveRouting(cfg structs.RoutingConfig) error {
	db.mu.Lock()
	defer db.mu.Unlock()

	path := db.GetRoutingFilePath()
	return db.writeEncryptedJSON(path, cfg)
}

// loadGroupRouting reads a group's custom rules. A missing file means the
// group has not customized anything yet, so the defaults are returned. The
// caller must decide whether the group actually uses custom rules by looking
// at Group.ManualRoutingMode.
func (db *DB) loadGroupRouting(groupID int) (structs.RoutingConfig, error) {
	var cfg structs.RoutingConfig

	path := db.GetGroupRoutingFilePath(groupID)
	if err := db.readEncryptedJSON(path, &cfg); err != nil {
		if os.IsNotExist(err) {
			return defaultRouting(), nil
		}
		logger.Warnf("routing config for group %d corrupted, using defaults: %v", groupID, err)
		return defaultRouting(), nil
	}

	return cfg, nil
}

func (db *DB) LoadGroupRouting(groupID int) (structs.RoutingConfig, error) {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.loadGroupRouting(groupID)
}

func (db *DB) saveGroupRouting(groupID int, cfg structs.RoutingConfig) error {
	dir := db.GetRoutingDir(groupID)
	if err := os.MkdirAll(dir, 0700); err != nil {
		return fmt.Errorf("create group routing dir: %w", err)
	}
	return db.writeEncryptedJSON(db.GetGroupRoutingFilePath(groupID), cfg)
}

func (db *DB) SaveGroupRouting(groupID int, cfg structs.RoutingConfig) error {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.saveGroupRouting(groupID, cfg)
}

func (db *DB) DeleteGroupRouting(groupID int) error {
	db.mu.Lock()
	defer db.mu.Unlock()

	if err := os.Remove(db.GetGroupRoutingFilePath(groupID)); err != nil && !os.IsNotExist(err) {
		return err
	}
	return nil
}

// loadGroupRoutingIfExists returns nil when the group has no custom override.
func (db *DB) loadGroupRoutingIfExists(groupID int) (*structs.RoutingConfig, error) {
	var cfg structs.RoutingConfig

	path := db.GetGroupRoutingFilePath(groupID)
	if err := db.readEncryptedJSON(path, &cfg); err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	return &cfg, nil
}

func (db *DB) LoadGroupRoutingIfExists(groupID int) (*structs.RoutingConfig, error) {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.loadGroupRoutingIfExists(groupID)
}

func (db *DB) hasGroupRouting(groupID int) bool {
	_, err := os.Stat(db.GetGroupRoutingFilePath(groupID))
	return err == nil
}

// loadEffectiveManualRouting resolves the manual rules that apply to a group:
// its own override when enabled, otherwise the global rules. The caller must
// hold db.mu.
func (db *DB) loadEffectiveManualRouting(groupID int) (structs.RoutingConfig, string, error) {
	if groupID != 0 {
		group, err := db.loadGroupConfig(groupID)
		if err == nil && group.ManualRoutingMode == structs.ManualRoutingCustom {
			cfg, err := db.loadGroupRouting(groupID)
			return cfg, structs.RoutingSourceCustom, err
		}
		if err != nil {
			logger.Warnf("routing: failed to load group %d, falling back to global rules: %v", groupID, err)
		}
	}

	cfg, err := db.loadRouting()
	return cfg, structs.RoutingSourceGlobal, err
}

func (db *DB) LoadEffectiveManualRouting(groupID int) (structs.RoutingConfig, string, error) {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.loadEffectiveManualRouting(groupID)
}

func (db *DB) setManualRoutingSource(groupID int, source string) error {
	if groupID == 0 {
		return errors.New("global routing has no manual source")
	}

	source = strings.ToLower(strings.TrimSpace(source))
	if source != structs.ManualRoutingInherit && source != structs.ManualRoutingCustom {
		return fmt.Errorf("invalid manual routing source %q: must be inherit or custom", source)
	}

	group, err := db.loadGroupConfig(groupID)
	if err != nil {
		return fmt.Errorf("load group %d: %w", groupID, err)
	}

	if source == structs.ManualRoutingCustom && !db.hasGroupRouting(groupID) {
		global, err := db.loadRouting()
		if err != nil {
			return err
		}
		if err := db.saveGroupRouting(groupID, global); err != nil {
			return err
		}
	}

	group.ManualRoutingMode = source
	return db.updateGroup(group)
}

func (db *DB) SetManualRoutingSource(groupID int, source string) error {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.setManualRoutingSource(groupID, source)
}

func defaultRouting() structs.RoutingConfig {
	return structs.RoutingConfig{
		DomainStrategy: "IPIfNonMatch",
		Rules: []structs.RoutingRule{
			{
				Type:        "field",
				IP:          "10.0.0.0/8,172.16.0.0/12,192.168.0.0/16,fc00::/7,fe80::/10,::1/128",
				OutboundTag: "direct",
				Enabled:     true,
			},
		},
	}
}
