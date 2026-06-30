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
	"whoisthat-core/lib/proxy/xray"
	"whoisthat-core/structs"
)

// connect -> can also switch
// stop -> stops everything
// getStatus -> status
// test -> limit to 5 concurrent tests, but simple return interface

type ProxyManager struct {
	status            structs.ProxyStatus
	mu                sync.Mutex
	xray_core         xray.XrayCore
	StatusChanged     chan structs.ProxyStatus
	StatsChanged      chan structs.TrafficStats
	testChannel       chan TestRequest
	TestResultChannel chan TestResult
	portPool          *portpool.PortPool
	statsCancel       chan struct{}
	DB                *db.DB
	proxyIPs          []string
}

func (p *ProxyManager) Init() {
	p.status = structs.ProxyStatus{
		Connection: "disconnected",
	}
	// Buffered so a transient send from xray-core's Exited watcher doesn't
	// block forever if the server's handleStatusChange goroutine is parked
	// inside a Broadcast. Combined with the per-client outbound goroutines
	// in TCPServer, this breaks the historic deadlock chain where a stuck
	// client write back-propagated into Connect/Stop.
	p.StatusChanged = make(chan structs.ProxyStatus, 8)
	p.StatsChanged = make(chan structs.TrafficStats, 8)
	test_channel := make(chan TestRequest)
	go p.listenForTests(test_channel)
	p.testChannel = test_channel
	p.TestResultChannel = make(chan TestResult)
	p.xray_core = xray.XrayCore{
		Exited: make(chan error),
	}
	test_port_range := appconfig.GetConfig().TestPortRange
	p.portPool = portpool.CreatePortPool(test_port_range.Start, test_port_range.End)
}

// exitWatcher tracks the single goroutine reading p.xray_core.Exited so we
// can stop it before swapping xray_core on the next Connect (previously it
// leaked one goroutine per connect, all racing on the swapped field).
var exitWatcherDone chan struct{}
var exitWatcherMu sync.Mutex

func (p *ProxyManager) Connect(profile structs.Profile) error {
	p.mu.Lock()
	defer p.mu.Unlock()

	if p.xray_core.IsRunning() {
		p.xray_core.Stop()
	}

	// Stop the previous Exited-watcher goroutine before swapping xray_core
	// out from under it. Without this, every Connect leaked one goroutine,
	// and all of them raced on the swapped field reading the latest Exited.
	exitWatcherMu.Lock()
	if exitWatcherDone != nil {
		close(exitWatcherDone)
		exitWatcherDone = nil
	}
	exitWatcherMu.Unlock()

	p.xray_core = xray.XrayCore{
		Exited: make(chan error),
	}

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
	xray_config, err := lib.ParseUri(profile.Uri, app_config.SocksPort, app_config.HttpPort)
	if err != nil {
		return err
	}

	// Inject stats tracking config (policy + stats counters)
	xray_config, err = injectStatsConfig(xray_config)
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

	if err := p.xray_core.Start(xray_config); err != nil {
		return err
	}

	p.proxyIPs = resolveProfileIPs(profile.Address)

	logger.Infof("connected to %s (%s://%s)",
		profile.Name, profile.Protocol, profile.Address)

	// Start stats collector (uses ss -tie on SOCKS port, no xray gRPC API)
	if p.statsCancel != nil {
		close(p.statsCancel)
	}
	p.statsCancel = make(chan struct{})
	go p.collectStats(app_config.SocksPort, p.statsCancel, p.StatsChanged)

	p.status = structs.ProxyStatus{
		Connection:  "connected",
		Profile:     profile,
		ConnectedAt: time.Now().Unix(),
	}
	select {
	case p.StatusChanged <- p.status:
	default:
	}

	// Spawn a single Exited-watcher for this xray_core instance. The done
	// channel lets the next Connect (or Stop) retire it cleanly.
	done := make(chan struct{})
	exitWatcherMu.Lock()
	exitWatcherDone = done
	exitWatcherMu.Unlock()

	go func() {
		for {
			select {
			case <-done:
				return
			case _, ok := <-p.xray_core.Exited:
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
	}()

	return nil
}

func (p *ProxyManager) Stop() {
	p.mu.Lock()
	defer p.mu.Unlock()
	logger.Info("disconnecting")
	// Retire the Exited-watcher goroutine.
	exitWatcherMu.Lock()
	if exitWatcherDone != nil {
		close(exitWatcherDone)
		exitWatcherDone = nil
	}
	exitWatcherMu.Unlock()
	if p.statsCancel != nil {
		close(p.statsCancel)
		p.statsCancel = nil
	}
	p.xray_core.Stop()
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
