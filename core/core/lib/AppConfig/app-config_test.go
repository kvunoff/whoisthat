package appconfig

import (
	"net"
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

func TestSanitizeDnsServersKeepsValidDropsInvalid(t *testing.T) {
	in := []string{
		"1.1.1.1",
		"2606:4700:4700::1111",
		"8.8.8.8\" || touch /tmp/pwned || echo \"", // shell injection attempt
		"not-an-ip",
		"",
	}
	got := sanitizeDnsServers(in)
	want := []string{"1.1.1.1", "2606:4700:4700::1111"}
	if len(got) != len(want) {
		t.Fatalf("sanitizeDnsServers(%v) = %v, want %v", in, got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("got[%d] = %q, want %q", i, got[i], want[i])
		}
	}
}

func TestSanitizeDnsServersFallsBackToDefaults(t *testing.T) {
	got := sanitizeDnsServers([]string{"garbage", ""})
	if len(got) == 0 {
		t.Fatal("expected fallback to defaults, got empty slice")
	}
	for _, s := range got {
		if net.ParseIP(s) == nil {
			t.Errorf("fallback returned non-IP entry %q", s)
		}
	}
}

func TestDefaultConfigAutoconnect(t *testing.T) {
	cfg := defaultConfig()
	if cfg.AutoconnectEnabled {
		t.Error("AutoconnectEnabled should default to false")
	}
	if cfg.AutoconnectMode != "proxy" {
		t.Errorf("AutoconnectMode = %q, want \"proxy\"", cfg.AutoconnectMode)
	}
	if cfg.AutoconnectGroupId != 0 || cfg.AutoconnectProfileId != 0 {
		t.Error("AutoconnectGroupId/ProfileId should default to 0")
	}
}

func TestSanitizeAutoconnectMode(t *testing.T) {
	tests := []struct {
		in, want string
	}{
		{"proxy", "proxy"},
		{"tun", "tun"},
		{"", "proxy"},
		{"foo", "proxy"},
		{"PROXY", "proxy"},
	}
	for _, tt := range tests {
		got := sanitizeAutoconnectMode(tt.in)
		if got != tt.want {
			t.Errorf("sanitizeAutoconnectMode(%q) = %q, want %q", tt.in, got, tt.want)
		}
	}
}

func TestDefaultConfigTestConfig(t *testing.T) {
	cfg := defaultConfig()
	tc := cfg.TestConfig
	if tc.Concurrency != 16 {
		t.Errorf("TestConfig.Concurrency = %d, want 16", tc.Concurrency)
	}
	if tc.TimeoutSeconds != 5 {
		t.Errorf("TestConfig.TimeoutSeconds = %d, want 5", tc.TimeoutSeconds)
	}
	if tc.SamplesPerTest != 3 {
		t.Errorf("TestConfig.SamplesPerTest = %d, want 3", tc.SamplesPerTest)
	}
	if tc.TestEndpoint == "" {
		t.Error("TestConfig.TestEndpoint must not be empty")
	}
	if !tc.AutoTestOnSub {
		t.Error("TestConfig.AutoTestOnSub should default to true")
	}
}

func TestSanitizeTestConfigFallsBackOnInvalid(t *testing.T) {
	// Plant known-good defaults first.
	application_configuration.TestConfig = defaultConfig().TestConfig
	got := sanitizeTestConfig(TestConfig{
		Concurrency:    0,
		TimeoutSeconds: 0,
		SamplesPerTest: 0,
		TestEndpoint:   "",
		AutoTestOnSub:  false,
	})
	if got.Concurrency != 16 {
		t.Errorf("Concurrency fallback = %d, want 16", got.Concurrency)
	}
	if got.TimeoutSeconds != 5 {
		t.Errorf("TimeoutSeconds fallback = %d, want 5", got.TimeoutSeconds)
	}
	if got.SamplesPerTest != 3 {
		t.Errorf("SamplesPerTest fallback = %d, want 3", got.SamplesPerTest)
	}
	if got.TestEndpoint == "" {
		t.Error("TestEndpoint fallback should preserve default URL")
	}
}

func TestSanitizeTestConfigRejectsNonHttpEndpoint(t *testing.T) {
	application_configuration.TestConfig = defaultConfig().TestConfig
	got := sanitizeTestConfig(TestConfig{
		Concurrency:    8,
		TimeoutSeconds: 10,
		SamplesPerTest: 1,
		TestEndpoint:   "file:///etc/passwd", // non-http scheme, must be rejected
		AutoTestOnSub:  true,
	})
	if got.TestEndpoint == "file:///etc/passwd" {
		t.Error("non-http(s) endpoint must be rejected and fall back to default")
	}
	if got.Concurrency != 8 {
		t.Errorf("Concurrency should have been accepted: got %d", got.Concurrency)
	}
}
