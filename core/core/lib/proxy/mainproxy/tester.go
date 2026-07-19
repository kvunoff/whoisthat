package mainproxy

import (
	"fmt"
	"net"
	"net/http"
	"regexp"
	"sort"
	"strconv"
	"sync/atomic"
	"time"
	"whoisthat-core/lib"
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
}

type TestRequest struct {
	Profile     structs.Profile
	Method      string
	SampleCount int
}

// testEpoch is bumped by CancelTests to invalidate in-flight test
// goroutines. A test goroutine captures the epoch value at enqueue time
// and periodically checks it; if the value changed, the goroutine aborts
// (so its subprocess doesn't linger killing mid-sample) — new tests for
// the new epoch will be queued by the caller. This avoids hard-killing
// xray/hysteria subprocesses which can leak if killed mid-handshake.
var testEpoch atomic.Int64

// testEndpoints is the tried-in-order fallback list of URLs used for the
// HTTP round-trip measurement. The first one that returns 2xx is the
// measured sample. Endpoints after the first act purely as reachability
// fallback — only the first endpoint's latency is recorded to keep
// samples comparable.
var testEndpoints = []string{
	"https://cp.cloudflare.com/generate_204",
	"https://www.gstatic.com/generate_204",
	"https://www.bing.com/",
}

// testConfigDefaults are used when no explicit value is supplied on a
// test request (set by the TUI / Defaults()). All values are mutable via
// the SetTestConfig command.
var testConfig = structs.TestConfig{
	Concurrency:    16,
	TimeoutSeconds: 5,
	SamplesPerTest: 3,
	TestEndpoint:   "https://cp.cloudflare.com/generate_204",
	AutoTestOnSub:  true,
}

// SetTestConfig replaces the live test configuration. Called by the
// SetTestConfig command handler. Receiver is the ProxyManager so the
// command-handler call site reads naturally; the configuration itself
// is package-global because tests share a single process-wide config.
func (p *ProxyManager) SetTestConfig(cfg structs.TestConfig) {
	if cfg.Concurrency < 1 {
		cfg.Concurrency = testConfig.Concurrency
	}
	if cfg.TimeoutSeconds < 1 {
		cfg.TimeoutSeconds = testConfig.TimeoutSeconds
	}
	if cfg.SamplesPerTest < 1 {
		cfg.SamplesPerTest = testConfig.SamplesPerTest
	}
	if cfg.TestEndpoint == "" {
		cfg.TestEndpoint = testConfig.TestEndpoint
	}
	endpoints := []string{cfg.TestEndpoint}
	for _, e := range testEndpoints {
		if e != cfg.TestEndpoint {
			endpoints = append(endpoints, e)
		}
	}
	testEndpoints = endpoints
	testConfig = cfg
}

func (p *ProxyManager) GetTestConfig() structs.TestConfig {
	return testConfig
}

// CancelTests invalidates all in-flight test goroutines by bumping the
// epoch. Already-spawned xray/hysteria subprocesses for in-flight samples
// finish their current sample (or time out) and then abort — no hard
// kills, no orphan risk.
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
	samples := testConfig.SamplesPerTest
	p.testChannel <- TestRequest{Profile: profile, Method: method, SampleCount: samples}
}

// TestGroup enqueues tests for every profile in the given group. The
// caller is the commands/test.go TestGroup handler — it pre-loads the
// profiles from the DB and passes them in. Returns the number of tests
// enqueued so the handler can include it in a TestProgress broadcast.
func (p *ProxyManager) TestGroup(profiles []structs.Profile, method string, samples int) int {
	if samples < 1 {
		samples = testConfig.SamplesPerTest
	}
	for _, prof := range profiles {
		p.testChannel <- TestRequest{Profile: prof, Method: method, SampleCount: samples}
	}
	return len(profiles)
}

func (p *ProxyManager) listenForTests(tests_chan chan TestRequest) {
	concurrency := testConfig.Concurrency
	if concurrency < 1 {
		concurrency = 16
	}
	sem := make(chan struct{}, concurrency)

	for req := range tests_chan {
		// Re-check concurrency in case SetTestConfig changed it.
		if cap(sem) != testConfig.Concurrency && testConfig.Concurrency > 0 {
			concurrency = testConfig.Concurrency
			newSem := make(chan struct{}, concurrency)
			sem = newSem
		}
		sem <- struct{}{}
		go func(req TestRequest) {
			defer func() { <-sem }()
			ping := p.test(req)
			p.sendTestResult(req.Profile, ping)
		}(req)
	}
}

// pingResult captures the rich metadata produced by a multi-sample test.
// Latency is the median of successful samples (ms), or -1 if all failed.
// Jitter is max-min of successful samples (ms).
// LossPct is the percentage of failed samples (0..100).
// Success is true if at least one sample completed with a 2xx response.
type pingResult struct {
	latencyMs   int
	jitterMs    int
	lossPct     int
	success     bool
	sampleCount int
}

func (p *ProxyManager) test(req TestRequest) pingResult {
	samples := req.SampleCount
	if samples < 1 {
		samples = testConfig.SamplesPerTest
	}

	// Fast pre-filter: a raw TCP reachability dial. For non-hysteria
	// protocols this is a meaningful quick reject — if the server's port
	// isn't even open, don't waste 5s spawning xray. For hysteria2 the
	// port is UDP, so the TCP dial is unrelated to reachability — skip.
	if req.Method == "tcp" {
		ms := p.testTcpOnly(req.Profile)
		return pingResult{
			latencyMs:   ms,
			success:     ms > 0,
			sampleCount: 1,
		}
	}

	if !isHysteriaProtocol(req.Profile.Protocol) {
		// Quick reject: dial the server port directly. If unreachable
		// we skip the expensive spawn+sample cycle entirely. This is
		// purely an optimization — never written to DB on its own.
		if !p.serverReachable(req.Profile) {
			return pingResult{
				latencyMs:   -1,
				success:     false,
				sampleCount: samples,
				lossPct:     100,
			}
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
	start := time.Now()
	conn, err := net.DialTimeout("tcp", target, time.Duration(testConfig.TimeoutSeconds)*time.Second)
	if err != nil {
		return -1
	}
	conn.Close()
	return int(time.Since(start).Milliseconds())
}

// serverReachable is a fast pre-filter dial. Returns true if the
// profile's server:port accepts a TCP connection within 2s. Latency is
// not recorded — this is a yes/no gate.
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
	conn, err := net.DialTimeout("tcp", target, 2*time.Second)
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
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100}
	}
	defer p.portPool.ReleasePort(port)

	parsed, err := lib.ParseUri(profile.Uri, port, -1)
	if err != nil {
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100}
	}

	xray_core := xray.XrayCore{Exited: make(chan error)}
	if err := xray_core.Start(parsed); err != nil {
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100}
	}
	defer xray_core.Stop()

	if !waitForListener("tcp", fmt.Sprintf("127.0.0.1:%d", port), 2*time.Second) {
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100}
	}
	return p.runSamples(profile, port, samples)
}

// testViaHysteria spawns the hysteria2 client with the profile's parsed
// YAML and uses its SOCKS5 listener for the HTTP samples. Mirrors
// testViaXray but calls ParseUriHysteria + HysteriaCore.
func (p *ProxyManager) testViaHysteria(profile structs.Profile, samples int) pingResult {
	port, err := p.portPool.GetPort()
	if err != nil {
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100}
	}
	defer p.portPool.ReleasePort(port)

	yaml_config, err := lib.ParseUriHysteria(profile.Uri, port, -1)
	if err != nil {
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100}
	}

	hyCore := hysteria.HysteriaCore{Exited: make(chan error)}
	if err := hyCore.Start(yaml_config); err != nil {
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100}
	}
	defer hyCore.Stop()

	if !waitForListener("tcp", fmt.Sprintf("127.0.0.1:%d", port), 3*time.Second) {
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100}
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
	timeout := time.Duration(testConfig.TimeoutSeconds) * time.Second
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
		ms, ok := p.oneSample(port, timeout)
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
func (p *ProxyManager) oneSample(port int, timeout time.Duration) (int, bool) {
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
	for _, url := range testEndpoints {
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
	p.TestResultChannel <- TestResult{
		Success:     ping.success,
		Profile:     profile,
		SampleCount: ping.sampleCount,
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
