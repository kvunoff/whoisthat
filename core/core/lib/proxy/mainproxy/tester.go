package mainproxy

import (
	"sync"
	"sync/atomic"
	"time"

	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
)

type TestResult struct {
	Success     bool
	Profile     structs.Profile
	SampleCount int
	FailReason  string
}

type TestRequest struct {
	Profile     structs.Profile
	Method      string
	SampleCount int
	epoch       int64 // epoch at enqueue time; 0 means "untracked" (skip check)
}

// testEpoch is bumped by CancelTests to invalidate in-flight test goroutines.
var testEpoch atomic.Int64

// inFlightMu guards inFlight — the set of (groupId, profileId) currently being
// tested. Used by TestProfile/TestGroup to skip re-enqueueing a profile that is
// already queued or running under the current epoch. Cleared in sendTestResult
// when the goroutine finishes.
var inFlightMu sync.Mutex
var inFlight = map[[2]int]struct{}{}

// SetTestConfig replaces the live test configuration.
func (p *ProxyManager) SetTestConfig(cfg structs.TestConfig) {
	p.testMu.Lock()
	defer p.testMu.Unlock()
	if cfg.Concurrency < 1 {
		cfg.Concurrency = p.testConfig.Concurrency
	}
	if cfg.TimeoutSeconds < 1 {
		cfg.TimeoutSeconds = p.testConfig.TimeoutSeconds
	}
	if cfg.SamplesPerTest < 1 {
		cfg.SamplesPerTest = p.testConfig.SamplesPerTest
	}
	if cfg.TestEndpoint == "" {
		cfg.TestEndpoint = p.testConfig.TestEndpoint
	}
	endpoints := []string{cfg.TestEndpoint}
	for _, e := range p.testEndpoints {
		if e != cfg.TestEndpoint {
			endpoints = append(endpoints, e)
		}
	}
	p.testEndpoints = endpoints
	p.testConfig = cfg
}

func (p *ProxyManager) GetTestConfig() structs.TestConfig {
	p.testMu.RLock()
	defer p.testMu.RUnlock()
	return p.testConfig
}

// CancelTests invalidates all in-flight test goroutines by bumping the
// epoch. Already-spawned xray/hysteria subprocesses for in-flight samples
// finish their current sample (or time out) and then abort — no hard
// kills, no orphan risk. Pending requests still in the testChannel are
// skipped by listenForTests, which checks epoch on dequeue.
func (p *ProxyManager) CancelTests() {
	testEpoch.Add(1)
}

// SeedTestProgress initializes the per-group (tested, total) progress

// tracker. Called by commands.TestGroup when enqueuing a batch.
func (p *ProxyManager) SeedTestProgress(groupId, total int) {
	if groupId == 0 {
		return
	}
	p.progressMu.Lock()
	defer p.progressMu.Unlock()
	p.progressGroups[int32(groupId)] = &struct {
		tested int64
		total  int
	}{0, total}
}

// IncrementTestProgress atomically bumps the tested count for a group and
// returns (tested, total, ok). ok is false if no batch is active for the
// group (e.g. standalone `test-profile`). When tested reaches total the
// tracker entry is cleared and subsequent increments return ok=false.
func (p *ProxyManager) IncrementTestProgress(groupId int) (int, int, bool) {
	if groupId == 0 {
		return 0, 0, false
	}
	p.progressMu.Lock()
	entry, ok := p.progressGroups[int32(groupId)]
	p.progressMu.Unlock()
	if !ok {
		return 0, 0, false
	}
	n := int(atomic.AddInt64(&entry.tested, 1))
	if n >= entry.total {
		p.progressMu.Lock()
		delete(p.progressGroups, int32(groupId))
		p.progressMu.Unlock()
	}
	return n, entry.total, true
}

func (p *ProxyManager) TestProfile(profile structs.Profile, method string) {
	p.testMu.RLock()
	samples := p.testConfig.SamplesPerTest
	p.testMu.RUnlock()
	if !p.tryEnqueueInFlight(profile.GroupId, profile.Id) {
		return
	}
	p.testChannel <- TestRequest{
		Profile:     profile,
		Method:      method,
		SampleCount: samples,
		epoch:       testEpoch.Load(),
	}
}

// TestGroup enqueues tests for every profile in the given group. The
// caller is the commands/test.go TestGroup handler — it pre-loads the
// profiles from the DB and passes them in. Returns the number of tests
// actually enqueued (skipping duplicates against the active in-flight set)
// so the handler can include it in a TestProgress broadcast.
func (p *ProxyManager) TestGroup(profiles []structs.Profile, method string, samples int) int {
	if samples < 1 {
		p.testMu.RLock()
		samples = p.testConfig.SamplesPerTest
		p.testMu.RUnlock()
	}
	enqueued := 0
	epoch := testEpoch.Load()
	for _, prof := range profiles {
		if !p.tryEnqueueInFlight(prof.GroupId, prof.Id) {
			continue
		}
		p.testChannel <- TestRequest{
			Profile:     prof,
			Method:      method,
			SampleCount: samples,
			epoch:       epoch,
		}
		enqueued++
	}
	return enqueued
}

// tryEnqueueInFlight reserves a (groupId, profileId) slot in the in-flight
// set. Returns true if the slot was free (caller should enqueue), false if a
// test for this profile is already queued/running (caller should skip).
func (p *ProxyManager) tryEnqueueInFlight(groupId, profileId int) bool {
	key := [2]int{groupId, profileId}
	inFlightMu.Lock()
	defer inFlightMu.Unlock()
	if _, ok := inFlight[key]; ok {
		return false
	}
	inFlight[key] = struct{}{}
	return true
}

// releaseInFlight drops a (groupId, profileId) reservation. Idempotent.
func releaseInFlight(groupId, profileId int) {
	inFlightMu.Lock()
	delete(inFlight, [2]int{groupId, profileId})
	inFlightMu.Unlock()
}

func (p *ProxyManager) listenForTests(tests_chan chan TestRequest) {
	p.testMu.RLock()
	concurrency := p.testConfig.Concurrency
	p.testMu.RUnlock()
	if concurrency < 1 {
		concurrency = 16
	}
	sem := make(chan struct{}, concurrency)

	for req := range tests_chan {
		// Honor CancelTests without spawning subprocesses for already-
		// cancelled requests. The epoch bumped by CancelTests invalidates
		// every request still in the queue; we drop them here and free
		// their in-flight slot so a fresh batch can immediately re-test.
		epoch := testEpoch.Load()
		reqEpoch := req.epoch
		if reqEpoch > 0 && reqEpoch != epoch {
			releaseInFlight(req.Profile.GroupId, req.Profile.Id)
			continue
		}

		// Allow live concurrency changes between batches: if the user
		// changes the setting in the TUI while a batch is running, swap
		// to a new sized semaphore for subsequent spawns. We capture the
		// current sem by VALUE in a local so existing goroutines keep
		// draining the sem they acquired (a `defer <-sem` in a goroutine
		// that already took `sem <- struct{}{}` on the old sem must
		// release THAT same sem, not the freshly allocated one — fixing
		// a previous goroutine-leak race on reassignment).
		p.testMu.RLock()
		newConcurrency := p.testConfig.Concurrency
		p.testMu.RUnlock()
		if newConcurrency > 0 && cap(sem) != newConcurrency {
			sem = make(chan struct{}, newConcurrency)
		}
		localSem := sem
		localSem <- struct{}{}
		go func(req TestRequest, sem chan struct{}) {
			defer func() { <-sem }()
			defer releaseInFlight(req.Profile.GroupId, req.Profile.Id)
			ping := p.test(req)
			p.sendTestResult(req.Profile, ping)
		}(req, localSem)
	}
}

// pingResult captures the rich metadata produced by a multi-sample test.
// Latency is the median of successful samples (ms), or -1 if all failed.
// Jitter is max-min of successful samples (ms).
// LossPct is the percentage of failed samples (0..100).
// Success is true if at least one sample completed with a 2xx response.
// failReason is populated only on failure and carries a human-readable
// diagnostic (e.g. "xray.Start failed: ...", "all samples timed out").
// Empty failReason with success=false means "no specific cause recorded".

func (p *ProxyManager) sendTestResult(profile structs.Profile, ping pingResult) {
	profile.TestResult = ping.latencyMs
	profile.LossPct = ping.lossPct
	profile.JitterMs = ping.jitterMs
	if ping.success {
		profile.TestedAt = time.Now().Unix()
	}
	res := TestResult{
		Success:     ping.success,
		Profile:     profile,
		SampleCount: ping.sampleCount,
		FailReason:  ping.failReason,
	}
	// Non-blocking send: TestResultChannel is buffered (32). If the broadcast
	// goroutine (handleTestResults) is briefly slow — typically because of
	// a DB flush or a slow client — we drop the result on the floor rather
	// than keeping this test goroutine (and the semaphore slot it owns)
	// pinned. Dropping does mean a flaky UI may briefly miss a result but
	// that's preferable to a wedged test queue that blocks Cancel and
	// fresh enqueues. The DB will have already been updated when the next
	// non-dropped result for this profile arrives.
	select {
	case p.TestResultChannel <- res:
	default:
		logger.Warnf("test %s (gid=%d id=%d): TestResultChannel full — dropping result latency=%dms",
			profile.Name, profile.GroupId, profile.Id, ping.latencyMs)
	}
}

// profilePort returns the stored endpoint port for JSON-subscription profiles,
// falling back to the URI parser for share-link profiles.
