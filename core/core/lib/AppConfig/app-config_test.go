package appconfig

import (
	"regexp"
	"testing"
)

func TestDefaultConfigPorts(t *testing.T) {
	cfg := defaultConfig()
	if cfg.SocksPort != 3090 {
		t.Errorf("SocksPort = %d, want 3090", cfg.SocksPort)
	}
	if cfg.HttpPort != 3091 {
		t.Errorf("HttpPort = %d, want 3091", cfg.HttpPort)
	}
	if cfg.CoreTCPPort != 4897 {
		t.Errorf("CoreTCPPort = %d, want 4897", cfg.CoreTCPPort)
	}
}

func TestDefaultConfigTestPortRange(t *testing.T) {
	cfg := defaultConfig()
	if cfg.TestPortRange.Start != 3095 {
		t.Errorf("TestPortRange.Start = %d, want 3095", cfg.TestPortRange.Start)
	}
	if cfg.TestPortRange.End != 30120 {
		t.Errorf("TestPortRange.End = %d, want 30120", cfg.TestPortRange.End)
	}
	if cfg.TestPortRange.Start >= cfg.TestPortRange.End {
		t.Error("TestPortRange.Start must be less than End")
	}
}

func TestDefaultConfigDnsServers(t *testing.T) {
	cfg := defaultConfig()
	if len(cfg.DnsServers) == 0 {
		t.Fatal("DnsServers must not be empty")
	}
	for _, dns := range cfg.DnsServers {
		if dns == "" {
			t.Error("DnsServers contains empty string")
		}
	}
}

func TestDefaultConfigHwidEnabled(t *testing.T) {
	cfg := defaultConfig()
	if !cfg.HwidEnabled {
		t.Error("HwidEnabled should default to true")
	}
}

var hexPattern = regexp.MustCompile(`^[0-9a-f]{16}$`)

func TestGenerateHwidFormat(t *testing.T) {
	hwid := generateHwid()
	if !hexPattern.MatchString(hwid) {
		t.Errorf("generateHwid() = %q, want 16 lowercase hex chars", hwid)
	}
}

func TestGenerateHwidIsRandom(t *testing.T) {
	h1 := generateHwid()
	h2 := generateHwid()
	if h1 == h2 {
		t.Error("two calls to generateHwid() returned identical values")
	}
}

func TestDefaultConfigUserAgent(t *testing.T) {
	cfg := defaultConfig()
	if cfg.UserAgent == "" {
		t.Error("UserAgent must not be empty")
	}
}
