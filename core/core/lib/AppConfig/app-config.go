package appconfig

import (
	"whoisthat-core/lib/logger"
	"whoisthat-core/utils"
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
)

var tunNameRe = regexp.MustCompile(`^[a-zA-Z][a-zA-Z0-9_-]{0,14}$`)

type AppConfig struct {
	SocksPort     int       `json:"socks-port"`
	HttpPort      int       `json:"http-port"`
	CoreTCPPort   int       `json:"core-tcp-port"`
	TestPortRange PortRange `json:"test-port-range"`
	DnsServers    []string  `json:"dns-servers"`
	TunName       string    `json:"tun-name"`
	HwidEnabled   bool      `json:"hwid-enabled"`
	Hwid          string    `json:"hwid"`
	UserAgent     string    `json:"user-agent"`
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
		DnsServers:  []string{"1.1.1.1", "8.8.8.8", "2606:4700:4700::1111", "2001:4860:4860::8888"},
		TunName:     "whoisthattun",
		HwidEnabled: true,
		UserAgent:   "whoisthat/v0.5.4",
	}
}

func GetConfig() AppConfig {
	return application_configuration
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
