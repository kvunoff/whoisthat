package mainproxy

import (
	"fmt"
	"net"
	"net/http"
	"regexp"
	"sort"
	"strconv"
	"time"

	"whoisthat-core/lib"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/geo"
	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/proxy/xray"
	"whoisthat-core/lib/xrayjson"
	"whoisthat-core/structs"

	goproxy "golang.org/x/net/proxy"
)

var rePort = regexp.MustCompile(`@[\w.\-\[\]:]+:(\d+)`)

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
		// For hysteria2 (UDP), skip the direct TCP probe.
		if !p.serverReachable(req.Profile) {
			// Still useful for debugging, but at default WARN it flooded the
			// log: every tested profile produced a line even when xray later
			// connected fine. The final result stays at WARN.
			logger.Debugf("test %s (gid=%d id=%d): serverReachable failed for %s://%s — proceeding anyway",
				req.Profile.Name, req.Profile.GroupId, req.Profile.Id,
				req.Profile.Protocol, req.Profile.Address)
		}
	}
	return p.testViaXray(req.Profile, req.Method, samples)
}

// testTcpOnly is the legacy "raw TCP dial to server:port" measurement.
// It does NOT validate protocol/auth/transport — only that something is
// listening on the target port. Returned as a final result when the user
// has explicitly chosen the "tcp" test method.
func (p *ProxyManager) testTcpOnly(profile structs.Profile) int {
	port := profilePort(profile)
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
	port := profilePort(profile)
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
// GETs/HEADs via the SOCKS5 proxy and computes median + jitter + loss.
func (p *ProxyManager) testViaXray(profile structs.Profile, method string, samples int) pingResult {
	port, err := p.portPool.GetPort()
	if err != nil {
		logger.Warnf("test %s (gid=%d id=%d): port pool exhausted: %v",
			profile.Name, profile.GroupId, profile.Id, err)
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100, failReason: "no free test port available"}
	}
	defer p.portPool.ReleasePort(port)

	var parsed []byte
	var geoDir string
	if profile.Managed {
		if p.DB == nil {
			return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100, failReason: "managed profile requires a database"}
		}
		raw, loadErr := p.DB.LoadProfileXrayConfig(profile.GroupId, profile.Id)
		if loadErr != nil {
			return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100, failReason: fmt.Sprintf("load provider config: %v", loadErr)}
		}
		geoDir, _ = p.DB.ResolveGroupGeoDir(profile.GroupId)
		prepared, prepErr := xrayjson.Prepare(raw, xrayjson.Options{
			SocksPort:    port,
			HttpPort:     -1,
			DNSServers:   appconfig.GetConfig().DnsServers,
			GlobalGeoExt: geo.EnsureGlobalCopies(geoDir),
		})
		if prepErr != nil {
			return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100, failReason: fmt.Sprintf("prepare provider config: %v", prepErr)}
		}
		parsed = prepared.Config
	} else {
		parsed, err = lib.ParseUri(profile.Uri, port, -1)
	}
	if err != nil {
		logger.Warnf("test %s (gid=%d id=%d): ParseUri failed for %s://%s: %v",
			profile.Name, profile.GroupId, profile.Id,
			profile.Protocol, profile.Address, err)
		return pingResult{latencyMs: -1, sampleCount: samples, lossPct: 100, failReason: fmt.Sprintf("ParseUri failed: %v", err)}
	}

	xray_core := xray.XrayCore{Exited: make(chan error, 1)}
	if err := xray_core.Start(parsed, geoDir); err != nil {
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
	return p.runSamples(profile, method, port, samples)
}

// runSamples issues N independent HTTP GETs/HEADs through the SOCKS5 proxy at
// 127.0.0.1:port, using the test endpoints as fallback. Records per-
// sample latency (only the primary endpoint counts), computes median,
// max-min jitter, and loss%.
func (p *ProxyManager) runSamples(profile structs.Profile, method string, port, samples int) pingResult {
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
		ms, ok := p.oneSample(port, method, timeout, endpoints)
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

// oneSample does a single HTTP GET/HEAD through SOCKS5 against the configured
// endpoint (with fallback to the alternates). Returns (latencyMs, true)
// on 2xx, (0, false) on any error or non-2xx.
func (p *ProxyManager) oneSample(port int, method string, timeout time.Duration, endpoints []string) (int, bool) {
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
	httpMethod := "GET"
	if method == "http-head" {
		httpMethod = "HEAD"
	}
	for _, url := range endpoints {
		req, err := http.NewRequest(httpMethod, url, nil)
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

func profilePort(profile structs.Profile) string {
	if profile.Port > 0 {
		return strconv.Itoa(profile.Port)
	}
	return extractPort(profile.Uri, profile.Protocol)
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
