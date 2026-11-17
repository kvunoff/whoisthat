package cmd

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"

	appconfig "whoisthat-core/lib/AppConfig"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
)

type fakeKSProxy struct {
	status structs.ProxyStatus
	ips    []string
}

func (f fakeKSProxy) GetStatus() structs.ProxyStatus { return f.status }
func (f fakeKSProxy) GetProxyIPs() []string          { return f.ips }

type ksStubs struct {
	setupCalls  int
	removeCalls int
	ips         []string
}

func stubKillSwitch(t *testing.T, setupErr, removeErr error, armed bool) *ksStubs {
	t.Helper()
	oldSetup, oldRemove, oldProbe := setupKillSwitchBlock, removeKillSwitchBlock, probeKillSwitchArmed
	st := &ksStubs{}
	setupKillSwitchBlock = func(ips []string, tunName string, dnsServers []string) error {
		st.setupCalls++
		st.ips = append([]string(nil), ips...)
		return setupErr
	}
	removeKillSwitchBlock = func() error {
		st.removeCalls++
		return removeErr
	}
	probeKillSwitchArmed = func() (bool, error) { return armed, nil }
	t.Cleanup(func() {
		setupKillSwitchBlock, removeKillSwitchBlock, probeKillSwitchArmed = oldSetup, oldRemove, oldProbe
	})
	return st
}

func resetKillSwitchConfig(t *testing.T) {
	t.Helper()
	appconfig.LoadConfig()
	appconfig.SetKillSwitch(false)
	appconfig.SetSplitTunnelMode("off")
	t.Cleanup(func() {
		appconfig.SetKillSwitch(false)
		appconfig.SetSplitTunnelMode("off")
	})
}

func newKillSwitchCmd() (*Cmd, chan []byte) {
	ch := make(chan []byte, 32)
	return &Cmd{Broadcast: func(data []byte) { ch <- data }}, ch
}

func lastKillSwitchEvent(t *testing.T, ch chan []byte) structs.SetKillSwitchData {
	t.Helper()
	for {
		select {
		case raw := <-ch:
			var msg struct {
				Msg  string          `json:"msg"`
				Data json.RawMessage `json:"data"`
			}
			if err := json.Unmarshal(raw, &msg); err != nil {
				t.Fatalf("decode notification: %v", err)
			}
			if msg.Msg != "kill-switch-updated" {
				continue
			}
			var event structs.SetKillSwitchData
			if err := json.Unmarshal(msg.Data, &event); err != nil {
				t.Fatalf("decode kill-switch event: %v", err)
			}
			return event
		default:
			t.Fatal("no kill-switch-updated notification")
		}
	}
}

func TestSetKillSwitchIdleEnableArmsEmptyAllowlist(t *testing.T) {
	resetKillSwitchConfig(t)
	st := stubKillSwitch(t, nil, nil, true)
	cmd, ch := newKillSwitchCmd()

	cmd.SetKillSwitch(structs.SetKillSwitchData{Enabled: true}, nil)

	if st.setupCalls != 1 {
		t.Fatalf("setup calls = %d, want 1", st.setupCalls)
	}
	if len(st.ips) != 0 {
		t.Fatalf("armed with IPs %v while disconnected, want an empty allowlist", st.ips)
	}
	if !appconfig.GetConfig().KillSwitchEnabled {
		t.Fatal("enabled intent was not persisted")
	}
	if event := lastKillSwitchEvent(t, ch); !event.Enabled || !event.Armed {
		t.Fatalf("event = %+v, want enabled and armed", event)
	}
}

func TestSetKillSwitchConnectedWithoutIPsRefusesToArm(t *testing.T) {
	resetKillSwitchConfig(t)
	st := stubKillSwitch(t, nil, nil, false)
	cmd, ch := newKillSwitchCmd()
	pm := fakeKSProxy{status: structs.ProxyStatus{Connection: "connected"}}

	cmd.SetKillSwitch(structs.SetKillSwitchData{Enabled: true}, pm)

	if st.setupCalls != 0 {
		t.Fatalf("setup calls = %d, want 0 (must not drop the live tunnel)", st.setupCalls)
	}
	if !appconfig.GetConfig().KillSwitchEnabled {
		t.Fatal("intent must survive a refusal to arm")
	}
	if event := lastKillSwitchEvent(t, ch); !event.Enabled || event.Armed {
		t.Fatalf("event = %+v, want enabled=true armed=false", event)
	}
}

func TestSetKillSwitchSetupFailureKeepsIntent(t *testing.T) {
	resetKillSwitchConfig(t)
	stubKillSwitch(t, errors.New("nft exploded"), nil, false)
	cmd, ch := newKillSwitchCmd()

	cmd.SetKillSwitch(structs.SetKillSwitchData{Enabled: true}, nil)

	if !appconfig.GetConfig().KillSwitchEnabled {
		t.Fatal("intent must be kept when arming fails")
	}
	if event := lastKillSwitchEvent(t, ch); !event.Enabled || event.Armed {
		t.Fatalf("event = %+v, want enabled=true armed=false", event)
	}
}

func TestSetKillSwitchDisable(t *testing.T) {
	resetKillSwitchConfig(t)
	appconfig.SetKillSwitch(true)
	st := stubKillSwitch(t, nil, nil, false)
	cmd, ch := newKillSwitchCmd()

	cmd.SetKillSwitch(structs.SetKillSwitchData{Enabled: false}, nil)

	if st.removeCalls != 1 {
		t.Fatalf("remove calls = %d, want 1", st.removeCalls)
	}
	if appconfig.GetConfig().KillSwitchEnabled {
		t.Fatal("disable was not persisted")
	}
	if event := lastKillSwitchEvent(t, ch); event.Enabled || event.Armed {
		t.Fatalf("event = %+v, want disabled and not armed", event)
	}
}

func TestSetKillSwitchDisableFailureKeepsSwitchOn(t *testing.T) {
	resetKillSwitchConfig(t)
	appconfig.SetKillSwitch(true)
	stubKillSwitch(t, nil, errors.New("still installed"), true)
	cmd, ch := newKillSwitchCmd()

	cmd.SetKillSwitch(structs.SetKillSwitchData{Enabled: false}, nil)

	if !appconfig.GetConfig().KillSwitchEnabled {
		t.Fatal("disable failure must keep the intent on, not report a false OFF")
	}
	if event := lastKillSwitchEvent(t, ch); !event.Enabled || !event.Armed {
		t.Fatalf("event = %+v, want enabled=true armed=true", event)
	}
}

func TestSetKillSwitchForceDisable(t *testing.T) {
	resetKillSwitchConfig(t)
	appconfig.SetKillSwitch(true)
	stubKillSwitch(t, nil, errors.New("cannot verify"), true)
	cmd, ch := newKillSwitchCmd()

	cmd.SetKillSwitch(structs.SetKillSwitchData{Enabled: false, Force: true}, nil)

	if appconfig.GetConfig().KillSwitchEnabled {
		t.Fatal("force-disable did not persist the off state")
	}
	if event := lastKillSwitchEvent(t, ch); event.Enabled || event.Armed {
		t.Fatalf("event = %+v, want disabled and not armed", event)
	}
}

func TestSetKillSwitchRejectsIncludeSplitMode(t *testing.T) {
	resetKillSwitchConfig(t)
	appconfig.SetSplitTunnelMode("include")
	st := stubKillSwitch(t, nil, nil, false)
	cmd, ch := newKillSwitchCmd()

	cmd.SetKillSwitch(structs.SetKillSwitchData{Enabled: true}, nil)

	if st.setupCalls != 0 {
		t.Fatalf("setup calls = %d, want 0 for include mode", st.setupCalls)
	}
	if appconfig.GetConfig().KillSwitchEnabled {
		t.Fatal("kill-switch must not be enabled alongside include mode")
	}
	if event := lastKillSwitchEvent(t, ch); event.Enabled {
		t.Fatalf("event = %+v, want the unchanged disabled state", event)
	}
}

func TestSetKillSwitchTestModeDoesNotTouchFirewall(t *testing.T) {
	resetKillSwitchConfig(t)
	t.Setenv("WHOISTHAT_TEST_MODE", "1")
	st := stubKillSwitch(t, nil, nil, true)
	cmd, ch := newKillSwitchCmd()

	cmd.SetKillSwitch(structs.SetKillSwitchData{Enabled: true}, nil)

	if st.setupCalls != 0 || st.removeCalls != 0 {
		t.Fatalf("test mode touched the firewall (setup=%d remove=%d)", st.setupCalls, st.removeCalls)
	}
	if event := lastKillSwitchEvent(t, ch); !event.Enabled || event.Armed {
		t.Fatalf("event = %+v, want enabled=true armed=false", event)
	}
}

// A rule added while armed only reached the firewall on the next arm before;
// the excluded host stayed dropped in the meantime.
func TestRefreshKillSwitchReappliesWhenArmed(t *testing.T) {
	resetKillSwitchConfig(t)
	st := stubKillSwitch(t, nil, nil, true)
	appconfig.SetKillSwitch(true)
	cmd, _ := newKillSwitchCmd()

	cmd.refreshKillSwitch(fakeKSProxy{ips: []string{"203.0.113.9"}})
	if st.setupCalls != 1 {
		t.Fatalf("setup calls = %d, want 1", st.setupCalls)
	}
	if len(st.ips) != 1 || st.ips[0] != "203.0.113.9" {
		t.Fatalf("ips = %v", st.ips)
	}
}

func TestRefreshKillSwitchSkipsWhenDisarmedOrEmpty(t *testing.T) {
	resetKillSwitchConfig(t)
	st := stubKillSwitch(t, nil, nil, false)
	appconfig.SetKillSwitch(true)
	cmd, _ := newKillSwitchCmd()

	cmd.refreshKillSwitch(fakeKSProxy{ips: []string{"203.0.113.9"}})
	if st.setupCalls != 0 {
		t.Fatalf("disarmed: setup calls = %d, want 0", st.setupCalls)
	}

	st2 := stubKillSwitch(t, nil, nil, true)
	cmd.refreshKillSwitch(fakeKSProxy{})
	if st2.setupCalls != 0 {
		t.Fatalf("empty ips: setup calls = %d, want 0", st2.setupCalls)
	}
}

// connect and disarm hold ConnectionMutex while routing reloads; a refresh that
// blocked on it deadlocked the whole core.
func TestRefreshKillSwitchDoesNotBlockOnHeldMutex(t *testing.T) {
	resetKillSwitchConfig(t)
	stubKillSwitch(t, nil, nil, true)
	appconfig.SetKillSwitch(true)
	cmd, _ := newKillSwitchCmd()

	ConnectionMutex.Lock()
	done := make(chan struct{})
	go func() { cmd.refreshKillSwitch(fakeKSProxy{ips: []string{"203.0.113.9"}}); close(done) }()
	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal("refresh blocked on a held ConnectionMutex")
	}
	ConnectionMutex.Unlock()
}

// A subscription refresh re-arms with the current server set, so the ruleset
// (and the exceptions resolved inside it) is rebuilt rather than left stale.
func TestRefreshKillSwitchRearmsWithChangedAddresses(t *testing.T) {
	resetKillSwitchConfig(t)
	st := stubKillSwitch(t, nil, nil, true)
	appconfig.SetKillSwitch(true)
	cmd, _ := newKillSwitchCmd()

	cmd.refreshKillSwitch(fakeKSProxy{ips: []string{"203.0.113.9"}})
	cmd.refreshKillSwitch(fakeKSProxy{ips: []string{"203.0.113.77"}})
	if st.setupCalls != 2 || len(st.ips) != 1 || st.ips[0] != "203.0.113.77" {
		t.Fatalf("setup calls = %d, ips = %v; want a second arm with the new address", st.setupCalls, st.ips)
	}
}

// A connect that never returns must not keep the core's only connection lock.
func TestConnectTimeoutReleasesConnectionMutex(t *testing.T) {
	oldBody, oldTimeout := connectBody, connectTimeout
	release := make(chan struct{})
	connectBody = func(*Cmd, structs.ConnectData, *proxy.ProxyManager, *tunmode.TunModeManager) { <-release }
	connectTimeout = 50 * time.Millisecond
	t.Cleanup(func() { close(release); connectBody, connectTimeout = oldBody, oldTimeout })
	cmd, ch := newKillSwitchCmd()

	done := make(chan struct{})
	go func() { cmd.Connect(structs.ConnectData{}, nil, nil); close(done) }()
	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal("Connect did not return after its deadline")
	}
	if !ConnectionMutex.TryLock() {
		t.Fatal("ConnectionMutex still held after a timed-out connect")
	}
	ConnectionMutex.Unlock()

	raw := <-ch
	if !strings.Contains(string(raw), "connect-failed") || !strings.Contains(string(raw), "timed out") {
		t.Fatalf("no clear timeout warning: %s", raw)
	}
}
