package appconfig

import (
	"crypto/rand"
	"encoding/hex"
	"net"
	"os"
	"os/exec"
	"runtime"
	"strings"

	"whoisthat-core/lib/logger"
)

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
