package mainproxy

import (
	"encoding/json"
	"strings"
	"testing"
	"whoisthat-core/structs"
)

const preflightTestURI = "vless://uuid@example.com:443?test=1#Preflight"

func TestBuildPreflightConfigOmitsTunInbound(t *testing.T) {
	profile := structs.Profile{Id: 1, GroupId: 1, Name: "p", Uri: preflightTestURI}
	selection := structs.RoutingSelection{
		Source: "manual",
		Config: structs.RoutingConfig{
			DomainStrategy: "IPIfNonMatch",
			Rules: []structs.RoutingRule{
				{Type: "field", Domain: "example.com", OutboundTag: "direct", Enabled: true},
			},
		},
	}

	raw, err := buildPreflightConfig(profile, 3090, 3091, 0, selection, true)
	if err != nil {
		t.Fatalf("buildPreflightConfig failed: %v", err)
	}

	var cfg map[string]interface{}
	if err := json.Unmarshal(raw, &cfg); err != nil {
		t.Fatalf("preflight config is not valid JSON: %v", err)
	}

	inbounds, _ := cfg["inbounds"].([]interface{})
	for _, ib := range inbounds {
		m, _ := ib.(map[string]interface{})
		if m["protocol"] == "tun" || m["tag"] == "tun-in" {
			t.Fatalf("preflight config must not contain the tun inbound: %v", m)
		}
	}

	// Routing must still be injected so a broken rule set is caught.
	if _, ok := cfg["routing"]; !ok {
		t.Errorf("routing section missing from preflight config")
	}
}

func TestBuildPreflightConfigWithoutRouting(t *testing.T) {
	profile := structs.Profile{Id: 1, GroupId: 1, Name: "p", Uri: preflightTestURI}
	raw, err := buildPreflightConfig(profile, 3090, 3091, 0, structs.RoutingSelection{}, false)
	if err != nil {
		t.Fatalf("buildPreflightConfig failed: %v", err)
	}

	var cfg map[string]interface{}
	if err := json.Unmarshal(raw, &cfg); err != nil {
		t.Fatalf("preflight config is not valid JSON: %v", err)
	}
	if _, ok := cfg["routing"]; ok {
		t.Errorf("routing must not be injected when withRouting is false")
	}
}

// Integration check against the real binary. Skips when xray is not installed
// (e.g. CI without the external runtime).
func TestValidateXrayConfigIntegration(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping xray binary integration test in -short mode")
	}
	if xrayTestBinary() == "" {
		t.Skip("xray binary not available")
	}

	profile := structs.Profile{Id: 1, GroupId: 1, Name: "p", Uri: preflightTestURI}
	selection := structs.RoutingSelection{
		Source: "manual",
		Config: structs.RoutingConfig{DomainStrategy: "IPIfNonMatch"},
	}
	good, err := buildPreflightConfig(profile, 0, 0, 0, selection, true)
	if err != nil {
		t.Fatalf("buildPreflightConfig failed: %v", err)
	}
	if err := validateXrayConfig(good, ""); err != nil {
		t.Fatalf("valid config rejected: %v", err)
	}

	err = validateXrayConfig([]byte("{ this is not valid json"), "")
	if err == nil {
		t.Fatal("expected invalid config to be rejected")
	}
	if !strings.Contains(err.Error(), "xray config test failed") {
		t.Errorf("unexpected error shape: %v", err)
	}
}

func TestSummarizeXrayTestOutput(t *testing.T) {
	t.Run("keeps the tail", func(t *testing.T) {
		out := []byte(strings.Join([]string{
			"line1", "line2", "line3", "line4", "line5", "line6",
			"Failed to start: main: failed to load config",
		}, "\n"))
		got := summarizeXrayTestOutput(out)
		if !strings.Contains(got, "Failed to start") {
			t.Errorf("tail error lost: %q", got)
		}
		if strings.Contains(got, "line1") {
			t.Errorf("old lines should be dropped: %q", got)
		}
		if strings.Contains(got, "\n") {
			t.Errorf("output must be collapsed to one line: %q", got)
		}
	})

	t.Run("empty", func(t *testing.T) {
		if got := summarizeXrayTestOutput(nil); got != "(no output)" {
			t.Errorf("got %q", got)
		}
	})

	t.Run("truncates long output", func(t *testing.T) {
		long := strings.Repeat("x", xrayPreflightMaxOutput*2)
		got := summarizeXrayTestOutput([]byte(long))
		if len(got) > xrayPreflightMaxOutput+len("…") {
			t.Errorf("output not truncated: %d bytes", len(got))
		}
	})
}
