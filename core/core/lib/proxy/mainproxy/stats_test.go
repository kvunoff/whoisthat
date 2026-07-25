package mainproxy

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"whoisthat-core/structs"
)

// withSysfsBase substitutes the package-level sysfs root with a caller-provided
// tempdir and restores it on test completion. Returns the substitutes root so
// the caller can populate it.
func withSysfsBase(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	prev := sysfsBase
	sysfsBase = dir
	t.Cleanup(func() { sysfsBase = prev })
	return dir
}

// TestReadSysfsInt_Valid parses a decimal integer followed by a trailing
// newline (the canonical sysfs byte-counter format).
func TestReadSysfsInt_Valid(t *testing.T) {
	dir := withSysfsBase(t)
	path := filepath.Join(dir, "rx_bytes")
	if err := os.WriteFile(path, []byte("654321\n"), 0644); err != nil {
		t.Fatalf("write: %v", err)
	}
	got, err := readSysfsInt(path)
	if err != nil {
		t.Fatalf("readSysfsInt: %v", err)
	}
	if got != 654321 {
		t.Errorf("readSysfsInt = %d, want 654321", got)
	}
}

// TestReadSysfsInt_MissingFile surfaces the ENOENT path — main endpoint of
// readIfaceBytes when the TUN device is not currently up.
func TestReadSysfsInt_MissingFile(t *testing.T) {
	dir := withSysfsBase(t)
	_, err := readSysfsInt(filepath.Join(dir, "nope"))
	if err == nil {
		t.Fatal("readSysfsInt: expected error on missing file, got nil")
	}
}

// TestReadIfaceBytes_ReadsBothCounters creates a mock sysfs tree for one
// interface and verifies rx/tx are both pulled and mapped correctly.
func TestReadIfaceBytes_ReadsBothCounters(t *testing.T) {
	dir := withSysfsBase(t)
	statsDir := filepath.Join(dir, "wt0", "statistics")
	if err := os.MkdirAll(statsDir, 0755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(filepath.Join(statsDir, "rx_bytes"), []byte("1000\n"), 0644); err != nil {
		t.Fatalf("write rx: %v", err)
	}
	if err := os.WriteFile(filepath.Join(statsDir, "tx_bytes"), []byte("2000\n"), 0644); err != nil {
		t.Fatalf("write tx: %v", err)
	}
	rx, tx, err := readIfaceBytes("wt0")
	if err != nil {
		t.Fatalf("readIfaceBytes: %v", err)
	}
	if rx != 1000 {
		t.Errorf("rx = %d, want 1000", rx)
	}
	if tx != 2000 {
		t.Errorf("tx = %d, want 2000", tx)
	}
}

// TestReadIfaceBytes_NoDevice verifies that a missing device directory returns
// an error (rx_bytes file not found). This is the TUN-down case.
func TestReadIfaceBytes_NoDevice(t *testing.T) {
	withSysfsBase(t) // empty tempdir → no interfaces
	_, _, err := readIfaceBytes("nonexistent0")
	if err == nil {
		t.Fatal("readIfaceBytes: expected error for missing device, got nil")
	}
}

// TestReadIfaceBytes_PartialDevice: rx_bytes present but tx_bytes missing
// must surface the error rather than silently returning tx=0.
func TestReadIfaceBytes_PartialDevice(t *testing.T) {
	dir := withSysfsBase(t)
	statsDir := filepath.Join(dir, "wt1", "statistics")
	if err := os.MkdirAll(statsDir, 0755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(filepath.Join(statsDir, "rx_bytes"), []byte("1000\n"), 0644); err != nil {
		t.Fatalf("write rx: %v", err)
	}
	// tx_bytes intentionally absent
	_, _, err := readIfaceBytes("wt1")
	if err == nil {
		t.Fatal("readIfaceBytes: expected error when tx_bytes missing, got nil")
	}
}

// TestCollectStats_CancelStopsLoop verifies that closing the cancel channel
// terminates the collector goroutine promptly. Uses apiPort=0 + tunName=""
// so neither ss nor sysfs actually runs (querySsStats errors out → continue,
// which is fine — we only care about the cancel arm).
func TestCollectStats_CancelStopsLoop(t *testing.T) {
	p := &ProxyManager{}
	cancel := make(chan struct{})
	out := make(chan structs.TrafficStats, 1)
	done := make(chan struct{})
	go func() {
		p.collectStats(0, 0, "", cancel, out)
		close(done)
	}()
	close(cancel)
	select {
	case <-done:
		// good — goroutine returned
	case <-chan struct{}(nil):
		t.Fatal("collectStats did not exit after cancel")
	}
}

// TestInjectStatsConfig_ApiInbound verifies that apiPort>0 adds the api
// service + api dokodemo-door inbound, and that apiPort=0 leaves them out.
func TestInjectStatsConfig_ApiInbound(t *testing.T) {
	base := `{"inbounds":[{"tag":"socks","port":3090,"protocol":"socks"}]}`
	t.Run("apiPort=0", func(t *testing.T) {
		out, err := injectStatsConfig([]byte(base), 0)
		if err != nil {
			t.Fatalf("injectStatsConfig: %v", err)
		}
		var cfg map[string]interface{}
		if err := json.Unmarshal(out, &cfg); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		if _, has := cfg["api"]; has {
			t.Error("api object present with apiPort=0, want absent")
		}
		ibs, _ := cfg["inbounds"].([]interface{})
		if len(ibs) != 1 {
			t.Errorf("inbounds = %d, want 1 (no api inbound)", len(ibs))
		}
	})
	t.Run("apiPort=12345", func(t *testing.T) {
		out, err := injectStatsConfig([]byte(base), 12345)
		if err != nil {
			t.Fatalf("injectStatsConfig: %v", err)
		}
		var cfg map[string]interface{}
		if err := json.Unmarshal(out, &cfg); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		api, ok := cfg["api"].(map[string]interface{})
		if !ok {
			t.Fatal("api object missing")
		}
		if api["tag"] != "api" {
			t.Errorf("api.tag = %v, want \"api\"", api["tag"])
		}
		svcs, _ := api["services"].([]interface{})
		if len(svcs) != 1 || svcs[0] != "StatsService" {
			t.Errorf("api.services = %v, want [StatsService]", svcs)
		}
		ibs, _ := cfg["inbounds"].([]interface{})
		if len(ibs) != 2 {
			t.Fatalf("inbounds = %d, want 2 (socks + api)", len(ibs))
		}
		apiInbound, ok := ibs[1].(map[string]interface{})
		if !ok {
			t.Fatal("second inbound not a map")
		}
		if apiInbound["tag"] != "api" {
			t.Errorf("api inbound tag = %v, want \"api\"", apiInbound["tag"])
		}
		if apiInbound["port"] != 12345.0 {
			t.Errorf("api inbound port = %v, want 12345", apiInbound["port"])
		}
		if apiInbound["protocol"] != "dokodemo-door" {
			t.Errorf("api inbound protocol = %v, want dokodemo-door", apiInbound["protocol"])
		}
	})
}

// TestInjectStatsConfig_NoInbounds verifies the inbound-append path works
// even when the source config has no inbounds at all.
func TestInjectStatsConfig_NoInbounds(t *testing.T) {
	base := `{"outbounds":[{"tag":"proxy"}]}`
	out, err := injectStatsConfig([]byte(base), 9999)
	if err != nil {
		t.Fatalf("injectStatsConfig: %v", err)
	}
	var cfg map[string]interface{}
	if err := json.Unmarshal(out, &cfg); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	ibs, _ := cfg["inbounds"].([]interface{})
	if len(ibs) != 1 {
		t.Fatalf("inbounds = %d, want 1 (api only)", len(ibs))
	}
}