package appconfig

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"whoisthat-core/lib/logger"
	"whoisthat-core/utils"
)

type LoadStatus struct {
	State string `json:"state"`
	Error string `json:"error,omitempty"`
}

var loadStatus = LoadStatus{State: "ok"}

// loaded is set once LoadConfig has read the file. SaveConfig refuses to write
// before that: persisting a config nobody read is exactly how a test run used
// to replace the developer's real settings with defaults.
var loaded bool

// Status returns how the current in-memory config was obtained.
func Status() LoadStatus {
	return loadStatus
}

func configPath() string {
	return filepath.Join(utils.ConfigDir(), "config.json")
}

// BackupPath is the last known good config.json, refreshed on every successful
// load. The TUI's "restore settings" action copies it back.
func BackupPath() string {
	return configPath() + ".bak"
}

// RestoreFromBackup writes the last known good config.json back and reloads
// it. Falls back to the previous generation when the newest copy is itself a
// default file written without a loaded config. It is a no-op error when no
// usable backup exists yet.
func RestoreFromBackup() error {
	data, _, ok := readUsableBackup()
	if !ok {
		return fmt.Errorf("no settings backup available: %w", os.ErrNotExist)
	}
	if err := utils.AtomicWriteFile(configPath(), data, 0600); err != nil {
		return fmt.Errorf("cannot write config.json: %w", err)
	}
	LoadConfig()
	if loadStatus.State == "corrupt" {
		return fmt.Errorf("settings backup is unreadable")
	}
	return nil
}

// readUsableBackup returns the newest backup that parses and actually carries a
// device id. A hwid-enabled copy without a hwid was produced by a process that
// never ran LoadConfig, so restoring it would only install defaults again.
func readUsableBackup() ([]byte, AppConfig, bool) {
	for _, path := range []string{BackupPath(), BackupPath() + ".prev"} {
		data, err := os.ReadFile(path)
		if err != nil {
			continue
		}
		cfg := defaultConfig()
		if err := json.Unmarshal(data, &cfg); err != nil {
			continue
		}
		if cfg.HwidEnabled && cfg.Hwid == "" {
			continue
		}
		return data, cfg, true
	}
	return nil, AppConfig{}, false
}

func refreshBackup() {
	data, err := os.ReadFile(configPath())
	if err != nil {
		return
	}
	if old, err := os.ReadFile(BackupPath()); err == nil {
		if bytes.Equal(old, data) {
			return
		}
		// Keep one generation further back, so a single bad write cannot
		// destroy the last meaningful copy.
		_ = utils.AtomicWriteFile(BackupPath()+".prev", old, 0600)
	}
	if err := utils.AtomicWriteFile(BackupPath(), data, 0600); err != nil {
		logger.Warnf("config: cannot refresh settings backup: %v", err)
	}
}

func LoadConfig() {
	config, status := readConfig()
	loaded = true
	// A valid file without a device id cannot have been produced by a normal
	// start (LoadConfig always generates one), so it was written by something
	// that never read the config. Only worth reporting when a usable backup
	// exists — otherwise there is nothing to offer but the current defaults.
	if status.State == "ok" && config.HwidEnabled && config.Hwid == "" {
		if _, _, ok := readUsableBackup(); ok {
			status = LoadStatus{State: "orphaned", Error: "config.json was written without a device id"}
		}
	}
	loadStatus = status
	if status.State == "corrupt" {
		logger.Warnf("config: %s (using recovered values; config.json left untouched)", status.Error)
	}
	if config.HwidEnabled && config.Hwid == "" {
		config.Hwid = generateHwid()
	}
	config.DnsServers = sanitizeDnsServers(config.DnsServers)
	config.TunName = sanitizeTunName(config.TunName)
	config.AutoconnectMode = sanitizeAutoconnectMode(config.AutoconnectMode)
	config.TestConfig = sanitizeTestConfig(config.TestConfig)
	var migratedUserAgent bool
	config.UserAgent, migratedUserAgent = migrateUserAgent(config.UserAgent)
	// include-mode split tunnel only routes the marked apps through the VPN;
	// a system-wide kill-switch would block everything else, contradicting the
	// mode's purpose. Refuse the combination rather than creating a surprise
	// total blackout for the user.
	if config.SplitTunnel.Mode == "include" && config.KillSwitchEnabled {
		logger.Warn("config: split-tunnel include mode is incompatible with kill-switch; disabling kill-switch")
		config.KillSwitchEnabled = false
	}
	application_configuration = config
	if migratedUserAgent || status.State == "missing" {
		SaveConfig()
	}
	// Keep the last known good copy so a corrupted config.json can be restored
	// with one click instead of re-entering every setting.
	if status.State == "ok" || status.State == "repaired" {
		refreshBackup()
	}
}

func SaveConfig() {
	// Writing a config that was never read is how a test process used to
	// replace the real settings with defaults.
	if !loaded {
		logger.Warn("save config: refusing to write before LoadConfig ran")
		return
	}
	dir_path := utils.ConfigDir()
	config_path := filepath.Join(dir_path, "config.json")

	if err := os.MkdirAll(dir_path, 0700); err != nil {
		logger.Warn("save config: cannot create dir:", err)
		return
	}
	utils.EnsurePerms(dir_path, 0700)

	// A damaged file is the only copy of the values that could not be parsed.
	// Keep it before the first write over it, then the file is valid again.
	if loadStatus.State == "corrupt" {
		if data, err := os.ReadFile(config_path); err == nil {
			aside := config_path + ".corrupt-" + time.Now().Format("20060102-150405")
			_ = utils.AtomicWriteFile(aside, data, 0600)
		}
	}

	jsonData, err := json.MarshalIndent(application_configuration, "", " ")
	if err != nil {
		logger.Warn("save config: marshal failed:", err)
		return
	}

	if err := utils.AtomicWriteFile(config_path, jsonData, 0600); err != nil {
		logger.Warn("save config: write failed:", err)
		return
	}
	loadStatus = LoadStatus{State: "ok"}
	logger.Debugf("config saved to %s", config_path)
}

func readConfig() (AppConfig, LoadStatus) {
	config_path := configPath()
	file_bytes, err := os.ReadFile(config_path)
	if os.IsNotExist(err) {
		return defaultConfig(), LoadStatus{State: "missing"}
	}
	if err != nil {
		return defaultConfig(), LoadStatus{State: "corrupt", Error: "cannot read config.json: " + err.Error()}
	}
	// Tighten permissions left behind by older releases (0666/0644).
	utils.EnsurePerms(config_path, 0600)
	return parseConfig(file_bytes)
}

// parseConfig never gives up on the whole file because of one bad value: a
// single corrupt field used to reset every setting on the next save.
func parseConfig(data []byte) (AppConfig, LoadStatus) {
	var cfg AppConfig = defaultConfig()
	if err := json.Unmarshal(data, &cfg); err == nil {
		return cfg, LoadStatus{State: "ok"}
	}

	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		// Not even a JSON object: fall back to the last known good copy.
		if restored, ok := restoreBackupFile(); ok {
			return restored, LoadStatus{State: "repaired"}
		}
		return defaultConfig(), LoadStatus{State: "corrupt", Error: "config.json is not valid JSON"}
	}

	cfg = defaultConfig()
	for key, raw := range fields {
		if string(raw) == "null" {
			continue
		}
		if err := applyField(&cfg, key, raw); err != nil {
			logger.Warnf("config: ignoring invalid value for %q: %v", key, err)
		}
	}
	return cfg, LoadStatus{State: "corrupt", Error: "config.json contains invalid values"}
}

// restoreBackupFile copies config.json.bak over config.json when the backup is
// readable, so one unreadable file does not force the user to reconfigure.
func restoreBackupFile() (AppConfig, bool) {
	data, cfg, ok := readUsableBackup()
	if !ok {
		return AppConfig{}, false
	}
	if err := utils.AtomicWriteFile(configPath(), data, 0600); err != nil {
		logger.Warnf("config: cannot restore backup: %v", err)
		return AppConfig{}, false
	}
	logger.Warnf("config: restored settings from backup")
	return cfg, true
}

// applyField decodes one top-level key so a single broken field cannot poison
// the rest of the file.
