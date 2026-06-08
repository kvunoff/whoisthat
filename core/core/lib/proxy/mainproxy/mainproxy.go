package mainproxy

import (
	"whoisthat-core/db"
	"whoisthat-core/lib"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	portpool "whoisthat-core/lib/PortPool"
	"whoisthat-core/lib/proxy/xray"
	"whoisthat-core/structs"
	"sync"
	"time"
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
	testChannel       chan structs.Profile
	TestResultChannel chan TestResult
	portPool          *portpool.PortPool
	statsCancel       chan struct{}
	DB                *db.DB
}

func (p *ProxyManager) Init() {
	p.status = structs.ProxyStatus{
		Connection: "disconnected",
	}
	p.StatusChanged = make(chan structs.ProxyStatus)
	p.StatsChanged = make(chan structs.TrafficStats)
	test_channel := make(chan structs.Profile)
	go p.listenForTests(test_channel)
	p.testChannel = test_channel
	p.TestResultChannel = make(chan TestResult)
	p.xray_core = xray.XrayCore{
		Exited: make(chan error),
	}
	test_port_range := appconfig.GetConfig().TestPortRange
	p.portPool = portpool.CreatePortPool(test_port_range.Start, test_port_range.End)
}

func (p *ProxyManager) Connect(profile structs.Profile) error {
	p.mu.Lock()
	defer p.mu.Unlock()

	if p.xray_core.IsRunning() {
		p.xray_core.Stop()
	}

	p.xray_core = xray.XrayCore{
		Exited: make(chan error),
	}

	if p.status.Connection == "connected" {
		p.status = structs.ProxyStatus{
			Connection: "disconnected",
		}
		p.StatusChanged <- p.status
	}

	app_config := appconfig.GetConfig()
	xray_config, err := lib.ParseUri(profile.Uri, app_config.SocksPort, app_config.HttpPort)
	if err != nil {
		return err
	}

	// Inject stats tracking config (policy + stats counters)
	xray_config, _ = injectStatsConfig(xray_config)

	// Inject routing rules + direct/block outbounds
	if p.DB != nil {
		var err error
		xray_config, err = injectRoutingConfig(xray_config, p.DB)
		if err != nil {
			logger.Warnf("failed to inject routing config: %v", err)
		}
	}

	if err := p.xray_core.Start(xray_config); err != nil {
		return err
	}

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
	p.StatusChanged <- p.status

	go func() {
		for {
			_, ok := <-p.xray_core.Exited
			if !ok {
				return
			}
			p.mu.Lock()
			p.status = structs.ProxyStatus{
				Connection: "disconnected",
			}
			p.mu.Unlock()
			p.StatusChanged <- p.status
		}
	}()

	return nil
}

func (p *ProxyManager) Stop() {
	p.mu.Lock()
	defer p.mu.Unlock()
	logger.Info("disconnecting")
	if p.statsCancel != nil {
		close(p.statsCancel)
		p.statsCancel = nil
	}
	p.xray_core.Stop()
	p.status = structs.ProxyStatus{
		Connection: "disconnected",
	}
	p.StatusChanged <- p.status
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
