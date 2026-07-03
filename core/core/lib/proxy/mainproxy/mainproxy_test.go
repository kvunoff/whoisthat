package mainproxy

import (
	"testing"
	"time"

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
	exitWatcherDone = nil
	p := newTestProxyManager()
	done := p.startExitWatcher()

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
	close(done)
	exitWatcherDone = nil
}

// TestWatchExit_ChannelClosed: closing Exited (as xray.go does on clean stop)
// makes the watcher return without emitting a status.
func TestWatchExit_ChannelClosed(t *testing.T) {
	exitWatcherDone = nil
	p := newTestProxyManager()
	done := p.startExitWatcher()

	close(p.core.ExitedCh())

	if got := drainStatus(t, p.StatusChanged, 1, 200*time.Millisecond); len(got) != 0 {
		t.Fatalf("expected no status on closed Exited, got %d", len(got))
	}

	close(done)
	exitWatcherDone = nil
}

// TestWatchExit_NonBlockingDrop: when the StatusChanged buffer (8) is full,
// the watcher must not block on the send — it drops and logs instead.
func TestWatchExit_NonBlockingDrop(t *testing.T) {
	exitWatcherDone = nil
	p := newTestProxyManager()
	// Pre-fill the buffered StatusChanged channel so the watcher's send can't land.
	for i := 0; i < cap(p.StatusChanged); i++ {
		p.StatusChanged <- structs.ProxyStatus{Connection: "connected"}
	}
	done := p.startExitWatcher()

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

	close(done)
	exitWatcherDone = nil
}

// TestRetireExitWatcher_IdempotentAndNilSafe: retiring twice and retiring when
// nothing is active must not panic on a double close or a nil channel.
func TestRetireExitWatcher_IdempotentAndNilSafe(t *testing.T) {
	exitWatcherDone = nil
	p := newTestProxyManager()

	p.retireExitWatcher() // nil → no-op, must not panic
	if exitWatcherDone != nil {
		t.Fatalf("exitWatcherDone = %v, want nil after nil-retire", exitWatcherDone)
	}

	p.startExitWatcher()
	if exitWatcherDone == nil {
		t.Fatal("exitWatcherDone = nil after startExitWatcher")
	}
	p.retireExitWatcher()
	if exitWatcherDone != nil {
		t.Fatalf("exitWatcherDone = %v, want nil after retire", exitWatcherDone)
	}
	p.retireExitWatcher() // second retire on the cleared global — must not panic
	if exitWatcherDone != nil {
		t.Fatalf("exitWatcherDone = %v, want nil after double retire", exitWatcherDone)
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
	exitWatcherDone = nil
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
// exitWatcherDone under the race detector: many concurrent start/retire cycles
// plus concurrent Exited sends. Run with -race.
func TestExitWatcher_ConcurrentStartRetire(t *testing.T) {
	exitWatcherDone = nil

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
