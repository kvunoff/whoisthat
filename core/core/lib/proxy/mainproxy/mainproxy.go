package mainproxy

import (
	"whoisthat-core/lib"
	appconfig "whoisthat-core/lib/AppConfig"
	portpool "whoisthat-core/lib/PortPool"
	"whoisthat-core/lib/proxy/xray"
	"whoisthat-core/structs"
	"log"
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
	apiPort           int
	statsCancel       chan struct{}
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

	// Allocate dedicated API port for stats gRPC (avoids SOCKS/gRPC conflict)
	if p.apiPort > 0 {
		p.portPool.ReleasePort(p.apiPort)
	}
	apiPort, err := p.portPool.GetPort()
	if err != nil {
		log.Println("WARNING: failed to allocate API port, traffic stats disabled:", err)
		p.apiPort = 0
	} else {
		p.apiPort = apiPort
	}

	// Inject stats/policy/api into xray config
	if p.apiPort > 0 {
		xray_config, _ = injectStatsConfig(xray_config, p.apiPort)
	}

	if err := p.xray_core.Start(xray_config); err != nil {
		return err
	}

	// Start stats collector
	if p.statsCancel != nil {
		close(p.statsCancel)
	}
	p.statsCancel = make(chan struct{})
	if p.apiPort > 0 {
		go p.collectStats(p.apiPort, p.statsCancel, p.StatsChanged)
	}

	p.status = structs.ProxyStatus{
		Connection:  "connected",
		Profile:     profile,
		ConnectedAt: time.Now().Unix(),
	}
	p.StatusChanged <- p.status
	log.Println("changing connection status to", p.status.Connection)

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
	if p.statsCancel != nil {
		close(p.statsCancel)
		p.statsCancel = nil
	}
	p.xray_core.Stop()
	p.status = structs.ProxyStatus{
		Connection: "disconnected",
	}
	p.StatusChanged <- p.status
	// Release API port
	if p.apiPort > 0 {
		p.portPool.ReleasePort(p.apiPort)
		p.apiPort = 0
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
