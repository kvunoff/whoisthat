package mainproxy

import (
	"fmt"
	"net"
	"net/http"
	"regexp"
	"sort"
	"strconv"
	"sync"
	"sync/atomic"
	"time"
	"whoisthat-core/lib"
	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/proxy/hysteria"
	"whoisthat-core/lib/proxy/xray"
	"whoisthat-core/structs"

	goproxy "golang.org/x/net/proxy"
)

var rePort = regexp.MustCompile(`@[\w.\-\[\]:]+:(\d+)`)

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
type pingResult struct {
	latencyMs   int
	jitterMs    int
	lossPct     int
	success     bool
	sampleCount int
	failReason  string
}

func (p *ProxyManager) test(req TestRequest) pingResult {
	samples := req.SampleCount
	if samples < 1 {
		p.testMu.RLock()
		samples = p.testConfig.SamplesPerTest
		p.testMu.RUnlock()
	}

	// Epoch guard at the very top — protects against CancelTests firing
	// between enqueue (in the caller) and spawn (here). Without this we'd
	// launch subprocesses for tests the user already cancelled.
	epoch := testEpoch.Load()
	if req.epoch > 0 && req.epoch != epoch {
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100, failReason: "cancelled"}
	}

	// Fast pre-filter: a raw TCP reachability dial. For non-hysteria
	// protocols this is a meaningful quick reject — if the server's port
	// isn't even open, don't waste 5s spawning xray. For hysteria2 the
	// port is UDP, so the TCP dial is unrelated to reachability — skip.
	if req.Method == "tcp" {
		ms := p.testTcpOnly(req.Profile)
		res := pingResult{
			latencyMs:   ms,
			success:     ms > 0,
			sampleCount: 1,
		}
		if ms <= 0 {
			res.failReason = "tcp dial failed (see core.log)"
		}
		return res
	}

	if !isHysteriaProtocol(req.Profile.Protocol) {
		// Soft quick-reject: dial the server port directly. We use this
		// as an informational signal rather than a hard gate — many real
		// servers (especially CDN-fronted ones) drop direct TCP to the
		// upstream port while still serving traffic through the proxy
		// correctly. So we log the failure and proceed to the real
		// xray-based test; if xray itself can't reach the server the
		// samples will legitimately fail and report 100% loss then.
		if !p.serverReachable(req.Profile) {
			logger.Warnf("test %s (gid=%d id=%d): serverReachable failed for %s://%s — proceeding anyway",
				req.Profile.Name, req.Profile.GroupId, req.Profile.Id,
				req.Profile.Protocol, req.Profile.Address)
		}
		return p.testViaXray(req.Profile, samples)
	}
	return p.testViaHysteria(req.Profile, samples)
}

// testTcpOnly is the legacy "raw TCP dial to server:port" measurement.
// It does NOT validate protocol/auth/transport — only that something is
// listening on the target port. Returned as a final result when the user
// has explicitly chosen the "tcp" test method.
func (p *ProxyManager) testTcpOnly(profile structs.Profile) int {
	port := extractPort(profile.Uri, profile.Protocol)
	addr := profile.Address
	if addr == "" {
		addr = profile.Host
	}
	if addr == "" {
		return -1
	}
	target := net.JoinHostPort(addr, port)
	p.testMu.RLock()
	timeoutSec := p.testConfig.TimeoutSeconds
	p.testMu.RUnlock()
	start := time.Now()
	conn, err := net.DialTimeout("tcp", target, time.Duration(timeoutSec)*time.Second)
	if err != nil {
		logger.Warnf("test %s (gid=%d id=%d): tcp dial %s failed: %v",
			profile.Name, profile.GroupId, profile.Id, target, err)
		return -1
	}
	conn.Close()
	return int(time.Since(start).Milliseconds())
}

// serverReachable is a fast pre-filter dial. Returns true if the
// profile's server:port accepts a TCP connection within 4s. Latency is
// not recorded — this is a yes/no gate. Callers should treat a `false`
// result as informational, not authoritative (CDN/proxy edge cases can
// defeat raw TCP probes).
func (p *ProxyManager) serverReachable(profile structs.Profile) bool {
	port := extractPort(profile.Uri, profile.Protocol)
	addr := profile.Address
	if addr == "" {
		addr = profile.Host
	}
	if addr == "" {
		return true // can't tell; let the real test decide
	}
	target := net.JoinHostPort(addr, port)
	conn, err := net.DialTimeout("tcp", target, 4*time.Second)
	if err != nil {
		return false
	}
	conn.Close()
	return true
}

// testViaXray spawns the xray subprocess with the profile's parsed
// config, polls its SOCKS5 listener for readiness, then issues N HTTP
// GETs via the SOCKS5 proxy and computes median + jitter + loss.
func (p *ProxyManager) testViaXray(profile structs.Profile, samples int) pingResult {
	port, err := p.portPool.GetPort()
	if err != nil {
		logger.Warnf("test %s (gid=%d id=%d): port pool exhausted: %v",
			profile.Name, profile.GroupId, profile.Id, err)
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100, failReason: "no free test port available"}
	}
	defer p.portPool.ReleasePort(port)

	parsed, err := lib.ParseUri(profile.Uri, port, -1)
	if err != nil {
		logger.Warnf("test %s (gid=%d id=%d): ParseUri failed for %s://%s: %v",
			profile.Name, profile.GroupId, profile.Id,
			profile.Protocol, profile.Address, err)
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100, failReason: fmt.Sprintf("ParseUri failed: %v", err)}
	}

	xray_core := xray.XrayCore{Exited: make(chan error)}
	if err := xray_core.Start(parsed); err != nil {
		logger.Warnf("test %s (gid=%d id=%d): xray.Start failed: %v",
			profile.Name, profile.GroupId, profile.Id, err)
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100, failReason: fmt.Sprintf("xray.Start failed: %v", err)}
	}
	defer xray_core.Stop()

	if !waitForListener("tcp", fmt.Sprintf("127.0.0.1:%d", port), 4*time.Second) {
		logger.Warnf("test %s (gid=%d id=%d): xray SOCKS listener did not bind on port %d within 4s",
			profile.Name, profile.GroupId, profile.Id, port)
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100, failReason: fmt.Sprintf("xray SOCKS listener did not bind on port %d within 4s", port)}
	}
	return p.runSamples(profile, port, samples)
}

// testViaHysteria spawns the hysteria2 client with the profile's parsed
// YAML and uses its SOCKS5 listener for the HTTP samples. Mirrors
// testViaXray but calls ParseUriHysteria + HysteriaCore.
func (p *ProxyManager) testViaHysteria(profile structs.Profile, samples int) pingResult {
	port, err := p.portPool.GetPort()
	if err != nil {
		logger.Warnf("test %s (gid=%d id=%d): port pool exhausted: %v",
			profile.Name, profile.GroupId, profile.Id, err)
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100, failReason: "no free test port available"}
	}
	defer p.portPool.ReleasePort(port)

	yaml_config, err := lib.ParseUriHysteria(profile.Uri, port, -1)
	if err != nil {
		logger.Warnf("test %s (gid=%d id=%d): ParseUriHysteria failed for %s://%s: %v",
			profile.Name, profile.GroupId, profile.Id,
			profile.Protocol, profile.Address, err)
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100, failReason: fmt.Sprintf("ParseUriHysteria failed: %v", err)}
	}

	hyCore := hysteria.HysteriaCore{Exited: make(chan error)}
	if err := hyCore.Start(yaml_config); err != nil {
		// The most common cause for a silent "always error" hysteria2
		// test result is a missing/broken hysteria binary. Surface a
		// clear, actionable log line so the user can install it. The
		// same text is also carried up to the TUI via failReason so the
		// user does not have to open core.log to diagnose it.
		reason := fmt.Sprintf("hysteria.Start failed: %v "+
			"(is the `hysteria` binary installed in /usr/bin or /usr/local/bin?)", err)
		logger.Warnf("test %s (gid=%d id=%d): %s",
			profile.Name, profile.GroupId, profile.Id, reason)
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100, failReason: reason}
	}
	defer hyCore.Stop()

	if !waitForListener("tcp", fmt.Sprintf("127.0.0.1:%d", port), 8*time.Second) {
		reason := fmt.Sprintf("hysteria SOCKS listener did not bind on port %d within 8s "+
			"(UDP handshake to %s:%s failed? check /tmp/whoisthat-hysteria-*.log)",
			port, profile.Address, extractPort(profile.Uri, profile.Protocol))
		logger.Warnf("test %s (gid=%d id=%d): %s",
			profile.Name, profile.GroupId, profile.Id, reason)
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100, failReason: reason}
	}
	return p.runSamples(profile, port, samples)
}

// runSamples issues N independent HTTP GETs through the SOCKS5 proxy at
// 127.0.0.1:port, using the test endpoints as fallback. Records per-
// sample latency (only the primary endpoint counts), computes median,
// max-min jitter, and loss%.
func (p *ProxyManager) runSamples(profile structs.Profile, port, samples int) pingResult {
	if samples < 1 {
		samples = 1
	}
	epoch := testEpoch.Load()
	p.testMu.RLock()
	timeoutSec := p.testConfig.TimeoutSeconds
	endpoints := append([]string(nil), p.testEndpoints...)
	p.testMu.RUnlock()
	timeout := time.Duration(timeoutSec) * time.Second
	if timeout <= 0 {
		timeout = 5 * time.Second
	}

	succ := make([]int, 0, samples)
	failures := 0
	for i := 0; i < samples; i++ {
		if testEpoch.Load() != epoch {
			// CancelTests was called mid-batch. Don't keep running.
			failures += samples - i
			break
		}
		ms, ok := p.oneSample(port, timeout, endpoints)
		if !ok {
			failures++
			continue
		}
		succ = append(succ, ms)
	}

	if len(succ) == 0 {
		return pingResult{
			latencyMs:   -1,
			sampleCount: samples,
			lossPct:     100,
			success:     false,
			failReason:  fmt.Sprintf("all %d sample(s) failed (%d%% loss, timeout or non-2xx via SOCKS5)", samples, 100),
		}
	}
	sort.Ints(succ)
	median := succ[len(succ)/2]
	jitter := 0
	if len(succ) > 1 {
		jitter = succ[len(succ)-1] - succ[0]
		if jitter < 0 {
			jitter = 0
		}
	}
	loss := 0
	if samples > 0 {
		loss = (failures * 100) / samples
	}
	return pingResult{
		latencyMs:   median,
		jitterMs:    jitter,
		lossPct:     loss,
		sampleCount: samples,
		success:     true,
	}
}

// oneSample does a single HTTP GET through SOCKS5 against the configured
// endpoint (with fallback to the alternates). Returns (latencyMs, true)
// on 2xx, (0, false) on any error or non-2xx.
func (p *ProxyManager) oneSample(port int, timeout time.Duration, endpoints []string) (int, bool) {
	dialer, err := goproxy.SOCKS5("tcp", fmt.Sprintf("127.0.0.1:%d", port), nil, goproxy.Direct)
	if err != nil {
		return 0, false
	}
	transport := &http.Transport{
		Dial: dialer.Dial,
	}
	client := &http.Client{
		Transport: transport,
		Timeout:   timeout,
	}
	for _, url := range endpoints {
		req, err := http.NewRequest("GET", url, nil)
		if err != nil {
			continue
		}
		start := time.Now()
		resp, err := client.Do(req)
		latency := time.Since(start)
		if err != nil {
			continue
		}
		resp.Body.Close()
		if resp.StatusCode >= 200 && resp.StatusCode < 300 {
			return int(latency.Milliseconds()), true
		}
	}
	return 0, false
}

// waitForListener polls a TCP address until it accepts a connection or
// the deadline passes. Replaces `time.Sleep(1*time.Second)` in the old
// tester — typically returns in 50-200ms once xray/hysteria binds.
func waitForListener(network, addr string, deadline time.Duration) bool {
	deadlineAt := time.Now().Add(deadline)
	for time.Now().Before(deadlineAt) {
		conn, err := net.DialTimeout(network, addr, 100*time.Millisecond)
		if err == nil {
			conn.Close()
			return true
		}
		time.Sleep(50 * time.Millisecond)
	}
	return false
}

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

func extractPort(uri string, protocol string) string {
	// Try the regex extraction first (handles most URI forms with an
	// explicit @host:port). vmess://<base64> doesn't have a visible
	// @host:port so this returns "" — the protocol-default switch below
	// fills it in.
	m := rePort.FindStringSubmatch(uri)
	if len(m) == 2 {
		if _, err := strconv.Atoi(m[1]); err == nil {
			return m[1]
		}
	}
	switch protocol {
	case "vless", "vmess", "trojan":
		return "443"
	case "shadowsocks":
		return "8388"
	case "socks":
		return "1080"
	}
	return "443"
}