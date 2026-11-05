package cmd

import (
	"encoding/json"
	"errors"
	"testing"

	appconfig "whoisthat-core/lib/AppConfig"
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
