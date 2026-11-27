package appconfig

import (
	"encoding/json"
	"net"
	"os"
	"path/filepath"
	"regexp"
	"testing"
	"whoisthat-core/lib/version"
	"whoisthat-core/structs"
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
	want := "whoisthat/v" + version.CoreVersion
	if cfg.UserAgent != want {
		t.Errorf("UserAgent = %q, want %q", cfg.UserAgent, want)
	}
}

func TestMigrateUserAgent(t *testing.T) {
	cases := []struct {
		in      string
		want    string
		changed bool
	}{
		{"whoisthat-core/v0.3.8", "whoisthat/v0.3.8", true},
		{"whoisthat-core/custom", "whoisthat/custom", true},
		{"whoisthat/v0.3.8", "whoisthat/v0.3.8", false},
		{"Happ/1.0", "Happ/1.0", false},
		{"", "", false},
	}
	for _, c := range cases {
		got, changed := migrateUserAgent(c.in)
		if got != c.want || changed != c.changed {
			t.Errorf("migrateUserAgent(%q) = (%q, %v), want (%q, %v)",
				c.in, got, changed, c.want, c.changed)
		}
	}
}

func TestLoadConfigMigratesLegacyUserAgent(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	configDir := filepath.Join(dir, "whoisthat")
	if err := os.MkdirAll(configDir, 0700); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(configDir, "config.json")
	legacy := `{"hwid-enabled": false, "user-agent": "whoisthat-core/v0.3.8"}`
	if err := os.WriteFile(path, []byte(legacy), 0600); err != nil {
		t.Fatal(err)
	}

	old := application_configuration
	t.Cleanup(func() { application_configuration = old })

	LoadConfig()
	if got := GetConfig().UserAgent; got != "whoisthat/v0.3.8" {
		t.Fatalf("UserAgent = %q, want whoisthat/v0.3.8", got)
	}

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var saved AppConfig
	if err := json.Unmarshal(data, &saved); err != nil {
		t.Fatal(err)
	}
	if saved.UserAgent != "whoisthat/v0.3.8" {
		t.Errorf("persisted UserAgent = %q, want whoisthat/v0.3.8", saved.UserAgent)
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

func TestDefaultSocketPathHonorsXDG(t *testing.T) {
	t.Setenv("XDG_RUNTIME_DIR", "/run/user/1234")
	got := DefaultSocketPath()
	want := "/run/user/1234/whoisthat/core.sock"
	if got != want {
		t.Errorf("DefaultSocketPath() = %q, want %q", got, want)
	}
}

func TestDefaultSocketPathFallsBackToTmp(t *testing.T) {
	t.Setenv("XDG_RUNTIME_DIR", "")
	got := DefaultSocketPath()
	// uid varies by test runner; assert the shape only. RuntimeDir() appends
	// whoisthat to the base, so the /tmp fallback is nested one level deeper.
	if !regexp.MustCompile(`^/tmp/whoisthat-\d+/whoisthat/core\.sock$`).MatchString(got) {
		t.Errorf("DefaultSocketPath() = %q, want /tmp/whoisthat-<uid>/whoisthat/core.sock", got)
	}
}

func TestSocketPathHonorsOverride(t *testing.T) {
	application_configuration = defaultConfig()
	application_configuration.IPCSocketPath = "/custom/where.sock"
	defer func() { application_configuration = defaultConfig() }()
	if got := SocketPath(); got != "/custom/where.sock" {
		t.Errorf("SocketPath() = %q, want /custom/where.sock", got)
	}
}

func TestTCPDisabledByDefault(t *testing.T) {
	if defaultConfig().TCPEnabled {
		t.Error("TCPEnabled should default to false (UDS is the secure default)")
	}
}

func TestDefaultConfigAntiDPI(t *testing.T) {
	got := defaultConfig().AntiDPI
	if got.Fragment.Enabled || got.Noise.Enabled {
		t.Error("anti-DPI must default to off")
	}
	if got.Fragment.Packets != "tlshello" || got.Fragment.Length != "100-200" || got.Fragment.Interval != "10-20" {
		t.Errorf("unexpected fragment defaults: %+v", got.Fragment)
	}
	if got.Noise.Type != "rand" || got.Noise.Packet != "50-100" || got.Noise.Delay != "10-16" {
		t.Errorf("unexpected noise defaults: %+v", got.Noise)
	}
}

func TestValidateAntiDPI(t *testing.T) {
	valid := func() structs.AntiDPIConfig {
		cfg := defaultAntiDPIConfig()
		cfg.Fragment.Enabled = true
		cfg.Noise.Enabled = true
		return cfg
	}
	if err := ValidateAntiDPI(valid()); err != nil {
		t.Fatalf("defaults must validate: %v", err)
	}

	tests := []struct {
		name   string
		mutate func(*structs.AntiDPIConfig)
	}{
		{"empty packets", func(c *structs.AntiDPIConfig) { c.Fragment.Packets = "" }},
		{"zero packets", func(c *structs.AntiDPIConfig) { c.Fragment.Packets = "0-1" }},
		{"reversed packets", func(c *structs.AntiDPIConfig) { c.Fragment.Packets = "3-1" }},
		{"packets overflow", func(c *structs.AntiDPIConfig) { c.Fragment.Packets = "1-2147483648" }},
		{"zero length", func(c *structs.AntiDPIConfig) { c.Fragment.Length = "0-100" }},
		{"length overflow", func(c *structs.AntiDPIConfig) { c.Fragment.Length = "1-65536" }},
		{"negative interval", func(c *structs.AntiDPIConfig) { c.Fragment.Interval = "-1-5" }},
		{"interval sleep", func(c *structs.AntiDPIConfig) { c.Fragment.Interval = "0-60001" }},
		{"bad noise type", func(c *structs.AntiDPIConfig) { c.Noise.Type = "bogus" }},
		{"zero rand", func(c *structs.AntiDPIConfig) { c.Noise.Packet = "0-10" }},
		{"rand oom", func(c *structs.AntiDPIConfig) { c.Noise.Packet = "1-4097" }},
		{"bad hex", func(c *structs.AntiDPIConfig) { c.Noise.Type = "hex"; c.Noise.Packet = "zz" }},
		{"bad base64", func(c *structs.AntiDPIConfig) { c.Noise.Type = "base64"; c.Noise.Packet = "???" }},
		{"empty str", func(c *structs.AntiDPIConfig) { c.Noise.Type = "str"; c.Noise.Packet = "" }},
		{"delay sleep", func(c *structs.AntiDPIConfig) { c.Noise.Delay = "0-60001" }},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			cfg := valid()
			tt.mutate(&cfg)
			if err := ValidateAntiDPI(cfg); err == nil {
				t.Errorf("expected validation error for %+v", cfg)
			}
		})
	}

	for _, packets := range []string{"tlshello", "1-1", "1-3", "5-5"} {
		cfg := valid()
		cfg.Fragment.Packets = packets
		if err := ValidateAntiDPI(cfg); err != nil {
			t.Errorf("packets %q must validate: %v", packets, err)
		}
	}
	for _, noiseType := range []string{"str", "base64", "hex"} {
		cfg := valid()
		cfg.Noise.Type = noiseType
		cfg.Noise.Packet = defaultNoisePacket(noiseType)
		if err := ValidateAntiDPI(cfg); err != nil {
			t.Errorf("noise type %q must validate: %v", noiseType, err)
		}
	}
}

func TestSanitizeAntiDPIFallsBackPerField(t *testing.T) {
	cfg := sanitizeAntiDPI(structs.AntiDPIConfig{
		Fragment: structs.AntiDPIFragment{
			Enabled:  true,
			Packets:  "0-1",
			Length:   "100-200",
			Interval: "999999",
		},
		Noise: structs.AntiDPINoise{
			Enabled: true,
			Type:    "hex",
			Packet:  "50-100",
			Delay:   "10-16",
		},
	})
	if cfg.Fragment.Packets != "tlshello" {
		t.Errorf("packets fallback = %q", cfg.Fragment.Packets)
	}
	if cfg.Fragment.Length != "100-200" {
		t.Errorf("valid length must be kept, got %q", cfg.Fragment.Length)
	}
	if cfg.Fragment.Interval != "10-20" {
		t.Errorf("interval fallback = %q", cfg.Fragment.Interval)
	}
	if cfg.Noise.Packet != "77686f697374686174" {
		t.Errorf("hex packet fallback = %q", cfg.Noise.Packet)
	}
	if !cfg.Fragment.Enabled || !cfg.Noise.Enabled {
		t.Error("enabled flags must be preserved")
	}
}

func TestSetAntiDPIPersists(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("XDG_DATA_HOME", filepath.Join(dir, "data"))

	old := application_configuration
	t.Cleanup(func() { application_configuration = old })
	LoadConfig()

	cfg := defaultAntiDPIConfig()
	cfg.Fragment.Enabled = true
	cfg.Noise.Enabled = true
	cfg.Noise.Type = "hex"
	cfg.Noise.Packet = "00ff"
	if err := SetAntiDPI(cfg); err != nil {
		t.Fatalf("SetAntiDPI: %v", err)
	}

	data, err := os.ReadFile(filepath.Join(dir, "whoisthat", "config.json"))
	if err != nil {
		t.Fatal(err)
	}
	var saved AppConfig
	if err := json.Unmarshal(data, &saved); err != nil {
		t.Fatal(err)
	}
	if !saved.AntiDPI.Fragment.Enabled || !saved.AntiDPI.Noise.Enabled {
		t.Errorf("enabled flags were not persisted: %+v", saved.AntiDPI)
	}
	if saved.AntiDPI.Noise.Packet != "00ff" {
		t.Errorf("noise packet was not persisted: %+v", saved.AntiDPI.Noise)
	}
}

func TestParseConfigAntiDPIDefaultsAndInvalidBlock(t *testing.T) {
	cfg, status := parseConfig([]byte(`{"hwid-enabled":false}`))
	if status.State != "ok" {
		t.Fatalf("unexpected status: %+v", status)
	}
	if cfg.AntiDPI.Fragment.Packets != "tlshello" || cfg.AntiDPI.Noise.Type != "rand" {
		t.Errorf("missing anti-dpi block must use defaults: %+v", cfg.AntiDPI)
	}

	cfg, _ = parseConfig([]byte(`{"anti-dpi":"not-an-object"}`))
	if cfg.AntiDPI.Fragment.Packets != "tlshello" || cfg.AntiDPI.Noise.Type != "rand" {
		t.Errorf("invalid anti-dpi block must use defaults: %+v", cfg.AntiDPI)
	}
}

func TestDefaultNetworkSettings(t *testing.T) {
	cfg := defaultConfig()
	if cfg.TunMTU != 1400 {
		t.Errorf("TunMTU = %d, want 1400", cfg.TunMTU)
	}
	if len(cfg.DnsServers) == 0 {
		t.Fatal("DnsServers must not be empty")
	}
}

func TestValidateNetworkSettings(t *testing.T) {
	base := structs.NetworkSettings{
		TunMTU:     1280,
		DnsServers: []string{"9.9.9.9", "2620:fe::fe"},
	}
	for name, mutate := range map[string]func(*structs.NetworkSettings){
		"mtu too small": func(c *structs.NetworkSettings) { c.TunMTU = 1279 },
		"mtu too large": func(c *structs.NetworkSettings) { c.TunMTU = 1501 },
		"empty dns":     func(c *structs.NetworkSettings) { c.DnsServers = nil },
		"invalid dns": func(c *structs.NetworkSettings) {
			c.DnsServers = []string{"not-an-ip"}
		},
	} {
		cfg := base
		mutate(&cfg)
		if _, err := validateNetworkSettings(cfg); err == nil {
			t.Errorf("%s: expected an error", name)
		}
	}

	got, err := validateNetworkSettings(structs.NetworkSettings{
		TunMTU:     1500,
		DnsServers: []string{" 9.9.9.9 ", "9.9.9.9", " 2620:fe::fe "},
	})
	if err != nil {
		t.Fatalf("valid settings rejected: %v", err)
	}
	if got.TunMTU != 1500 || len(got.DnsServers) != 2 {
		t.Fatalf("unexpected normalized settings: %+v", got)
	}
	if got.DnsServers[0] != "9.9.9.9" || got.DnsServers[1] != "2620:fe::fe" {
		t.Errorf("trim/dedup failed: %+v", got.DnsServers)
	}
}

func TestSetNetworkSettingsKeepsBothFields(t *testing.T) {
	original := application_configuration
	wasLoaded := loaded
	defer func() {
		application_configuration = original
		loaded = wasLoaded
	}()
	loaded = false

	first := structs.NetworkSettings{TunMTU: 1280, DnsServers: []string{"9.9.9.9"}}
	if err := SetNetworkSettings(first); err != nil {
		t.Fatalf("first set: %v", err)
	}
	second := structs.NetworkSettings{TunMTU: 1500, DnsServers: []string{"9.9.9.9"}}
	if err := SetNetworkSettings(second); err != nil {
		t.Fatalf("second set: %v", err)
	}
	if application_configuration.TunMTU != 1500 {
		t.Errorf("TunMTU = %d, want 1500", application_configuration.TunMTU)
	}
	if len(application_configuration.DnsServers) != 1 || application_configuration.DnsServers[0] != "9.9.9.9" {
		t.Errorf("DnsServers changed unexpectedly: %+v", application_configuration.DnsServers)
	}
}
