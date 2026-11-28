package appconfig

import (
	"crypto/rand"
	"encoding/base64"
	"encoding/hex"
	"errors"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"

	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/version"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

var tunNameRe = regexp.MustCompile(`^[a-zA-Z][a-zA-Z0-9_-]{0,14}$`)

const (
	maxInt32           = 1<<31 - 1
	maxFragmentLength  = 65535
	maxAntiDPIDelayMs  = 60000
	maxNoisePacketSize = 4096
)

type AppConfig struct {
	SocksPort            int                   `json:"socks-port"`
	HttpPort             int                   `json:"http-port"`
	CoreTCPPort          int                   `json:"core-tcp-port"`
	TestPortRange        PortRange             `json:"test-port-range"`
	DnsServers           []string              `json:"dns-servers"`
	TunName              string                `json:"tun-name"`
	TunMTU               int                   `json:"tun-mtu"`
	HwidEnabled          bool                  `json:"hwid-enabled"`
	Hwid                 string                `json:"hwid"`
	UserAgent            string                `json:"user-agent"`
	KillSwitchEnabled    bool                  `json:"kill-switch-enabled"`
	AutoconnectEnabled   bool                  `json:"autoconnect-enabled"`
	AutoconnectGroupId   int                   `json:"autoconnect-group-id"`
	AutoconnectProfileId int                   `json:"autoconnect-profile-id"`
	AutoconnectMode      string                `json:"autoconnect-mode"`
	TestConfig           TestConfig            `json:"test-config"`
	AntiDPI              structs.AntiDPIConfig `json:"anti-dpi"`
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
		TunMTU:          1400,
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
		AntiDPI: defaultAntiDPIConfig(),
	}
}

func defaultAntiDPIConfig() structs.AntiDPIConfig {
	return structs.AntiDPIConfig{
		Fragment: structs.AntiDPIFragment{
			Packets:  "tlshello",
			Length:   "100-200",
			Interval: "10-20",
		},
		Noise: structs.AntiDPINoise{
			Type:   "rand",
			Packet: "50-100",
			Delay:  "10-16",
		},
	}
}

func defaultNoisePacket(noiseType string) string {
	switch noiseType {
	case "str":
		return "whoisthat"
	case "base64":
		return "d2hvaXN0aGF0"
	case "hex":
		return "77686f697374686174"
	default:
		return "50-100"
	}
}

func SetTestConfig(cfg TestConfig) {
	configMu.Lock()
	defer configMu.Unlock()
	t := sanitizeTestConfig(cfg)
	application_configuration.TestConfig = t
	saveCurrentConfig()
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

func SetAntiDPI(cfg structs.AntiDPIConfig) error {
	configMu.Lock()
	defer configMu.Unlock()
	cfg = normalizeAntiDPI(cfg)
	if err := ValidateAntiDPI(cfg); err != nil {
		return err
	}
	application_configuration.AntiDPI = cfg
	saveCurrentConfig()
	return nil
}

func normalizeAntiDPI(cfg structs.AntiDPIConfig) structs.AntiDPIConfig {
	cfg.Fragment.Packets = strings.TrimSpace(cfg.Fragment.Packets)
	cfg.Fragment.Length = strings.TrimSpace(cfg.Fragment.Length)
	cfg.Fragment.Interval = strings.TrimSpace(cfg.Fragment.Interval)
	cfg.Noise.Type = strings.TrimSpace(cfg.Noise.Type)
	cfg.Noise.Packet = strings.TrimSpace(cfg.Noise.Packet)
	cfg.Noise.Delay = strings.TrimSpace(cfg.Noise.Delay)
	return cfg
}

// ValidateAntiDPI rejects values Xray cannot build or would turn into an
// unbounded allocation/sleep. The upper bounds are deliberately tighter than
// Xray's int32 parser for noise length and delays.
func ValidateAntiDPI(cfg structs.AntiDPIConfig) error {
	if !validPackets(cfg.Fragment.Packets) {
		return fmt.Errorf("anti-dpi.fragment.packets must be tlshello or N-M (N >= 1)")
	}
	if !validRange(cfg.Fragment.Length, 1, maxFragmentLength) {
		return fmt.Errorf("anti-dpi.fragment.length must be N-M with 1 <= N <= M <= %d", maxFragmentLength)
	}
	if !validRange(cfg.Fragment.Interval, 0, maxAntiDPIDelayMs) {
		return fmt.Errorf("anti-dpi.fragment.interval must be N-M with 0 <= N <= M <= %d", maxAntiDPIDelayMs)
	}
	switch cfg.Noise.Type {
	case "rand", "str", "base64", "hex":
	default:
		return fmt.Errorf("anti-dpi.noise.type must be rand, str, base64, or hex")
	}
	if cfg.Noise.Type == "rand" {
		if !validRange(cfg.Noise.Packet, 1, maxNoisePacketSize) {
			return fmt.Errorf("anti-dpi.noise.packet must be N-M with 1 <= N <= M <= %d for rand", maxNoisePacketSize)
		}
	} else if !validNoisePayload(cfg.Noise.Type, cfg.Noise.Packet) {
		return fmt.Errorf("anti-dpi.noise.packet must be a valid non-empty %s payload (max %d bytes)", cfg.Noise.Type, maxNoisePacketSize)
	}
	if !validRange(cfg.Noise.Delay, 0, maxAntiDPIDelayMs) {
		return fmt.Errorf("anti-dpi.noise.delay must be N-M with 0 <= N <= M <= %d", maxAntiDPIDelayMs)
	}
	return nil
}

func sanitizeAntiDPI(cfg structs.AntiDPIConfig) structs.AntiDPIConfig {
	def := defaultAntiDPIConfig()
	cfg = normalizeAntiDPI(cfg)

	if cfg.Fragment.Packets == "" {
		cfg.Fragment.Packets = def.Fragment.Packets
	} else if !validPackets(cfg.Fragment.Packets) {
		logger.Warnf("config: invalid anti-dpi.fragment.packets %q, using default", cfg.Fragment.Packets)
		cfg.Fragment.Packets = def.Fragment.Packets
	}
	if cfg.Fragment.Length == "" {
		cfg.Fragment.Length = def.Fragment.Length
	} else if !validRange(cfg.Fragment.Length, 1, maxFragmentLength) {
		logger.Warnf("config: invalid anti-dpi.fragment.length %q, using default", cfg.Fragment.Length)
		cfg.Fragment.Length = def.Fragment.Length
	}
	if cfg.Fragment.Interval == "" {
		cfg.Fragment.Interval = def.Fragment.Interval
	} else if !validRange(cfg.Fragment.Interval, 0, maxAntiDPIDelayMs) {
		logger.Warnf("config: invalid anti-dpi.fragment.interval %q, using default", cfg.Fragment.Interval)
		cfg.Fragment.Interval = def.Fragment.Interval
	}

	switch cfg.Noise.Type {
	case "":
		cfg.Noise.Type = def.Noise.Type
	case "rand", "str", "base64", "hex":
	default:
		logger.Warnf("config: invalid anti-dpi.noise.type %q, using default", cfg.Noise.Type)
		cfg.Noise.Type = def.Noise.Type
	}
	if cfg.Noise.Packet == "" {
		cfg.Noise.Packet = defaultNoisePacket(cfg.Noise.Type)
	} else if cfg.Noise.Type == "rand" && !validRange(cfg.Noise.Packet, 1, maxNoisePacketSize) {
		logger.Warnf("config: invalid anti-dpi.noise.packet %q, using default", cfg.Noise.Packet)
		cfg.Noise.Packet = defaultNoisePacket(cfg.Noise.Type)
	} else if cfg.Noise.Type != "rand" && !validNoisePayload(cfg.Noise.Type, cfg.Noise.Packet) {
		logger.Warnf("config: invalid anti-dpi.noise.packet for type %q, using default", cfg.Noise.Type)
		cfg.Noise.Packet = defaultNoisePacket(cfg.Noise.Type)
	}
	if cfg.Noise.Delay == "" {
		cfg.Noise.Delay = def.Noise.Delay
	} else if !validRange(cfg.Noise.Delay, 0, maxAntiDPIDelayMs) {
		logger.Warnf("config: invalid anti-dpi.noise.delay %q, using default", cfg.Noise.Delay)
		cfg.Noise.Delay = def.Noise.Delay
	}
	return cfg
}

func validPackets(value string) bool {
	if value == "tlshello" {
		return true
	}
	_, _, ok := parseRange(value, 1, maxInt32)
	return ok
}

func validRange(value string, min, max int64) bool {
	_, _, ok := parseRange(value, min, max)
	return ok
}

func parseRange(value string, min, max int64) (int64, int64, bool) {
	parts := strings.Split(value, "-")
	if len(parts) != 2 {
		return 0, 0, false
	}
	from, err := strconv.ParseInt(strings.TrimSpace(parts[0]), 10, 64)
	if err != nil {
		return 0, 0, false
	}
	to, err := strconv.ParseInt(strings.TrimSpace(parts[1]), 10, 64)
	if err != nil {
		return 0, 0, false
	}
	if from < min || to > max || from > to {
		return 0, 0, false
	}
	return from, to, true
}

func validNoisePayload(noiseType, payload string) bool {
	if payload == "" || len(payload) > maxNoisePacketSize {
		return false
	}
	switch noiseType {
	case "str":
		return true
	case "base64":
		decoded, err := base64.StdEncoding.DecodeString(payload)
		return err == nil && len(decoded) > 0 && len(decoded) <= maxNoisePacketSize
	case "hex":
		decoded, err := hex.DecodeString(payload)
		return err == nil && len(decoded) > 0 && len(decoded) <= maxNoisePacketSize
	default:
		return false
	}
}

func GetConfig() AppConfig {
	configMu.RLock()
	defer configMu.RUnlock()
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
	if err := utils.EnsurePrivateDir(filepath.Dir(path), os.Getuid()); err != nil {
		return "", fmt.Errorf("unsafe IPC token directory %s: %w", filepath.Dir(path), err)
	}
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
	if err := utils.AtomicWriteFile(path, []byte(token+"\n"), 0600); err != nil {
		return "", fmt.Errorf("failed to write IPC token %s: %w", path, err)
	}
	return token, nil
}

func SocketPath() string {
	if p := strings.TrimSpace(GetConfig().IPCSocketPath); p != "" {
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
	configMu.Lock()
	defer configMu.Unlock()
	application_configuration.KillSwitchEnabled = enabled
	saveCurrentConfig()
}

// SetSplitTunnelMode persists the split-tunnel mode. Invalid values are coerced
// to "off" (with a warning) so a bad client value cannot install bogus routing.
func SetSplitTunnelMode(mode string) string {
	configMu.Lock()
	defer configMu.Unlock()
	switch mode {
	case "off", "exclude", "include":
	default:
		if mode != "" {
			logger.Warnf("config: invalid split-tunnel mode %q, using off", mode)
		}
		mode = "off"
	}
	application_configuration.SplitTunnel.Mode = mode
	saveCurrentConfig()
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

func SetAutoconnect(enabled bool, groupId, profileId int, mode string) error {
	configMu.Lock()
	defer configMu.Unlock()
	next := application_configuration
	next.AutoconnectEnabled = enabled
	next.AutoconnectGroupId = groupId
	next.AutoconnectProfileId = profileId
	next.AutoconnectMode = sanitizeAutoconnectMode(mode)
	if err := saveConfig(next); err != nil {
		return err
	}
	application_configuration = next
	return nil
}

func sanitizeTunName(name string) string {
	if name == "" || !tunNameRe.MatchString(name) {
		logger.Warnf("config: invalid tun name %q, using default", name)
		return defaultConfig().TunName
	}
	return name
}

func SetTunName(name string) error {
	configMu.Lock()
	defer configMu.Unlock()
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
	saveCurrentConfig()
	return nil
}

const (
	// 1280 is the minimum IPv6 link MTU (RFC 8200); 1500 is Ethernet. Larger
	// values need a jumbo-capable underlay and are not supported here.
	minTunMTU = 1280
	maxTunMTU = 1500
)

// SetNetworkSettings persists the whole TUN/DNS block atomically. Clients read
// the current application-state and only change the field they edit.
func SetNetworkSettings(cfg structs.NetworkSettings) error {
	configMu.Lock()
	defer configMu.Unlock()
	norm, err := validateNetworkSettings(cfg)
	if err != nil {
		return err
	}
	application_configuration.TunMTU = norm.TunMTU
	application_configuration.DnsServers = norm.DnsServers
	saveCurrentConfig()
	return nil
}

func validateNetworkSettings(cfg structs.NetworkSettings) (structs.NetworkSettings, error) {
	if cfg.TunMTU < minTunMTU || cfg.TunMTU > maxTunMTU {
		return cfg, fmt.Errorf("tun-mtu must be in %d..%d", minTunMTU, maxTunMTU)
	}
	seen := make(map[string]bool)
	servers := make([]string, 0, len(cfg.DnsServers))
	for _, raw := range cfg.DnsServers {
		s := strings.TrimSpace(raw)
		if s == "" || net.ParseIP(s) == nil {
			return cfg, fmt.Errorf("dns-servers contains an invalid IP address %q", raw)
		}
		if !seen[s] {
			seen[s] = true
			servers = append(servers, s)
		}
	}
	if len(servers) == 0 {
		return cfg, errors.New("dns-servers must not be empty")
	}
	cfg.DnsServers = servers
	return cfg, nil
}

func sanitizeTunMTU(mtu int) int {
	if mtu < minTunMTU || mtu > maxTunMTU {
		if mtu != 0 {
			logger.Warnf("config: invalid tun-mtu %d, using default", mtu)
		}
		return defaultConfig().TunMTU
	}
	return mtu
}
