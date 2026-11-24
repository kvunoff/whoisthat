package xrayjson

import (
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"testing"

	"whoisthat-core/structs"
)

const finalmaskBaseConfig = `{"outbounds":[{"tag":"proxy","protocol":"vless","settings":{"vnext":[{"address":"127.0.0.1","port":443,"users":[{"id":"00000000-0000-0000-0000-000000000000","encryption":"none"}]}]},"streamSettings":{"network":"tcp","security":"tls","tlsSettings":{"serverName":"example.com"}}}]}`

func xrayBinary() string {
	if p := os.Getenv("WHOISTHAT_XRAY_BIN"); p != "" {
		return p
	}
	home, _ := os.UserHomeDir()
	matches, _ := filepath.Glob(filepath.Join(home, ".local/share/whoisthat/runtimes/xray/v*/xray"))
	if len(matches) > 0 {
		sort.Slice(matches, func(i, j int) bool {
			return versionLess(matches[i], matches[j])
		})
		return matches[len(matches)-1]
	}
	if p, err := exec.LookPath("xray"); err == nil {
		return p
	}
	return ""
}

func versionLess(a, b string) bool {
	av, bv := versionParts(a), versionParts(b)
	for i := 0; i < len(av) && i < len(bv); i++ {
		if av[i] != bv[i] {
			return av[i] < bv[i]
		}
	}
	return len(av) < len(bv)
}

func versionParts(path string) []int {
	base := strings.TrimPrefix(filepath.Base(filepath.Dir(path)), "v")
	parts := strings.Split(base, ".")
	out := make([]int, 0, len(parts))
	for _, part := range parts {
		n, _ := strconv.Atoi(part)
		out = append(out, n)
	}
	return out
}

// TestFinalmaskConfigsWithXray validates every fragment/noise combination with
// the pinned Xray binary. It skips when no binary is installed; `make test-xray`
// wires the managed runtime explicitly.
func TestFinalmaskConfigsWithXray(t *testing.T) {
	bin := xrayBinary()
	if bin == "" {
		t.Skip("xray binary not found; set WHOISTHAT_XRAY_BIN or run make test-xray")
	}

	packetPresets := []string{"tlshello", "1-1", "1-2", "1-3"}
	noiseTypes := map[string]string{
		"rand":   "50-100",
		"str":    "whoisthat",
		"base64": "d2hvaXN0aGF0",
		"hex":    "77686f697374686174",
	}
	off := structs.AntiDPIConfig{}
	raw := []byte(finalmaskBaseConfig)
	if out, err := ApplyAntiDPI(raw, off); err != nil || string(out) != string(raw) {
		t.Fatalf("off must be a no-op: %v", err)
	}

	for _, packets := range packetPresets {
		for noiseType, packet := range noiseTypes {
			name := packets + "/" + noiseType
			t.Run(name, func(t *testing.T) {
				cfg := structs.AntiDPIConfig{
					Fragment: structs.AntiDPIFragment{
						Enabled:  true,
						Packets:  packets,
						Length:   "100-200",
						Interval: "10-20",
					},
					Noise: structs.AntiDPINoise{
						Enabled: true,
						Type:    noiseType,
						Packet:  packet,
						Delay:   "10-16",
					},
				}
				got, err := ApplyAntiDPI(raw, cfg)
				if err != nil {
					t.Fatalf("ApplyAntiDPI: %v", err)
				}
				if err := runXrayTest(t, bin, got); err != nil {
					t.Fatalf("xray rejected fragment/noise combination: %v", err)
				}
			})
		}
	}

	t.Run("unknown mask fails closed", func(t *testing.T) {
		bogus := []byte(`{"outbounds":[{"tag":"proxy","protocol":"vless","settings":{"vnext":[{"address":"127.0.0.1","port":443,"users":[{"id":"00000000-0000-0000-0000-000000000000","encryption":"none"}]}]},"streamSettings":{"network":"tcp","finalmask":{"udp":[{"type":"bogus"}]}}}]}`)
		if err := runXrayTest(t, bin, bogus); err == nil {
			t.Fatal("bogus finalmask must be rejected by Xray")
		}
	})
}

func runXrayTest(t *testing.T, bin string, config []byte) error {
	t.Helper()
	path := filepath.Join(t.TempDir(), "config.json")
	if err := os.WriteFile(path, config, 0600); err != nil {
		t.Fatalf("write config: %v", err)
	}
	cmd := exec.Command(bin, "run", "-test", "-c", path)
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Logf("xray output: %s", strings.TrimSpace(string(out)))
	}
	return err
}
