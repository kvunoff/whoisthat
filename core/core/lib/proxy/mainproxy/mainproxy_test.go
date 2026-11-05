package mainproxy

import (
	"encoding/json"
	"errors"
	"sync"
	"testing"
	"time"

	portpool "whoisthat-core/lib/PortPool"
	"whoisthat-core/lib/proxy/xray"
	"whoisthat-core/structs"
)

// newTestProxyManager builds a ProxyManager wired only with the channels the
// exit-watcher touches. core is a bare xray.XrayCore with a fresh Exited
// channel — Start (which execs the real xray binary) is never called. This
// isolates the watcher/retire/StatusChanged plumbing from the xray subprocess.
func newTestProxyManager() *ProxyManager {
	return &ProxyManager{
		status:        structs.ProxyStatus{Connection: "disconnected"},
		StatusChanged: make(chan structs.ProxyStatus, 8),
		StatsChanged:  make(chan structs.TrafficStats, 8),
		core:          &xray.XrayCore{Exited: make(chan error)},
	}
}

// drainStatus reads and returns up to n status updates, or fails if the
// timeout elapses before n arrive. A zero-length result after the timeout
// also means "nothing was sent", which some tests assert explicitly.
func drainStatus(t *testing.T, ch chan structs.ProxyStatus, n int, timeout time.Duration) []structs.ProxyStatus {
	t.Helper()
	var got []structs.ProxyStatus
	deadline := time.After(timeout)
	for len(got) < n {
		select {
		case s := <-ch:
			got = append(got, s)
		case <-deadline:
			return got
		}
	}
	return got
}

// TestWatchExit_HappyPath: an Exited signal flips status to disconnected and
// delivers exactly one StatusChanged update.
func TestWatchExit_HappyPath(t *testing.T) {
	p := newTestProxyManager()
	p.startExitWatcher()

	p.core.ExitedCh() <- nil

	select {
	case s := <-p.StatusChanged:
		if s.Connection != "disconnected" {
			t.Fatalf("status = %q, want %q", s.Connection, "disconnected")
		}
	case <-time.After(time.Second):
		t.Fatal("watcher did not emit a disconnected status")
	}

	// Retire to stop the watcher goroutine cleanly.
	p.retireExitWatcher()
}

// TestWatchExit_ChannelClosed: closing Exited (as xray.go does on clean stop)
// makes the watcher return without emitting a status.
func TestWatchExit_ChannelClosed(t *testing.T) {
	p := newTestProxyManager()
	p.startExitWatcher()

	close(p.core.ExitedCh())

	if got := drainStatus(t, p.StatusChanged, 1, 200*time.Millisecond); len(got) != 0 {
		t.Fatalf("expected no status on closed Exited, got %d", len(got))
	}

	p.retireExitWatcher()
}

// TestWatchExit_NonBlockingDrop: when the StatusChanged buffer (8) is full,
// the watcher must not block on the send — it drops and logs instead.
func TestWatchExit_NonBlockingDrop(t *testing.T) {
	p := newTestProxyManager()
	// Pre-fill the buffered StatusChanged channel so the watcher's send can't land.
	for i := 0; i < cap(p.StatusChanged); i++ {
		p.StatusChanged <- structs.ProxyStatus{Connection: "connected"}
	}
	p.startExitWatcher()

	doneSend := make(chan struct{})
	go func() {
		p.core.ExitedCh() <- nil // would block a non-buffered/non-selecting watcher
		close(doneSend)
	}()

	select {
	case <-doneSend:
		// good — watcher did not block the Exited sender
	case <-time.After(time.Second):
		t.Fatal("watcher blocked on Exited send when StatusChanged was full")
	}

	p.retireExitWatcher()
}

// TestRetireExitWatcher_IdempotentAndNilSafe: retiring twice and retiring when
// nothing is active must not panic on a double close or a nil channel.
func TestRetireExitWatcher_IdempotentAndNilSafe(t *testing.T) {
	p := newTestProxyManager()

	p.retireExitWatcher() // nil → no-op, must not panic
	if p.watchDone != nil {
		t.Fatalf("watchDone = %v, want nil after nil-retire", p.watchDone)
	}

	p.startExitWatcher()
	if p.watchDone == nil {
		t.Fatal("watchDone = nil after startExitWatcher")
	}
	p.retireExitWatcher()
	if p.watchDone != nil {
		t.Fatalf("watchDone = %v, want nil after retire", p.watchDone)
	}
	p.retireExitWatcher() // second retire on the cleared channel — must not panic
	if p.watchDone != nil {
		t.Fatalf("watchDone = %v, want nil after double retire", p.watchDone)
	}
}

// TestRetireExitWatcher_StopsWatching: after retirement, closing Exited must
// NOT produce a disconnected status — the watcher goroutine has already exited.
//
// Note: racing a retire against an *in-flight* Exited send is a legitimate
// nondeterministic case (either select arm may win), so this test instead
// ensures the watcher is fully stopped BEFORE we close Exited, then asserts no
// status is emitted. Closing Exited (rather than sending) is what xray.go does
// on a clean Stop.
func TestRetireExitWatcher_StopsWatching(t *testing.T) {
	p := newTestProxyManager()
	p.startExitWatcher()
	p.retireExitWatcher()

	// Give the watcher goroutine a moment to observe the closed done channel
	// and return, so the subsequent Exited close races against nobody.
	time.Sleep(50 * time.Millisecond)

	close(p.core.ExitedCh())

	if got := drainStatus(t, p.StatusChanged, 1, 200*time.Millisecond); len(got) != 0 {
		t.Fatalf("expected no status after retire, got %d", len(got))
	}
}

// TestExitWatcher_ConcurrentStartRetire exercises the package-global
// p.watchDone under the race detector: many concurrent start/retire cycles
// plus concurrent Exited sends. Run with -race.
func TestExitWatcher_ConcurrentStartRetire(t *testing.T) {

	const cycles = 200
	done := make(chan struct{})

	for i := 0; i < 4; i++ {
		go func() {
			defer func() { done <- struct{}{} }()
			for c := 0; c < cycles; c++ {
				p := newTestProxyManager()
				p.startExitWatcher()
				// Race an Exited send against retire; both are valid timing.
				go func(p *ProxyManager) { p.core.ExitedCh() <- nil }(p)
				p.retireExitWatcher()
				drainStatus(t, p.StatusChanged, 1, 50*time.Millisecond)
			}
		}()
	}

	for i := 0; i < 4; i++ {
		select {
		case <-done:
		case <-time.After(30 * time.Second):
			t.Fatal("concurrency test timed out")
		}
	}
}

// TestWatcherGenerationInvalidatesStale: a new connect cycle must make the
// previous watcher generation stale, and the current one current.
func TestWatcherGenerationInvalidatesStale(t *testing.T) {
	p := newTestProxyManager()
	p.startExitWatcher()
	gen := p.watchGen
	p.startExitWatcher() // supersedes the first watcher

	if !p.watcherIsStale(gen) {
		t.Fatal("previous watcher generation is still current")
	}
	if p.watcherIsStale(p.watchGen) {
		t.Fatal("current watcher generation reported stale")
	}

	p.retireExitWatcher()
	if !p.watcherIsStale(p.watchGen) {
		t.Fatal("retired watcher generation reported current")
	}
}

func TestReloadRouting_Disconnected(t *testing.T) {
	p := newTestProxyManager()
	if err := p.ReloadRouting("whoisthattun"); err != nil {
		t.Errorf("ReloadRouting when disconnected = %v, want nil", err)
	}
	if p.status.Connection != "disconnected" {
		t.Errorf("status = %v, want disconnected", p.status.Connection)
	}
}

func TestReloadRouting_Hysteria(t *testing.T) {
	p := newTestProxyManager()
	p.status = structs.ProxyStatus{
		Connection: "connected",
		Profile: structs.Profile{
			Protocol: "hysteria2",
			Name:     "hy2-node",
		},
	}
	if err := p.ReloadRouting("whoisthattun"); err != nil {
		t.Errorf("ReloadRouting for hysteria = %v, want nil", err)
	}
	if p.status.Connection != "connected" {
		t.Errorf("status = %v, want connected", p.status.Connection)
	}
}

func TestStop_ClearsStatusAndStats(t *testing.T) {
	p := newTestProxyManager()
	p.status = structs.ProxyStatus{Connection: "connected"}

	p.Stop()

	if got := p.GetStatus().Connection; got != "disconnected" {
		t.Fatalf("status = %q, want disconnected", got)
	}
	statuses := drainStatus(t, p.StatusChanged, 1, time.Second)
	if len(statuses) != 1 || statuses[0].Connection != "disconnected" {
		t.Fatalf("StatusChanged = %+v, want one disconnected update", statuses)
	}
	select {
	case st := <-p.StatsChanged:
		if st != (structs.TrafficStats{}) {
			t.Fatalf("StatsChanged = %+v, want zero stats", st)
		}
	case <-time.After(time.Second):
		t.Fatal("StatsChanged got no zero update")
	}
}

func TestSetTunMode_DisconnectedUpdatesConfig(t *testing.T) {
	p := newTestProxyManager()
	if err := p.SetTunMode("tun9", true); err != nil {
		t.Fatalf("SetTunMode() = %v", err)
	}
	if p.tunName != "tun9" || !p.tunEnabled {
		t.Fatalf("tunName=%q tunEnabled=%v", p.tunName, p.tunEnabled)
	}
}

func TestUpdateMovedProfile_Connected(t *testing.T) {
	p := newTestProxyManager()
	p.status = structs.ProxyStatus{
		Connection: "connected",
		Profile:    structs.Profile{Id: 1, GroupId: 2, Name: "old"},
	}

	p.UpdateMovedProfile(
		structs.ProfileID{Id: 1, GroupId: 2},
		structs.Profile{Id: 1, GroupId: 2, Name: "new"},
	)
	if got := p.GetStatus().Profile.Name; got != "new" {
		t.Fatalf("profile = %q, want new", got)
	}

	p.UpdateMovedProfile(
		structs.ProfileID{Id: 9, GroupId: 2},
		structs.Profile{Id: 9, GroupId: 2, Name: "wrong"},
	)
	if got := p.GetStatus().Profile.Name; got != "new" {
		t.Fatalf("unrelated profile update changed the status: %q", got)
	}
}

func TestCountOutbounds(t *testing.T) {
	cases := []struct {
		name string
		in   string
		want int
	}{
		{"valid", `{"outbounds":[{"tag":"a"},{"tag":"b"}]}`, 2},
		{"no outbounds", `{}`, 0},
		{"invalid json", `{`, -1},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := countOutbounds([]byte(tc.in)); got != tc.want {
				t.Fatalf("countOutbounds() = %d, want %d", got, tc.want)
			}
		})
	}
}

func TestBootstrapHosts(t *testing.T) {
	if got := bootstrapHosts("1.2.3.4", []string{"1.2.3.4"}); got != nil {
		t.Fatalf("IP address got hosts entry: %v", got)
	}
	if got := bootstrapHosts("", []string{"1.1.1.1"}); got != nil {
		t.Fatalf("empty address got hosts entry: %v", got)
	}
	if got := bootstrapHosts("example.com", nil); got != nil {
		t.Fatalf("no IPs got hosts entry: %v", got)
	}
	got := bootstrapHosts("example.com", []string{"1.1.1.1"})
	if len(got["example.com"]) != 1 || got["example.com"][0] != "1.1.1.1" {
		t.Fatalf("bootstrapHosts() = %v", got)
	}
}

type fakeCore struct {
	running   bool
	stopped   bool
	exited    chan error
	config    []byte
	closeOnce sync.Once
}

func (f *fakeCore) Start(cfg []byte, _ string) error {
	f.config = append([]byte(nil), cfg...)
	f.running = true
	return nil
}

func (f *fakeCore) Stop() {
	if !f.running {
		return
	}
	f.running = false
	f.stopped = true
	f.closeOnce.Do(func() { close(f.exited) })
}

func (f *fakeCore) IsRunning() bool { return f.running }

func (f *fakeCore) ExitedCh() chan error { return f.exited }

func TestConnect_WithFakeCore(t *testing.T) {
	// Keep exec.LookPath from finding a system xray; GetBinPath still
	// probes the fixed /usr/local/bin and /usr/bin paths.
	t.Setenv("PATH", t.TempDir())

	p := newTestProxyManager()
	p.portPool = portpool.CreatePortPool(30000, 30001)
	fake := &fakeCore{exited: make(chan error, 1)}
	old := newCore
	newCore = func() coreProc { return fake }
	t.Cleanup(func() { newCore = old })

	profile := structs.Profile{
		Name:     "test",
		Protocol: "vless",
		Address:  "127.0.0.1",
		Uri:      "vless://uuid@127.0.0.1:443?test=1#Test",
	}
	if err := p.Connect(profile, "", false); err != nil {
		t.Fatalf("Connect() = %v", err)
	}
	if !fake.running {
		t.Fatal("core was not started")
	}
	if got := p.GetStatus().Connection; got != "connected" {
		t.Fatalf("status = %q, want connected", got)
	}
	var cfg map[string]any
	if err := json.Unmarshal(fake.config, &cfg); err != nil {
		t.Fatalf("generated config is not JSON: %v", err)
	}
	if _, ok := cfg["outbounds"]; !ok {
		t.Fatal("generated config has no outbounds")
	}

	p.Stop()
	if fake.running || !fake.stopped {
		t.Fatalf("core not stopped: running=%v stopped=%v", fake.running, fake.stopped)
	}
	if got := p.GetStatus().Connection; got != "disconnected" {
		t.Fatalf("status after Stop = %q, want disconnected", got)
	}
}

func TestConnect_InvalidURIReleasesPort(t *testing.T) {
	p := newTestProxyManager()
	p.portPool = portpool.CreatePortPool(30000, 30001)

	if err := p.Connect(structs.Profile{Name: "bad", Uri: "not-a-uri"}, "", false); err == nil {
		t.Fatal("Connect() = nil, want an error")
	}
	if _, err := p.portPool.GetPort(); err != nil {
		t.Fatalf("api port leaked after a failed connect: %v", err)
	}
}

func TestConnect_PreStartHookRunsBeforeCoreStarts(t *testing.T) {
	t.Setenv("PATH", t.TempDir())

	p := newTestProxyManager()
	p.portPool = portpool.CreatePortPool(30000, 30001)
	fake := &fakeCore{exited: make(chan error, 1)}
	old := newCore
	newCore = func() coreProc { return fake }
	t.Cleanup(func() { newCore = old })

	var got []string
	p.SetPreStartHandler(func(ips []string) error {
		if fake.running {
			t.Error("pre-start hook ran after the core started")
		}
		got = append([]string(nil), ips...)
		return nil
	})

	profile := structs.Profile{
		Name:     "test",
		Protocol: "vless",
		Address:  "127.0.0.1",
		Uri:      "vless://uuid@127.0.0.1:443?test=1#Test",
	}
	if err := p.Connect(profile, "", false); err != nil {
		t.Fatalf("Connect() = %v", err)
	}
	if !fake.running {
		t.Fatal("core was not started")
	}
	if len(got) == 0 || got[0] != "127.0.0.1" {
		t.Fatalf("hook got IPs %v, want the resolved profile IP", got)
	}
}

func TestConnect_AbortsWhenPreStartHookFails(t *testing.T) {
	t.Setenv("PATH", t.TempDir())

	p := newTestProxyManager()
	p.portPool = portpool.CreatePortPool(30000, 30001)
	fake := &fakeCore{exited: make(chan error, 1)}
	old := newCore
	newCore = func() coreProc { return fake }
	t.Cleanup(func() { newCore = old })

	p.SetPreStartHandler(func([]string) error { return ErrKillSwitchNotArmed })

	profile := structs.Profile{
		Name:     "test",
		Protocol: "vless",
		Address:  "127.0.0.1",
		Uri:      "vless://uuid@127.0.0.1:443?test=1#Test",
	}
	err := p.Connect(profile, "", false)
	if !errors.Is(err, ErrKillSwitchNotArmed) {
		t.Fatalf("Connect() = %v, want ErrKillSwitchNotArmed", err)
	}
	if fake.running {
		t.Fatal("core started despite the failed pre-start hook")
	}
	if _, err := p.portPool.GetPort(); err != nil {
		t.Fatalf("api port leaked after a failed hook: %v", err)
	}
}

func TestResolveProfileIPsKeepsIPLiterals(t *testing.T) {
	if got := resolveProfileIPs("192.0.2.7", []string{"1.1.1.1"}); len(got) != 1 || got[0] != "192.0.2.7" {
		t.Fatalf("resolveProfileIPs() = %v, want the literal IP", got)
	}
}
