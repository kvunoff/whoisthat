package appconfig

import (
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
	"whoisthat-core/lib/logger"
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
	// TCPEnabled additionally opens the legacy unauthenticated TCP listener on
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
		UserAgent:       "whoisthat/v0.8.3",
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

func LoadConfig() {
	config, err := readConfig()
	if err != nil {
		logger.Warn("failed to read config file:", err, "using default config")
	}
	if config.HwidEnabled && config.Hwid == "" {
		config.Hwid = generateHwid()
	}
	config.DnsServers = sanitizeDnsServers(config.DnsServers)
	config.TunName = sanitizeTunName(config.TunName)
	config.AutoconnectMode = sanitizeAutoconnectMode(config.AutoconnectMode)
	config.TestConfig = sanitizeTestConfig(config.TestConfig)
	application_configuration = config
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
	home_dir, err := utils.GetHomeDir()
	if err != nil {
		logger.Warn("save config: cannot get home dir:", err)
		return
	}
	var dir_path = filepath.Join(home_dir, ".config", "whoisthat")
	var config_path = filepath.Join(dir_path, "config.json")

	if err := os.MkdirAll(dir_path, 0777); err != nil {
		logger.Warn("save config: cannot create dir:", err)
		return
	}

	jsonData, err := json.MarshalIndent(application_configuration, "", " ")
	if err != nil {
		logger.Warn("save config: marshal failed:", err)
		return
	}

	if err := os.WriteFile(config_path, jsonData, 0666); err != nil {
		logger.Warn("save config: write failed:", err)
	}
}

func readConfig() (AppConfig, error) {
	var default_config AppConfig = defaultConfig()
	home_dir, err := utils.GetHomeDir()
	if err != nil {
		return default_config, err
	}
	var dir_path = filepath.Join(home_dir, ".config", "whoisthat")
	var config_path = filepath.Join(dir_path, "config.json")
	file_bytes, err := os.ReadFile(config_path)
	if err == nil {
		if err := json.Unmarshal(file_bytes, &default_config); err != nil {
			return default_config, fmt.Errorf("failed to parse config file, invalid json")
		}
		return default_config, nil
	}

	if !os.IsNotExist(err) {
		return default_config, fmt.Errorf("failed to read config file")
	}

	return default_config, nil
}
