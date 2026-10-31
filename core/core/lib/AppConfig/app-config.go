package appconfig

import (
	"bytes"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"strings"
	"time"
	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/version"
	"whoisthat-core/utils"
)

var tunNameRe = regexp.MustCompile(`^[a-zA-Z][a-zA-Z0-9_-]{0,14}$`)

type AppConfig struct {
	SocksPort            int        `json:"socks-port"`
	HttpPort             int        `json:"http-port"`
	CoreTCPPort          int        `json:"core-tcp-port"`
	TestPortRange        PortRange  `json:"test-port-range"`
	DnsServers           []string   `json:"dns-servers"`
	TunName              string     `json:"tun-name"`
	HwidEnabled          bool       `json:"hwid-enabled"`
	Hwid                 string     `json:"hwid"`
	UserAgent            string     `json:"user-agent"`
	KillSwitchEnabled    bool       `json:"kill-switch-enabled"`
	AutoconnectEnabled   bool       `json:"autoconnect-enabled"`
	AutoconnectGroupId   int        `json:"autoconnect-group-id"`
	AutoconnectProfileId int        `json:"autoconnect-profile-id"`
	AutoconnectMode      string     `json:"autoconnect-mode"`
	TestConfig           TestConfig `json:"test-config"`
	// IPCSocketPath is the Unix domain socket the core listens on. Empty means
	// "derive the default from XDG_RUNTIME_DIR at runtime" (see SocketPath()).
	IPCSocketPath string `json:"ipc-socket-path,omitempty"`
	// TCPEnabled additionally opens the token-authenticated TCP listener on
	// CoreTCPPort. Off by default: the UDS is the secure default transport and
	// TCP is opt-in for remote/advanced setups.
	TCPEnabled bool `json:"tcp-enabled"`
	// SplitTunnel controls per-app routing for apps launched via `whoisthat run`.
	SplitTunnel SplitTunnelConfig `json:"split-tunnel"`
}

// SplitTunnelConfig persists the split-tunnel mode. Mode is one of "off"
// (default), "exclude" (split apps bypass the tunnel), or "include" (only split
// apps use the tunnel). Apps enter the split slice via `whoisthat run <app>`.
type SplitTunnelConfig struct {
	Mode string `json:"mode"`
}

type TestConfig struct {
	Concurrency    int    `json:"concurrency"`
	TimeoutSeconds int    `json:"timeout-seconds"`
	SamplesPerTest int    `json:"samples-per-test"`
	TestEndpoint   string `json:"test-endpoint"`
	AutoTestOnSub  bool   `json:"auto-test-on-subscribe"`
}

type PortRange struct {
	Start int `json:"start"`
	End   int `json:"end"`
}

var application_configuration AppConfig = defaultConfig()

func defaultConfig() AppConfig {
	return AppConfig{
		SocksPort:   3090,
		HttpPort:    3091,
		CoreTCPPort: 4897,
		TestPortRange: PortRange{
			Start: 3095,
			End:   30120,
		},
		DnsServers:      []string{"1.1.1.1", "8.8.8.8", "2606:4700:4700::1111", "2001:4860:4860::8888"},
		TunName:         "whoisthattun",
		HwidEnabled:     true,
		UserAgent:       "whoisthat/v" + version.CoreVersion,
		AutoconnectMode: "proxy",
		TestConfig: TestConfig{
			Concurrency:    16,
			TimeoutSeconds: 5,
			SamplesPerTest: 3,
			TestEndpoint:   "https://cp.cloudflare.com/generate_204",
			AutoTestOnSub:  true,
		},
	}
}

func SetTestConfig(cfg TestConfig) {
	t := sanitizeTestConfig(cfg)
	application_configuration.TestConfig = t
	SaveConfig()
}

func sanitizeTestConfig(cfg TestConfig) TestConfig {
	if cfg.Concurrency < 1 {
		cfg.Concurrency = application_configuration.TestConfig.Concurrency
	}
	if cfg.TimeoutSeconds < 1 {
		cfg.TimeoutSeconds = application_configuration.TestConfig.TimeoutSeconds
	}
	if cfg.SamplesPerTest < 1 {
		cfg.SamplesPerTest = application_configuration.TestConfig.SamplesPerTest
	}
	if cfg.TestEndpoint == "" {
		cfg.TestEndpoint = application_configuration.TestConfig.TestEndpoint
	}
	if !strings.HasPrefix(cfg.TestEndpoint, "http://") &&
		!strings.HasPrefix(cfg.TestEndpoint, "https://") {
		cfg.TestEndpoint = application_configuration.TestConfig.TestEndpoint
	}
	return cfg
}

func GetConfig() AppConfig {
	return application_configuration
}

// SocketPath returns the Unix domain socket path the core listens on. It honors
// an explicit IPCSocketPath override, otherwise derives the default under
// XDG_RUNTIME_DIR (falling back to /tmp/whoisthat-<uid> when that is unset, e.g.
// under some systemd/boot contexts). The Rust TUI computes the same path so the
// two agree without any handshake.
// IPCTokenPath is the shared secret used to authenticate TCP IPC clients.
// The file lives in the user's runtime directory (0700/0600) and is never
// written into config.json.
func IPCTokenPath() string {
	return filepath.Join(utils.RuntimeDir(), "ipc.token")
}

// EnsureIPCToken reads the existing token or generates a new 256-bit one.
func EnsureIPCToken() (string, error) {
	path := IPCTokenPath()
	if data, err := os.ReadFile(path); err == nil {
		token := strings.TrimSpace(string(data))
		if len(token) == 64 {
			if _, err := hex.DecodeString(token); err == nil {
				_ = os.Chmod(path, 0600)
				return token, nil
			}
		}
	}
	raw := make([]byte, 32)
	if _, err := rand.Read(raw); err != nil {
		return "", fmt.Errorf("failed to generate IPC token: %w", err)
	}
	token := hex.EncodeToString(raw)
	_ = os.Chmod(filepath.Dir(path), 0700)
	if err := utils.AtomicWriteFile(path, []byte(token+"\n"), 0600); err != nil {
		return "", fmt.Errorf("failed to write IPC token %s: %w", path, err)
	}
	return token, nil
}

func SocketPath() string {
	if p := strings.TrimSpace(application_configuration.IPCSocketPath); p != "" {
		return p
	}
	return DefaultSocketPath()
}

func DefaultSocketPath() string {
	dir := os.Getenv("XDG_RUNTIME_DIR")
	if dir == "" {
		dir = filepath.Join("/tmp", fmt.Sprintf("whoisthat-%d", os.Getuid()))
	} else {
		dir = filepath.Join(dir, "whoisthat")
	}
	return filepath.Join(dir, "core.sock")
}

// LoadStatus tells the TUI whether config.json could be read. States:
// "ok" (loaded), "missing" (first start), "corrupt" (unreadable; the running
// config holds whatever could be recovered and the file was not written),
// "repaired" (restored from config.json.bak).
type LoadStatus struct {
	State string `json:"state"`
	Error string `json:"error,omitempty"`
}

var loadStatus = LoadStatus{State: "ok"}

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
// it. It is a no-op error when no backup exists yet.
func RestoreFromBackup() error {
	data, err := os.ReadFile(BackupPath())
	if err != nil {
		return fmt.Errorf("no settings backup available: %w", err)
	}
	var probe = defaultConfig()
	if err := json.Unmarshal(data, &probe); err != nil {
		return fmt.Errorf("settings backup is unreadable: %w", err)
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

func refreshBackup() {
	data, err := os.ReadFile(configPath())
	if err != nil {
		return
	}
	if old, err := os.ReadFile(BackupPath()); err == nil && bytes.Equal(old, data) {
		return
	}
	if err := utils.AtomicWriteFile(BackupPath(), data, 0600); err != nil {
		logger.Warnf("config: cannot refresh settings backup: %v", err)
	}
}

func LoadConfig() {
	config, status := readConfig()
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

const legacyUserAgentPrefix = "whoisthat-core/"

// migrateUserAgent renames the product in the stored default User-Agent.
// Values that do not use the legacy prefix (custom client signatures) are
// returned unchanged.
func migrateUserAgent(ua string) (string, bool) {
	if rest, ok := strings.CutPrefix(ua, legacyUserAgentPrefix); ok {
		return "whoisthat/" + rest, true
	}
	return ua, false
}

// sanitizeDnsServers drops any entry that is not a literal IP address. DNS
// servers are interpolated into root-capable shell scripts (TUN DNS hijack/
// routing) and into xray's config; restricting them to parseable IPs closes
// that injection surface. Falls back to defaults if nothing valid remains.
func sanitizeDnsServers(servers []string) []string {
	valid := make([]string, 0, len(servers))
	for _, s := range servers {
		if net.ParseIP(s) != nil {
			valid = append(valid, s)
			continue
		}
		logger.Warnf("config: ignoring invalid dns server %q", s)
	}
	if len(valid) == 0 {
		logger.Warn("config: no valid dns servers configured, using defaults")
		return defaultConfig().DnsServers
	}
	return valid
}

func generateHwid() string {
	b := make([]byte, 8)
	if _, err := rand.Read(b); err != nil {
		logger.Warn("hwid: failed to generate random bytes, using fallback")
		return "0000000000000000"
	}
	return hex.EncodeToString(b)
}

func Platform() string {
	return runtime.GOOS
}

func KernelVersion() string {
	out, err := exec.Command("uname", "-r").Output()
	if err != nil {
		return "unknown"
	}
	return strings.TrimSpace(string(out))
}

func DistroModel() string {
	data, err := os.ReadFile("/etc/os-release")
	if err != nil {
		return "Linux"
	}
	for _, line := range strings.Split(string(data), "\n") {
		if strings.HasPrefix(line, "PRETTY_NAME=") {
			v := strings.TrimPrefix(line, "PRETTY_NAME=")
			v = strings.Trim(v, "\"")
			return v
		}
	}
	return "Linux"
}

func EnableHwid(enable bool) {
	application_configuration.HwidEnabled = enable
	SaveConfig()
}

func ResetHwid() {
	application_configuration.Hwid = generateHwid()
	SaveConfig()
}

func SetUserAgent(ua string) {
	application_configuration.UserAgent = ua
	SaveConfig()
}

func SetKillSwitch(enabled bool) {
	application_configuration.KillSwitchEnabled = enabled
	SaveConfig()
}

// SetSplitTunnelMode persists the split-tunnel mode. Invalid values are coerced
// to "off" (with a warning) so a bad client value cannot install bogus routing.
func SetSplitTunnelMode(mode string) string {
	switch mode {
	case "off", "exclude", "include":
	default:
		if mode != "" {
			logger.Warnf("config: invalid split-tunnel mode %q, using off", mode)
		}
		mode = "off"
	}
	application_configuration.SplitTunnel.Mode = mode
	SaveConfig()
	return mode
}

func sanitizeAutoconnectMode(mode string) string {
	if mode != "proxy" && mode != "tun" {
		if mode != "" {
			logger.Warnf("config: invalid autoconnect mode %q, using proxy", mode)
		}
		return "proxy"
	}
	return mode
}

func SetAutoconnect(enabled bool, groupId, profileId int, mode string) {
	mode = sanitizeAutoconnectMode(mode)
	application_configuration.AutoconnectEnabled = enabled
	application_configuration.AutoconnectGroupId = groupId
	application_configuration.AutoconnectProfileId = profileId
	application_configuration.AutoconnectMode = mode
	SaveConfig()
}

func sanitizeTunName(name string) string {
	if name == "" || !tunNameRe.MatchString(name) {
		logger.Warnf("config: invalid tun name %q, using default", name)
		return defaultConfig().TunName
	}
	return name
}

func SetTunName(name string) error {
	if name == "" {
		return errors.New("tun name must not be empty")
	}
	if len(name) > 15 {
		return fmt.Errorf("tun name too long: %d characters (max 15)", len(name))
	}
	if !tunNameRe.MatchString(name) {
		return errors.New("tun name must start with a letter and contain only [a-zA-Z0-9_-]")
	}
	application_configuration.TunName = name
	SaveConfig()
	return nil
}

func SaveConfig() {
	dir_path := utils.ConfigDir()
	config_path := filepath.Join(dir_path, "config.json")

	if err := os.MkdirAll(dir_path, 0700); err != nil {
		logger.Warn("save config: cannot create dir:", err)
		return
	}
	_ = os.Chmod(dir_path, 0700)

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
	_ = os.Chmod(config_path, 0600)
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
	data, err := os.ReadFile(BackupPath())
	if err != nil {
		return AppConfig{}, false
	}
	var cfg AppConfig = defaultConfig()
	if err := json.Unmarshal(data, &cfg); err != nil {
		return AppConfig{}, false
	}
	if err := utils.AtomicWriteFile(configPath(), data, 0600); err != nil {
		logger.Warnf("config: cannot restore backup: %v", err)
		return AppConfig{}, false
	}
	logger.Warnf("config: restored settings from %s", BackupPath())
	return cfg, true
}

// applyField decodes one top-level key so a single broken field cannot poison
// the rest of the file.
func applyField(cfg *AppConfig, key string, raw json.RawMessage) error {
	switch key {
	case "socks-port":
		return json.Unmarshal(raw, &cfg.SocksPort)
	case "http-port":
		return json.Unmarshal(raw, &cfg.HttpPort)
	case "core-tcp-port":
		return json.Unmarshal(raw, &cfg.CoreTCPPort)
	case "test-port-range":
		return json.Unmarshal(raw, &cfg.TestPortRange)
	case "dns-servers":
		return json.Unmarshal(raw, &cfg.DnsServers)
	case "tun-name":
		return json.Unmarshal(raw, &cfg.TunName)
	case "hwid-enabled":
		return json.Unmarshal(raw, &cfg.HwidEnabled)
	case "hwid":
		return json.Unmarshal(raw, &cfg.Hwid)
	case "user-agent":
		return json.Unmarshal(raw, &cfg.UserAgent)
	case "kill-switch-enabled":
		return json.Unmarshal(raw, &cfg.KillSwitchEnabled)
	case "autoconnect-enabled":
		return json.Unmarshal(raw, &cfg.AutoconnectEnabled)
	case "autoconnect-group-id":
		return json.Unmarshal(raw, &cfg.AutoconnectGroupId)
	case "autoconnect-profile-id":
		return json.Unmarshal(raw, &cfg.AutoconnectProfileId)
	case "autoconnect-mode":
		return json.Unmarshal(raw, &cfg.AutoconnectMode)
	case "test-config":
		return json.Unmarshal(raw, &cfg.TestConfig)
	case "ipc-socket-path":
		return json.Unmarshal(raw, &cfg.IPCSocketPath)
	case "tcp-enabled":
		return json.Unmarshal(raw, &cfg.TCPEnabled)
	case "split-tunnel":
		return json.Unmarshal(raw, &cfg.SplitTunnel)
	}
	return nil // unknown keys are ignored, not fatal
}
