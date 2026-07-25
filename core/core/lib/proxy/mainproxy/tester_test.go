package mainproxy

import (
	"sync"
	"testing"
	"time"
	"whoisthat-core/structs"
)

// resetInFlightForTest clears the package-global inFlight map between tests so
// they don't observe each other's reservations. Tests are run sequentially by
// the Go test runner so no extra locking is needed beyond the package mutex.
func resetInFlightForTest() {
	inFlightMu.Lock()
	inFlight = map[[2]int]struct{}{}
	inFlightMu.Unlock()
}

// TestTryEnqueueInFlight_FirstWins verifies a (groupId, profileId) reservation
// is granted to the first caller and rejected for a second concurrent caller.
func TestTryEnqueueInFlight_FirstWins(t *testing.T) {
	resetInFlightForTest()
	p := &ProxyManager{}
	if !p.tryEnqueueInFlight(1, 7) {
		t.Fatal("first enqueue should succeed")
	}
	if p.tryEnqueueInFlight(1, 7) {
		t.Fatal("second enqueue for the same (gid,pid) should be rejected")
	}
	// Different profile in the same group is unaffected.
	if !p.tryEnqueueInFlight(1, 8) {
		t.Fatal("enqueue of a different (gid,pid) should succeed")
	}
}

// TestReleaseInFlight_AllowsReenqueue verifies that once a test goroutine
// releases its slot via releaseInFlight, the same profile can be re-enqueued.
// This is the path that lets `t` pressed twice in close succession run the
// second batch once the first has finished (or been cancelled).
func TestReleaseInFlight_AllowsReenqueue(t *testing.T) {
	resetInFlightForTest()
	p := &ProxyManager{}
	if !p.tryEnqueueInFlight(2, 5) {
		t.Fatal("initial enqueue should succeed")
	}
	releaseInFlight(2, 5)
	if !p.tryEnqueueInFlight(2, 5) {
		t.Fatal("re-enqueue after release should succeed")
	}
}

// TestReleaseInFlight_Idempotent ensures double-release does not panic and
// does not corrupt the reservation map (the underlying delete on an absent
// key is a no-op).
func TestReleaseInFlight_Idempotent(t *testing.T) {
	resetInFlightForTest()
	releaseInFlight(99, 99) // never reserved — must not panic
	releaseInFlight(99, 99)
	p := &ProxyManager{}
	if !p.tryEnqueueInFlight(99, 99) {
		t.Fatal("reservation should be grantable after noop releases")
	}
}

// TestSendTestResult_NonBlocking_DropsOnFull verifies the send path cannot
// block the test goroutine. We pre-fill the channel to capacity so the next
// send must hit the `default` arm of the select. The test passes if and only
// if sendTestResult returns (rather than blocking forever).
func TestSendTestResult_NonBlocking_DropsOnFull(t *testing.T) {
	p := &ProxyManager{
		TestResultChannel: make(chan TestResult, 2),
	}
	// Pre-fill the buffered channel.
	p.TestResultChannel <- TestResult{}
	p.TestResultChannel <- TestResult{}

	done := make(chan struct{})
	go func() {
		p.sendTestResult(structs.Profile{GroupId: 1, Id: 1, Name: "x"}, pingResult{latencyMs: 42})
		close(done)
	}()
	select {
	case <-done:
		// good — non-blocking drop path fired
	case <-time.After(time.Second):
		t.Fatal("sendTestResult blocked when TestResultChannel was full")
	}
}

// TestCancelTests_BumpsEpoch verifies that CancelTests increments the
// package-global testEpoch so in-flight goroutines and listenForTests
// skip their (already cancelled) work.
func TestCancelTests_BumpsEpoch(t *testing.T) {
	before := testEpoch.Load()
	p := &ProxyManager{}
	p.CancelTests()
	after := testEpoch.Load()
	if after != before+1 {
		t.Fatalf("testEpoch: before=%d after=%d, want +%d", before, after, 1)
	}
}

// TestTestEpochDropsCancelledRequests is a behavioural test: enqueue a request
// with a stale epoch, drive listenForTests briefly, and assert listenForTests
// released the in-flight reservation without invoking test() on it.
//
// We do NOT enqueue a "fresh epoch" request alongside it: that would require
// populating portPool (etc.) before test() / testViaXray could run, which is
// out of scope for this dispatcher-only assertion. The stale-path exhaustively
// demonstrates the bug we fixed — that listenForTests used to spawn xray for
// every queued request regardless of whether CancelTests had fired in the
// meantime.
func TestTestEpochDropsCancelledRequests(t *testing.T) {
	resetInFlightForTest()
	p := &ProxyManager{
		TestResultChannel: make(chan TestResult, 8),
	}

	// Reserve the (gid=10, pid=1) slot manually so the test request is
	// associated with an in-flight entry that listenForTests is responsible
	// for releasing when it drops the request.
	if !p.tryEnqueueInFlight(10, 1) {
		t.Fatal("pre-reserve should succeed on empty inFlight set")
	}

	// Bump epoch now — anything enqueued under a pre-bump epoch must drop.
	staleEpoch := testEpoch.Load()
	testEpoch.Add(1)

	ch := make(chan TestRequest, 4)
	go p.listenForTests(ch)
	defer close(ch)

	ch <- TestRequest{
		Profile:     structs.Profile{GroupId: 10, Id: 1, Name: "stale"},
		SampleCount: 1,
		epoch:       staleEpoch,
	}

	// Allow listenForTests time to dequeue and observe the mismatch.
	deadline := time.Now().Add(500 * time.Millisecond)
	for time.Now().Before(deadline) {
		inFlightMu.Lock()
		_, stillHeld := inFlight[[2]int{10, 1}]
		inFlightMu.Unlock()
		if !stillHeld {
			return // pass — released as expected
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Errorf("stale request (gid=10 pid=1) reservation was not released by listenForTests within 500ms")
}

// TestWaitForListener_UnreachableReturnsFastly confirms the polling loop
// bounds its runtime by the deadline and doesn't hang on a never-bound port.
// We use an arbitrarily high port that nobody is listening on (1) to make
// DialTimeout fail immediately and (2) to allow the loop to spin until the
// ~120ms deadline we set below.
func TestWaitForListener_UnreachableReturnsFastly(t *testing.T) {
	start := time.Now()
	if waitForListener("tcp", "127.0.0.1:1", 120*time.Millisecond) {
		t.Fatal("waitForListener returned true for a port that is closed on most systems")
	}
	elapsed := time.Since(start)
	// Should be roughly the deadline; allow generous slack for scheduling
	// jitter. The important property is that it actually returned.
	if elapsed > 500*time.Millisecond {
		t.Errorf("waitForListener ran %v, want ~120ms", elapsed)
	}
	if elapsed < 120*time.Millisecond {
		t.Errorf("waitForListener returned early after %v, want the deadline honored", elapsed)
	}
}

// TestExtractPort_Hysteria verifies hysteria2's port extraction falls through
// to the default (443) when the URI omits an explicit port — relevant to the
// hysteria2 test path, which uses extractPort for the log line about the
// pending UDP handshake.
func TestExtractPort_Hysteria(t *testing.T) {
	got := extractPort("hysteria2://pw@example.com?sni=x", "hysteria2")
	if got != "443" {
		t.Errorf("extractPort hysteria2 default = %q, want 443", got)
	}
	got = extractPort("hysteria2://pw@example.com:8443?sni=x", "hysteria2")
	if got != "8443" {
		t.Errorf("extractPort hysteria2 with explicit port = %q, want 8443", got)
	}
}

// TestInFlightThreadSafety: spawn many concurrent tryEnqueue+release pairs
// and confirm the map remains consistent (no panics, no orphans).
func TestInFlightThreadSafety(t *testing.T) {
	resetInFlightForTest()
	p := &ProxyManager{}
	var wg sync.WaitGroup
	const goroutines = 50
	const ops = 200
	for g := 0; g < goroutines; g++ {
		wg.Add(1)
		go func(grp int) {
			defer wg.Done()
			for i := 0; i < ops; i++ {
				if p.tryEnqueueInFlight(grp, i%8) {
					releaseInFlight(grp, i%8)
				}
			}
		}(g)
	}
	wg.Wait()
	inFlightMu.Lock()
	leaked := len(inFlight)
	inFlightMu.Unlock()
	if leaked != 0 {
		t.Errorf("inFlight had %d orphan reservations after concurrent loop; want 0", leaked)
	}
}