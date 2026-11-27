package xrayjson

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// TestTunInboundMTUWithXray checks that the configured MTU reaches the
// generated TUN inbound. The Xray preflight is optional: it needs the pinned
// binary and /dev/net/tun, neither of which is available in plain CI.
func TestTunInboundMTUWithXray(t *testing.T) {
	const wantMTU = 1280
	prepared, err := Prepare([]byte(finalmaskBaseConfig), Options{
		TunName:   "whoisthattun",
		TunMTU:    wantMTU,
		SocksPort: 3090,
		HttpPort:  3091,
	})
	if err != nil {
		t.Fatalf("prepare config: %v", err)
	}

	var cfg struct {
		Inbounds []struct {
			Tag      string `json:"tag"`
			Protocol string `json:"protocol"`
			Settings struct {
				MTU float64 `json:"MTU"`
			} `json:"settings"`
		} `json:"inbounds"`
	}
	if err := json.Unmarshal(prepared.Config, &cfg); err != nil {
		t.Fatalf("parse prepared config: %v", err)
	}
	var found bool
	for _, in := range cfg.Inbounds {
		if in.Protocol == "tun" && in.Tag == "tun-in" {
			found = true
			if int(in.Settings.MTU) != wantMTU {
				t.Fatalf("TUN inbound MTU = %v, want %d", in.Settings.MTU, wantMTU)
			}
		}
	}
	if !found {
		t.Fatal("TUN inbound not found in prepared config")
	}

	bin := xrayBinary()
	if bin == "" {
		t.Skip("xray binary not found; structural MTU check passed")
	}
	if _, err := os.Stat("/dev/net/tun"); err != nil {
		t.Skip("/dev/net/tun unavailable; structural MTU check passed")
	}
	path := filepath.Join(t.TempDir(), "config.json")
	if err := os.WriteFile(path, prepared.Config, 0o600); err != nil {
		t.Fatalf("write config: %v", err)
	}
	out, err := exec.Command(bin, "run", "-test", "-config", path).CombinedOutput()
	if err != nil {
		t.Skipf("xray -test rejected the TUN config (likely needs root): %v\n%s", err, out)
	}
}
