package appconfig

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"

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
				utils.EnsurePerms(path, 0600)
				return token, nil
			}
		}
	}
	raw := make([]byte, 32)
	if _, err := rand.Read(raw); err != nil {
		return "", fmt.Errorf("failed to generate IPC token: %w", err)
	}
	token := hex.EncodeToString(raw)
	utils.EnsurePerms(filepath.Dir(path), 0700)
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
	return filepath.Join(utils.RuntimeDir(), "core.sock")
}

// LoadStatus tells the TUI whether config.json could be read. States:
// "ok" (loaded), "missing" (first start), "corrupt" (unreadable; the running
// config holds whatever could be recovered and the file was not written),
// "orphaned" (valid JSON without a device id, i.e. written by something that
// never loaded the config — old tooling or a test), "repaired" (restored from
// config.json.bak).

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
