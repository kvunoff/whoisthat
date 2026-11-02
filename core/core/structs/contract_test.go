package structs

import (
	"encoding/json"
	"flag"
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

// The Rust client and the Go daemon share these JSON payloads by hand. The
// snapshot below is generated from the Go structs and verified by a Rust test
// that deserializes the same file into its own types. Updating one side
// without the other fails CI.
var updateIPCContract = flag.Bool("update-contract", false, "rewrite testdata/ipc-contract.json")

const ipcContractPath = "../../../testdata/ipc-contract.json"

func TestIPCContractSnapshots(t *testing.T) {
	fixtures := ipcContractFixtures()

	if *updateIPCContract {
		data, err := json.MarshalIndent(fixtures, "", "  ")
		if err != nil {
			t.Fatalf("marshal contract: %v", err)
		}
		if err := os.MkdirAll(filepath.Dir(ipcContractPath), 0o755); err != nil {
			t.Fatalf("create testdata: %v", err)
		}
		if err := os.WriteFile(ipcContractPath, append(data, '\n'), 0o644); err != nil {
			t.Fatalf("write contract: %v", err)
		}
		t.Logf("updated %s", ipcContractPath)
		return
	}

	raw, err := os.ReadFile(ipcContractPath)
	if err != nil {
		t.Fatalf("read %s: %v (run: go test ./structs -update-contract)", ipcContractPath, err)
	}
	var golden map[string]json.RawMessage
	if err := json.Unmarshal(raw, &golden); err != nil {
		t.Fatalf("parse %s: %v", ipcContractPath, err)
	}

	for name, fixture := range fixtures {
		wantJSON, err := json.Marshal(fixture)
		if err != nil {
			t.Errorf("%s: marshal: %v", name, err)
			continue
		}
		gotRaw, ok := golden[name]
		if !ok {
			t.Errorf("%s: missing fixture (run: go test ./structs -update-contract)", name)
			continue
		}
		var want, got any
		if err := json.Unmarshal(wantJSON, &want); err != nil {
			t.Errorf("%s: unmarshal current: %v", name, err)
			continue
		}
		if err := json.Unmarshal(gotRaw, &got); err != nil {
			t.Errorf("%s: unmarshal golden: %v", name, err)
			continue
		}
		if !reflect.DeepEqual(want, got) {
			t.Errorf("%s: Go structs no longer match %s (run: go test ./structs -update-contract)", name, ipcContractPath)
		}
	}

	for name := range golden {
		if _, ok := fixtures[name]; !ok {
			t.Errorf("%s: stale fixture not produced by structs (run: go test ./structs -update-contract)", name)
		}
	}
}

func ipcContractPtr[T any](v T) *T { return &v }

func ipcContractFixtures() map[string]any {
	profile := Profile{
		Id:          7,
		GroupId:     3,
		Name:        "Frankfurt VLESS",
		Protocol:    "vless",
		Uri:         "vless://uuid@vpn.example.test:443?security=reality#Frankfurt",
		Address:     "198.51.100.7",
		Host:        "vpn.example.test",
		TestResult:  42,
		TestedAt:    1712345678,
		LossPct:     1,
		JitterMs:    3,
		Port:        443,
		Managed:     true,
		ProviderKey: "provider-key-1",
	}
	group := Group{
		Id:                            3,
		SubscriptionUrl:               "https://provider.test/sub/token",
		Name:                          "Premium",
		LastId:                        7,
		SubLastUpdated:                1712345678,
		SubExpires:                    1720000000,
		SubUpload:                     1048576,
		SubDownload:                   52428800,
		SubTotal:                      107374182400,
		SubUpdateInterval:             21600,
		SubUpdateIntervalOverride:     43200,
		SubAutoUpdateDisabled:         true,
		ProfileOrder:                  []int{7},
		RoutingMode:                   "auto",
		ManualRoutingMode:             "custom",
		ActiveRoutingProfileId:        "rp-1",
		PendingActiveRoutingProfileId: "rp-2",
		SubscriptionFormat:            "json",
	}
	routingProfile := RoutingProfileInfo{
		Id:             "rp-1",
		Name:           "Provider Full Bypass",
		State:          "ready",
		LastError:      "none",
		Active:         true,
		RulesCount:     42,
		GlobalProxy:    true,
		DomainStrategy: "IPIfNonMatch",
		FakeDNSEnabled: true,
		RemoteDNS:      "DoH cloudflare-dns.com",
		DomesticDNS:    "DoU 8.8.8.8",
		LastUpdated:    1712345678,
		GeoUpdatedAt:   1712345678,
	}
	testConfig := TestConfig{
		Concurrency:    16,
		TimeoutSeconds: 5,
		SamplesPerTest: 3,
		TestEndpoint:   "https://cp.cloudflare.com/generate_204",
		AutoTestOnSub:  true,
	}
	xrayStatus := XrayStatusInfo{
		Version:         "v26.10.31",
		TargetVersion:   "v26.10.31",
		Status:          "ready",
		Progress:        100,
		BytesDownloaded: 1,
		TotalBytes:      2,
		Error:           "none",
	}
	proxyStatus := ProxyStatus{
		Connection:  "connected",
		Profile:     profile,
		ConnectedAt: 1712345678,
		Reason:      "none",
	}
	applicationState := ApplicationState{
		Groups: []GroupWithProfiles{{
			Group:           group,
			Profiles:        []Profile{profile},
			RoutingProfiles: []RoutingProfileInfo{routingProfile},
		}},
		ConnectionStatus: proxyStatus,
		TunStatus:        true,
		HwidInfo: HwidData{
			Enabled:   true,
			Hwid:      "1fb1e0141ab3e35a",
			UserAgent: "whoisthat/v0.4.3",
			Platform:  "linux",
			Kernel:    "6.12.0-arch1-1",
			Model:     "Arch Linux",
		},
		KillSwitch:      true,
		SplitTunnel:     "exclude",
		Autoconnect:     AutoconnectInfo{Enabled: true, Mode: "tun"},
		XrayStatus:      xrayStatus,
		CoreVersion:     "0.4.3",
		ProtocolVersion: 4,
		TunName:         "whoisthattun",
		TestConfig:      testConfig,
		ConfigStatus:    ConfigStatus{State: "ok", Error: "none"},
		DBStatus:        DBStatus{State: "ok", Error: "none", SnapshotAt: "20261102"},
	}
	routingConfig := RoutingConfig{
		DomainStrategy: "IPIfNonMatch",
		Rules: []RoutingRule{{
			Type:        "field",
			Domain:      "example.test",
			IP:          "192.0.2.0/24",
			Protocol:    "tls",
			Port:        "443",
			Network:     "tcp,udp",
			OutboundTag: "proxy",
			Enabled:     true,
		}},
		DNS: &HappDNSConfig{
			FakeDNSEnabled:    true,
			RemoteDNSType:     "DoH",
			RemoteDNSDomain:   "https://cloudflare-dns.com/dns-query",
			RemoteDNSIP:       "1.1.1.1",
			DomesticDNSType:   "DoU",
			DomesticDNSDomain: "dns.example.test",
			DomesticDNSIP:     "8.8.8.8",
			Hosts:             map[string]string{"vpn.example.test": "203.0.113.7"},
		},
	}

	return map[string]any{
		"application_state":       applicationState,
		"profile_updated":         ProfileUpdated{Profile: profile},
		"subscription_updated":    SubscriptionUpdated{GroupId: 3, Group: group, Profiles: []Profile{profile}, RoutingProfiles: []RoutingProfileInfo{routingProfile}, Auto: true},
		"routing_updated":         RoutingUpdated{GroupId: 3, Config: routingConfig, Source: "happ", Custom: &routingConfig},
		"status_changed":          proxyStatus,
		"connect_request":         ConnectData{Profile: ProfileID{Id: 7, GroupId: 3}},
		"update_group_request":    UpdateGroupData{Id: 3, Name: "Premium", SubscriptionUrl: "https://provider.test/sub/token", AutoUpdate: ipcContractPtr(true), UpdateInterval: ipcContractPtr(int64(21600))},
		"set_test_config_request": SetTestConfigData{Config: testConfig},
		"set_autoconnect_request": SetAutoconnectData{Enabled: true, GroupId: 3, ProfileId: 7, Mode: "tun"},
		"test_config_updated":     TestConfigUpdated{Config: testConfig},
	}
}
