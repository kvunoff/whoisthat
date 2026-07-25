package mainproxy

import (
	"encoding/json"
	"net"
	"sync"
	"time"
	"whoisthat-core/db"
	"whoisthat-core/lib"
	appconfig "whoisthat-core/lib/AppConfig"
	portpool "whoisthat-core/lib/PortPool"
	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/proxy/hysteria"
	"whoisthat-core/lib/proxy/xray"
	"whoisthat-core/structs"
)

// connect -> can also switch
// stop -> stops everything
// getStatus -> status
// test -> limit to 5 concurrent tests, but simple return interface

// coreProc is the union of methods mainproxy uses to drive either xray-core
// (vless/vmess/trojan/ss/socks) or the official hysteria2 client subprocess.
// Both xray.XrayCore and hysteria.HysteriaCore satisfy this interface.
type coreProc interface {
	Start(stdinConfig []byte) error
	Stop()
	IsRunning() bool
	ExitedCh() chan error
}

type ProxyManager struct {
	status            structs.ProxyStatus
	mu                sync.Mutex
	core              coreProc
	StatusChanged     chan structs.ProxyStatus
	StatsChanged      chan structs.TrafficStats
	testChannel       chan TestRequest
	TestResultChannel chan TestResult
	portPool          *portpool.PortPool
	statsCancel       chan struct{}
	statsApiPort      int
	DB                *db.DB
	proxyIPs          []string

	// Per-group batch progress tracker. SeedTestProgress initializes a
	// group's (tested, total) tuple; IncrementTestProgress atomically
	// bumps the tested count and returns the new value plus the total.
	// When tested == total the entry is cleared. The TCPServer broadcasts
	// a `test-progress` notification for each increment so the TUI can
	// render "Testing 12/30…".
	progressMu     sync.Mutex
	progressGroups map[int32]*struct {
		tested int64
		total  int
	}

	testMu       sync.RWMutex
	testConfig   structs.TestConfig
	testEndpoints []string
}

func (p *ProxyManager) Init() {
	p.status = structs.ProxyStatus{
		Connection: "disconnected",
	}
	p.progressGroups = map[int32]*struct {
		tested int64
		total  int
	}{}
	p.StatusChanged = make(chan structs.ProxyStatus, 8)
	p.StatsChanged = make(chan structs.TrafficStats, 8)
	p.testConfig = structs.TestConfig{
		Concurrency:    16,
		TimeoutSeconds: 5,
		SamplesPerTest: 3,
		TestEndpoint:   "https://cp.cloudflare.com/generate_204",
		AutoTestOnSub:  true,
	}
	p.testEndpoints = []string{
		"https://cp.cloudflare.com/generate_204",
		"https://www.gstatic.com/generate_204",
		"https://www.bing.com/",
	}
	test_channel := make(chan TestRequest, 256)
	go p.listenForTests(test_channel)
	p.testChannel = test_channel
	p.TestResultChannel = make(chan TestResult, 32)
	p.core = &xray.XrayCore{
		Exited: make(chan error),
	}
	test_port_range := appconfig.GetConfig().TestPortRange
	p.portPool = portpool.CreatePortPool(test_port_range.Start, test_port_range.End)
}

// exitWatcher tracks the single goroutine reading p.core.ExitedCh() so we
// can stop it before swapping core on the next Connect (previously it
// leaked one goroutine per connect, all racing on the swapped field).
var exitWatcherDone chan struct{}
var exitWatcherMu sync.Mutex

// retireExitWatcher signals the active exit-watcher goroutine to stop and
// clears the package-global reference. Idempotent and nil-safe.
func (p *ProxyManager) retireExitWatcher() {
	exitWatcherMu.Lock()
	if exitWatcherDone != nil {
		close(exitWatcherDone)
		exitWatcherDone = nil
	}
	exitWatcherMu.Unlock()
}

// startExitWatcher registers a fresh done channel for this Connect cycle and
// spawns the watcher goroutine. The previous watcher (if any) must already
// have been retired by the caller.
func (p *ProxyManager) startExitWatcher() chan struct{} {
	done := make(chan struct{})
	exitWatcherMu.Lock()
	exitWatcherDone = done
	exitWatcherMu.Unlock()
	go p.watchExit(done)
	return done
}

// watchExit is the Exited-watcher goroutine body. On xray exit it flips the
// status to disconnected and emits a non-blocking StatusChanged update (the
// channel is buffered at 8; if full we drop rather than deadlock the daemon).
// It returns when done is closed (retire) or Exited is closed.
func (p *ProxyManager) watchExit(done chan struct{}) {
	for {
		select {
		case <-done:
			return
		case _, ok := <-p.core.ExitedCh():
			if !ok {
				return
			}
			p.mu.Lock()
			p.status = structs.ProxyStatus{
				Connection: "disconnected",
			}
			p.mu.Unlock()
			// Non-blocking send: StatusChanged is buffered (8) and the
			// server's handleStatusChange drains it; if it's somehow
			// full we'd rather drop than deadlock the daemon.
			select {
			case p.StatusChanged <- p.status:
			default:
				logger.Warn("StatusChanged full; dropping disconnected event")
			}
		}
	}
}

// isHysteriaProtocol reports whether the profile's protocol must run via the
// official hysteria2 binary instead of xray-core.
func isHysteriaProtocol(protocol string) bool {
	return protocol == "hysteria2" || protocol == "hy2"
}

func (p *ProxyManager) Connect(profile structs.Profile, tunName string) error {
	p.mu.Lock()
	defer p.mu.Unlock()

	if p.core.IsRunning() {
		p.core.Stop()
	}

	// Stop the previous Exited-watcher goroutine before swapping core
	// out from under it. Without this, every Connect leaked one goroutine,
	// and all of them raced on the swapped field reading the latest Exited.
	p.retireExitWatcher()

	if p.status.Connection == "connected" {
		p.status = structs.ProxyStatus{
			Connection: "disconnected",
		}
		select {
		case p.StatusChanged <- p.status:
		default:
		}
	}

	app_config := appconfig.GetConfig()

	// Default 0 → ss-based fallback. Set to a portPool port on the xray path.
	apiPort := 0

	if isHysteriaProtocol(profile.Protocol) {
		// Hysteria2 is NOT supported by xray-core; the official hysteria2
		// client is spawned with a YAML config produced by the parser. xray
		// JSON-injection (stats/routing) does not apply — hysteria2 has no
		// equivalent of xray's `outbounds`/`routing`/`stats` blocks.
		yaml_config, err := lib.ParseUriHysteria(profile.Uri, app_config.SocksPort, app_config.HttpPort)
		if err != nil {
			return err
		}
		p.core = &hysteria.HysteriaCore{Exited: make(chan error)}
		if err := p.core.Start(yaml_config); err != nil {
			return err
		}
	} else {
		xray_config, err := lib.ParseUri(profile.Uri, app_config.SocksPort, app_config.HttpPort)
		if err != nil {
			return err
		}

		// Allocate a port for xray's gRPC StatsService listener. On failure
		// (pool exhausted) we fall back to ss-based stats — apiPort stays 0,
		// which disables injection of the api inbound/outbound and routes the
		// collector to the legacy ss -tie path. The port is released in Stop().
		allocated, apiErr := p.portPool.GetPort()
		if apiErr != nil {
			logger.Warnf("stats: port pool exhausted, falling back to ss -tie: %v", apiErr)
		} else {
			apiPort = allocated
		}

		// Inject stats tracking config (policy + stats counters, optional
		// dokodemo-door API inbound when apiPort > 0).
		xray_config, err = injectStatsConfig(xray_config, apiPort)
		if err != nil {
			logger.Warnf("failed to inject stats config: %v", err)
		}

		// Inject routing rules + direct/block outbounds
		if p.DB != nil {
			prevOutboundsCount := countOutbounds(xray_config)
			xray_config, err = injectRoutingConfig(xray_config, p.DB)
			if err != nil {
				logger.Warnf("failed to inject routing config: %v", err)
			}
			afterOutboundsCount := countOutbounds(xray_config)
			logger.Infof("routing: outbounds %d → %d", prevOutboundsCount, afterOutboundsCount)
		}

		p.core = &xray.XrayCore{Exited: make(chan error)}
		if err := p.core.Start(xray_config); err != nil {
			return err
		}
	}

	p.proxyIPs = resolveProfileIPs(profile.Address)

	logger.Infof("connected to %s (%s://%s)",
		profile.Name, profile.Protocol, profile.Address)

	// Stop the previous stats goroutine and release the apiPort it was using
	// (defends against leaks when Connect is called without an intervening
	// Stop, e.g. switching profiles).
	if p.statsCancel != nil {
		close(p.statsCancel)
		p.statsCancel = nil
	}
	if p.statsApiPort != 0 && p.portPool != nil {
		p.portPool.ReleasePort(p.statsApiPort)
		p.statsApiPort = 0
	}

	// Start stats collector. For xray use the gRPC StatsService on apiPort;
	// for hysteria2 (and the apiPort=0 fallback) use ss -tie on the SOCKS port.
	// Direct traffic is read from sysfs for the TUN device when one is configured.
	p.statsCancel = make(chan struct{})
	p.statsApiPort = apiPort
	go p.collectStats(app_config.SocksPort, p.statsApiPort, tunName, p.statsCancel, p.StatsChanged)

	p.status = structs.ProxyStatus{
		Connection:  "connected",
		Profile:     profile,
		ConnectedAt: time.Now().Unix(),
	}
	select {
	case p.StatusChanged <- p.status:
	default:
	}

	// Spawn a single Exited-watcher for this core instance. The done
	// channel lets the next Connect (or Stop) retire it cleanly.
	p.startExitWatcher()

	return nil
}

func (p *ProxyManager) Stop() {
	p.mu.Lock()
	defer p.mu.Unlock()
	logger.Info("disconnecting")
	// Retire the Exited-watcher goroutine.
	p.retireExitWatcher()
	if p.statsCancel != nil {
		close(p.statsCancel)
		p.statsCancel = nil
	}
	if p.statsApiPort != 0 {
		if p.portPool != nil {
			p.portPool.ReleasePort(p.statsApiPort)
		}
		p.statsApiPort = 0
	}
	p.core.Stop()
	p.status = structs.ProxyStatus{
		Connection: "disconnected",
	}
	select {
	case p.StatusChanged <- p.status:
	default:
	}
	// Send zero stats on disconnect
	select {
	case p.StatsChanged <- structs.TrafficStats{}:
	default:
	}
}

func (p *ProxyManager) GetStatus() structs.ProxyStatus {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.status
}

func countOutbounds(configJSON []byte) int {
	var cfg map[string]interface{}
	if err := json.Unmarshal(configJSON, &cfg); err != nil {
		return -1
	}
	obs, ok := cfg["outbounds"].([]interface{})
	if !ok {
		return 0
	}
	return len(obs)
}

func (p *ProxyManager) KillSwitchEnabled() bool {
	return appconfig.GetConfig().KillSwitchEnabled
}

func (p *ProxyManager) GetProxyIPs() []string {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.proxyIPs
}

func resolveProfileIPs(address string) []string {
	ips, err := net.LookupIP(address)
	if err != nil {
		return nil
	}
	var result []string
	for _, ip := range ips {
		result = append(result, ip.String())
	}
	return result
}
