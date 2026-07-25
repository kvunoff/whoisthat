package TCPServer

import (
	"encoding/json"
	"net"
	"strings"
	"sync"
	"testing"
	"time"
	"whoisthat-core/structs"
)

// TestWarnThrottle_AllowFirstSuppressesSecond verifies the throttle admits the
// first call for a given reason and suppresses an immediately-following second.
// A test that runs 50 hy2 profiles with no hysteria binary installed would
// otherwise produce 50 identical status-bar flashes within one second; the
// throttle collapses them into one.
func TestWarnThrottle_AllowFirstSuppressesSecond(t *testing.T) {
	th := &warnThrottle{lastAt: map[string]time.Time{}}
	if !th.allow("X") {
		t.Fatal("first allow must return true")
	}
	if th.allow("X") {
		t.Fatal("second allow for the same reason within the window must return false")
	}
}

// TestWarnThrottle_DistinctReasonsPassThrough ensures two different reason
// strings both get through even when issued back-to-back — this protects
// against the throttle collapsing genuinely-distinct diagnostics (e.g. one
// "binary not found" plus one "listener did not bind" inside the same window).
func TestWarnThrottle_DistinctReasonsPassThrough(t *testing.T) {
	th := &warnThrottle{lastAt: map[string]time.Time{}}
	if !th.allow("X") {
		t.Fatal("first distinct reason must pass")
	}
	if !th.allow("Y") {
		t.Fatal("second distinct reason must pass")
	}
}

// TestWarnThrottle_AllowsAfterWindow: once warnThrottleWindow elapses for a
// reason, the next call for it must pass again. (We can't easily mock the
// window so we manipulate lastAt directly to backdate the prior emission.)
func TestWarnThrottle_AllowsAfterWindow(t *testing.T) {
	th := &warnThrottle{lastAt: map[string]time.Time{}}
	if !th.allow("X") {
		t.Fatal("first must pass")
	}
	// Backdate the lastAt entry past the window.
	th.mu.Lock()
	th.lastAt["X"] = time.Now().Add(-(warnThrottleWindow + time.Second))
	th.mu.Unlock()
	if !th.allow("X") {
		t.Fatal("after the window elapses, the same reason must pass again")
	}
}

// TestWarnThrottle_ConcurrentSafe exercises the throttle under -race: many
// concurrent allow() calls for mixed reasons must not panic and must comply
// with the allow/allow-not invariant — at least the FIRST caller for each reason
// gets true.
func TestWarnThrottle_ConcurrentSafe(t *testing.T) {
	th := &warnThrottle{lastAt: map[string]time.Time{}}
	const goroutines = 50
	const reasons = 5
	var wg sync.WaitGroup
	passedPerReason := [reasons]int32{}
	var mu sync.Mutex
	for g := 0; g < goroutines; g++ {
		wg.Add(1)
		go func(grp int) {
			defer wg.Done()
			for r := 0; r < reasons; r++ {
				reason := "R" + itoa(r)
				if th.allow(reason) {
					mu.Lock()
					passedPerReason[r]++
					mu.Unlock()
				}
			}
		}(g)
	}
	wg.Wait()
	// Exactly one allow per reason must have passed (the first caller).
	for r := int32(0); r < int32(reasons); r++ {
		if passedPerReason[r] != 1 {
			t.Errorf("reason %d: passed %d times, want exactly 1", r, passedPerReason[r])
		}
	}
}

// TestCheckMissingBinaries_ReturnsExpectedNames verifies the pre-flight check
// returns entries named exactly "xray", "hysteria", "tun2socks", "parser" when
// those binaries are missing. We can't fake a missing binary on a CI/dev
// host where they may all be installed, so we only assert the structural
// invariant of the result (each entry has a non-empty Name+Hint). When
// everything is installed the result is empty and that's a valid pass too.
func TestCheckMissingBinaries_Structure(t *testing.T) {
	missing := CheckMissingBinaries()
	for _, mb := range missing {
		if mb.Name == "" {
			t.Error("MissingBinary.Name is empty")
		}
		if mb.Hint == "" {
			t.Errorf("MissingBinary(%s).Hint is empty", mb.Name)
		}
		// The hint should be actionable — include the install command or
		// at least mention the binary name.
		if !strings.Contains(mb.Hint, mb.Name) && !strings.Contains(mb.Hint, "parser") {
			t.Errorf("MissingBinary(%s).Hint does not reference the binary name: %q", mb.Name, mb.Hint)
		}
	}
}

// TestSendMissingBinaryWarnings_Unicast verifies the pre-flight warn path
// writes exactly one message per missing binary directly to the target
// client's outbound queue — and ONLY to that client. We construct two clients,
// call sendMissingBinaryWarnings on only one, and assert (a) the targeted
// client received N framed warn messages, (b) the other client received none.
func TestSendMissingBinaryWarnings_Unicast(t *testing.T) {
	s := newTestServer()

	// Targeted client.
	a1, b1 := net.Pipe()
	defer b1.Close()
	cc := newClientConn(a1)
	defer closeClient(cc)
	s.clients["target"] = cc

	// Innocent bystander — must NOT receive any of these warns.
	a2, b2 := net.Pipe()
	defer b2.Close()
	ccOther := newClientConn(a2)
	defer closeClient(ccOther)
	s.clients["other"] = ccOther

	missing := []MissingBinary{
		{Name: "hysteria", Hint: "hysteria binary not installed"},
		{Name: "tun2socks", Hint: "tun2socks binary not installed"},
	}
	sendMissingBinaryWarnings(cc, missing)

	// Drain the targeted client pipe and assert 2 warn frames arrived.
	for i := 0; i < len(missing); i++ {
		got, ok := readFramed(t, b1, time.Second)
		if !ok {
			t.Fatalf("target client did not receive warn #%d", i)
		}
		var env structs.TCPMessage
		if err := json.Unmarshal(got, &env); err != nil {
			t.Fatalf("warn #%d not JSON: %v", i, err)
		}
		if env.Msg != "warn" {
			t.Errorf("warn #%d env.Msg = %q, want \"warn\"", i, env.Msg)
		}
		var w structs.Warning
		if err := json.Unmarshal(env.Data, &w); err != nil {
			t.Fatalf("warn #%d data not Warning-shaped: %v", i, err)
		}
		if w.Key == "" || w.Content == "" {
			t.Errorf("warn #%d empty key/content: %+v", i, w)
		}
	}

	// Bystander must receive nothing within a short window.
	if got, ok := readFramed(t, b2, 100*time.Millisecond); ok {
		t.Errorf("innocent bystander received a frame: %q", got)
	}
}

// TestSendMissingBinaryWarnings_EmptyNoop: calling with an empty missing-list
// is a no-op. We assert no bytes arrive at the peer within a short window.
func TestSendMissingBinaryWarnings_EmptyNoop(t *testing.T) {
	s := newTestServer()
	a, b := net.Pipe()
	defer b.Close()
	cc := newClientConn(a)
	defer closeClient(cc)
	s.clients["c"] = cc

	sendMissingBinaryWarnings(cc, nil)
	if got, ok := readFramed(t, b, 100*time.Millisecond); ok {
		t.Errorf("expected no warn frames, got %q", got)
	}
}