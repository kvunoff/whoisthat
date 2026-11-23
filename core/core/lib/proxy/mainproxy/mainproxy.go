package mainproxy

import (
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"strings"
	"sync"
	"sync/atomic"
	"time"
	"whoisthat-core/db"
	"whoisthat-core/lib"
	appconfig "whoisthat-core/lib/AppConfig"
	portpool "whoisthat-core/lib/PortPool"
	"whoisthat-core/lib/logger"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/lib/proxy/xray"
	"whoisthat-core/lib/traffic"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

// connect -> can also switch
// stop -> stops everything
// getStatus -> status
// test -> limit to 5 concurrent tests, but simple return interface

// coreProc is the interface mainproxy uses to drive the Xray subprocess.
// It is kept as an interface so tests can substitute a fake core.
type coreProc interface {
	Start(stdinConfig []byte, assetDir string) error
	Stop()
	IsRunning() bool
	ExitedCh() chan error
}

// Test seam: Connect replaces the process unconditionally, so tests need a
// way to inject a fake core.
var newCore = func() coreProc {
	return &xray.XrayCore{Exited: make(chan error, 1)}
}

type ProxyManager struct {
	status            structs.ProxyStatus
	mu                sync.Mutex
	core              coreProc
	StatusChanged     chan structs.ProxyStatus
	StatsChanged      chan structs.TrafficSample
	testChannel       chan TestRequest
	TestResultChannel chan TestResult
	portPool          *portpool.PortPool
	statsApiPort      int
	DB                *db.DB
	proxyIPs          []string
	profileIPs        []string
	bootstrapHosts    map[string][]string
	tunEnabled        bool
	tunName           string

	// Traffic accounting. The meter owns sampling; trafficSnapshot is a
	// separate atomic copy for synchronous application-state reads, so the
	// broadcast channel is never consumed by a reader.
	trafficSnapshot atomic.Value
	trafficSeq      uint64
	meter           *traffic.Meter

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

	testMu        sync.RWMutex
	testConfig    structs.TestConfig
	testEndpoints []string

	exitHandlerMu    sync.RWMutex
	onUnexpectedExit func(string)

	// preStartMu guards the handler that runs after the profile's server IPs
	// are resolved and before Xray starts (the daemon uses it to install the
	// kill-switch allowlist).
	preStartMu sync.RWMutex
	onPreStart PreStartFunc

	// directTargets: destinations of direct-outbound rules, accepted by the
	// armed kill-switch. Refreshed on connect and on every routing reload.
	directTargets directTargetStore

	// tunActive mirrors tunEnabled for the kill-switch pre-start handler,
	// which runs while p.mu is held.
	tunActive atomic.Bool

	// exit-watcher state. The mutex is separate from p.mu so retire/start stay
	// safe from any caller, including Stop() which already holds p.mu.
	watchMu       sync.Mutex
	watchDone     chan struct{}
	watchGen      uint64
	watchStopping bool
}

// SetUnexpectedExitHandler registers a callback invoked when the proxy
// subprocess exits without a preceding Stop()/Connect(). The daemon uses it to
// tear down TUN state and re-arm the kill-switch instead of leaving the user
// with a half-configured network.
func (p *ProxyManager) SetUnexpectedExitHandler(fn func(string)) {
	p.exitHandlerMu.Lock()
	p.onUnexpectedExit = fn
	p.exitHandlerMu.Unlock()
}

// TunEnabled reports the mode of the last Connect/SetTunMode. It is atomic
// because the kill-switch pre-start handler reads it while p.mu is held.
func (p *ProxyManager) TunEnabled() bool { return p.tunActive.Load() }

func (p *ProxyManager) notifyUnexpectedExit(reason string) {
	p.exitHandlerMu.RLock()
	fn := p.onUnexpectedExit
	p.exitHandlerMu.RUnlock()
	if fn != nil {
		go fn(reason)
	}
}

// PreStartFunc runs after the profile's server IPs are resolved and before the
// Xray process starts. A non-nil error aborts the connect: the daemon must not
// report "connected" while an enabled kill-switch could not be armed.
type PreStartFunc func(proxyIPs []string) error

// ErrKillSwitchNotArmed marks a failure to install the enabled kill-switch, so
// the command layer can emit a specific warning instead of a generic
// "connect failed".
var ErrKillSwitchNotArmed = errors.New("kill-switch is enabled but could not be armed")

func (p *ProxyManager) SetPreStartHandler(fn PreStartFunc) {
	p.preStartMu.Lock()
	p.onPreStart = fn
	p.preStartMu.Unlock()
}

func (p *ProxyManager) runPreStartHandler(proxyIPs []string) error {
	p.preStartMu.RLock()
	fn := p.onPreStart
	p.preStartMu.RUnlock()
	if fn == nil {
		return nil
	}
	return fn(proxyIPs)
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
	p.StatsChanged = make(chan structs.TrafficSample, 1)
	p.trafficSnapshot.Store(structs.TrafficSample{})
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
	p.TestResultChannel = make(chan TestResult, 256)
	p.core = newCore()
	test_port_range := appconfig.GetConfig().TestPortRange
	p.portPool = portpool.CreatePortPool(test_port_range.Start, test_port_range.End)

	// In test mode the core has no Xray process, so a deterministic source
	// keeps the traffic contract observable end-to-end.
	if utils.TestMode() {
		p.startMeter(&traffic.SyntheticSource{})
	}
}

// GetTraffic returns the latest snapshot for application-state. It never reads
// the broadcast channel: doing so would swallow a frame and race the meter.
func (p *ProxyManager) GetTraffic() structs.TrafficSample {
	if v := p.trafficSnapshot.Load(); v != nil {
		return v.(structs.TrafficSample)
	}
	return structs.TrafficSample{}
}

func (p *ProxyManager) setTraffic(s structs.TrafficSample) {
	p.trafficSnapshot.Store(s)
}

// startMeter swaps the sampler for a new connection generation. Caller holds
// p.mu. The source is replaced with the real Xray/firewall sources in a later
// change; NopSource keeps the frame stream alive meanwhile.
func (p *ProxyManager) startMeter(src traffic.Source) {
	p.stopMeter()
	p.trafficSeq++
	session := fmt.Sprintf("%d-%d", p.trafficSeq, time.Now().UnixMilli())
	p.meter = traffic.New(src, session, p.StatsChanged, p.setTraffic)
	p.meter.Start()
}

func (p *ProxyManager) stopMeter() {
	if p.meter == nil {
		return
	}
	p.meter.Stop()
	p.meter = nil
}

// newTrafficSource classifies the exact config handed to Xray, so provider
// JSON, balancers and chained outbounds all map to the right category. Bypass
// counters only exist while TUN is up; without them the sample is still
// complete, not degraded.
func (p *ProxyManager) newTrafficSource(apiPort int, configJSON []byte, tunEnabled bool) traffic.Source {
	categories, err := traffic.ClassifyOutbounds(configJSON)
	if err != nil {
		logger.Warnf("traffic: cannot classify outbounds, stats disabled: %v", err)
		return traffic.NopSource{}
	}
	var firewall traffic.FirewallReader
	if tunEnabled {
		firewall = tunmode.BypassCounters
	}
	return traffic.NewCompositeSource(traffic.NewXraySource(apiPort, categories), firewall)
}

// retireExitWatcher signals the active exit-watcher goroutine to stop.
// Idempotent and nil-safe.
func (p *ProxyManager) retireExitWatcher() {
	p.watchMu.Lock()
	defer p.watchMu.Unlock()
	// Mark the stop as intentional *before* closing done: the watcher may win
	// the select race against the closed channel and observe the subprocess
	// exit, which must not be reported as a crash.
	p.watchStopping = true
	if p.watchDone != nil {
		close(p.watchDone)
		p.watchDone = nil
	}
	p.watchGen++
}

// startExitWatcher registers a fresh done channel for this Connect cycle and
// spawns the watcher goroutine. The previous watcher (if any) must already
// have been retired by the caller.
func (p *ProxyManager) startExitWatcher() chan struct{} {
	done := make(chan struct{})
	p.watchMu.Lock()
	p.watchStopping = false
	p.watchDone = done
	p.watchGen++
	gen := p.watchGen
	p.watchMu.Unlock()
	go p.watchExit(done, gen)
	return done
}

// watcherIsStale reports whether gen belongs to a watcher that has been retired
// or superseded. This closes the window where a retired goroutine wins the
// select race against a closed done channel and reports an intentional Stop as
// a crash.
func (p *ProxyManager) watcherIsStale(gen uint64) bool {
	p.watchMu.Lock()
	defer p.watchMu.Unlock()
	return gen != p.watchGen || p.watchStopping
}

// watchExit is the Exited-watcher goroutine body. On xray exit it flips the
// status to disconnected and emits a non-blocking StatusChanged update (the
// channel is buffered at 8; if full we drop rather than deadlock the daemon).
// It returns when done is closed (retire) or Exited is closed.
func (p *ProxyManager) watchExit(done chan struct{}, gen uint64) {
	for {
		select {
		case <-done:
			return
		case _, ok := <-p.core.ExitedCh():
			if !ok {
				return
			}
			if p.watcherIsStale(gen) {
				// Intentional Stop/Connect or a newer connect cycle: the caller
				// handles cleanup.
				return
			}
			reason := "proxy process exited unexpectedly"
			p.mu.Lock()
			p.status = structs.ProxyStatus{
				Connection: "disconnected",
				Reason:     reason,
			}
			p.stopMeter()
			status := p.status
			p.mu.Unlock()
			// Non-blocking send: StatusChanged is buffered (8) and the
			// server's handleStatusChange drains it; if it's somehow
			// full we'd rather drop than deadlock the daemon.
			select {
			case p.StatusChanged <- status:
			default:
				logger.Warn("StatusChanged full; dropping disconnected event")
			}
			logger.Warn("proxy: " + reason + "; tearing down TUN state")
			p.notifyUnexpectedExit(reason)
		}
	}
}

// isHysteriaProtocol reports whether the profile uses the hysteria2 transport.
// Xray runs it natively, but it is UDP, so the tester must skip the TCP probe.
func isHysteriaProtocol(protocol string) bool {
	return protocol == "hysteria2" || protocol == "hy2"
}

func (p *ProxyManager) Connect(profile structs.Profile, tunName string, tunEnabled bool) error {
	p.mu.Lock()
	defer p.mu.Unlock()

	p.tunEnabled = tunEnabled
	p.tunActive.Store(tunEnabled)
	p.tunName = tunName

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

	tunArg := ""
	if tunEnabled {
		tunArg = tunName
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

	var xray_config []byte
	var profileIPs []string
	var selection structs.RoutingSelection

	if profile.Managed && p.DB != nil {
		// JSON subscription: the provider config is the routing baseline and
		// manual rules are prepended by xrayjson.Prepare.
		ips, hosts, err := p.managedSource(profile)
		if err != nil {
			if p.portPool != nil && apiPort != 0 {
				p.portPool.ReleasePort(apiPort)
			}
			return err
		}
		profileIPs = ips
		manual, _, _ := p.DB.LoadEffectiveManualRouting(profile.GroupId)
		geoDir, _ := p.DB.ResolveGroupGeoDir(profile.GroupId)

		selection = structs.RoutingSelection{
			Config:         manual,
			Source:         "manual",
			GeoDir:         geoDir,
			BootstrapHosts: hosts,
		}
		p.bootstrapHosts = hosts

		cfg, _, err := p.prepareManagedConfig(profile, app_config.SocksPort, app_config.HttpPort, tunArg, apiPort, geoDir, manual, hosts)
		if err != nil {
			if p.portPool != nil && apiPort != 0 {
				p.portPool.ReleasePort(apiPort)
			}
			return err
		}
		xray_config = cfg
		logger.Infof("routing: provider JSON (%d outbounds, %d manual rules, source=manual)",
			countOutbounds(xray_config), len(manual.Rules))
	} else {
		var err error
		xray_config, err = lib.ParseUriWithTun(profile.Uri, app_config.SocksPort, app_config.HttpPort, tunArg)
		if err != nil {
			if p.portPool != nil && apiPort != 0 {
				p.portPool.ReleasePort(apiPort)
			}
			return err
		}

		// Inject stats tracking config (policy + stats counters, optional
		// dokodemo-door API inbound when apiPort > 0).
		xray_config, err = injectStatsConfig(xray_config, apiPort)
		if err != nil {
			logger.Warnf("failed to inject stats config: %v", err)
		}

		// Resolve the VPN server address before Xray starts: once TUN is up the
		// system resolver points at the tunnel and the server domain must already
		// be available as a DNS bootstrap entry.
		profileIPs = resolveProfileIPs(profile.Address, app_config.DnsServers)

		if p.DB != nil {
			sel, selErr := p.DB.ResolveRouting(profile.GroupId)
			if selErr != nil {
				logger.Warnf("failed to resolve routing for group %d: %v", profile.GroupId, selErr)
				selection = structs.RoutingSelection{Source: "manual"}
			} else {
				selection = sel
			}
			selection.BootstrapHosts = bootstrapHosts(profile.Address, profileIPs)

			prevOutboundsCount := countOutbounds(xray_config)
			xray_config, err = injectRoutingConfig(xray_config, selection, tunEnabled)
			if err != nil {
				logger.Warnf("failed to inject routing config: %v", err)
			}
			afterOutboundsCount := countOutbounds(xray_config)
			logger.Infof("routing: outbounds %d → %d (source=%s)", prevOutboundsCount, afterOutboundsCount, selection.Source)
		}
	}

	p.profileIPs = profileIPs

	// Pre-flight: validate the generated config before spawning xray so a bad
	// profile/routing/DNS config surfaces as an actionable connect error instead
	// of a silent immediate disconnect. The TUN inbound is omitted from the test
	// config (see buildPreflightConfig).
	var rawConfig json.RawMessage
	if profile.Managed && p.DB != nil {
		rawConfig, _ = p.DB.LoadProfileXrayConfig(profile.GroupId, profile.Id)
	}
	if preflightCfg, buildErr := buildPreflightConfig(profile, app_config.SocksPort, app_config.HttpPort, apiPort, selection, p.DB != nil, tunEnabled, rawConfig); buildErr != nil {
		logger.Warnf("preflight: could not build test config, skipping validation: %v", buildErr)
	} else if testErr := validateXrayConfig(preflightCfg, selection.GeoDir); testErr != nil {
		// The stats port was reserved above but never handed to a running core.
		if p.portPool != nil && apiPort != 0 {
			p.portPool.ReleasePort(apiPort)
		}
		return testErr
	}

	// Install the enabled kill-switch allowlist with the exact IPs Xray is
	// about to use, before it starts. The block is never removed around the
	// connect, so there is no unprotected window. The handler also reads the
	// direct-rule exceptions, so publish them first.
	p.directTargets.set(directTargetsFromRules(selection.Config.Rules))
	if err := p.runPreStartHandler(profileIPs); err != nil {
		if p.portPool != nil && apiPort != 0 {
			p.portPool.ReleasePort(apiPort)
		}
		return err
	}

	p.core = newCore()
	if err := p.core.Start(xray_config, selection.GeoDir); err != nil {
		return err
	}

	p.proxyIPs = profileIPs

	logger.Infof("connected to %s (%s://%s)",
		profile.Name, profile.Protocol, profile.Address)

	// Retire the previous sampler (closing its gRPC client) before releasing
	// the StatsService port it was using.
	p.stopMeter()
	if p.statsApiPort != 0 && p.portPool != nil {
		p.portPool.ReleasePort(p.statsApiPort)
	}
	p.statsApiPort = apiPort
	p.startMeter(p.newTrafficSource(apiPort, xray_config, tunEnabled))

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

// SetTunMode updates the TUN mode status and hot-reloads Xray if currently connected.
func (p *ProxyManager) SetTunMode(tunName string, enabled bool) error {
	p.mu.Lock()
	defer p.mu.Unlock()

	p.tunEnabled = enabled
	p.tunActive.Store(enabled)
	p.tunName = tunName

	if p.status.Connection != "connected" || p.core == nil || !p.core.IsRunning() {
		return nil
	}

	return p.reloadCoreLocked(tunName)
}

func (p *ProxyManager) Stop() {
	p.mu.Lock()
	defer p.mu.Unlock()
	logger.Info("disconnecting")
	p.tunEnabled = false
	p.tunActive.Store(false)
	// Retire the Exited-watcher goroutine and the traffic sampler. The meter
	// publishes one final frame with connected=false.
	p.retireExitWatcher()
	p.stopMeter()
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
}

func (p *ProxyManager) GetStatus() structs.ProxyStatus {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.status
}

func (p *ProxyManager) UpdateMovedProfile(oldId structs.ProfileID, newProfile structs.Profile) {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.status.Connection == "connected" && p.status.Profile.Id == oldId.Id && p.status.Profile.GroupId == oldId.GroupId {
		p.status.Profile = newProfile
		select {
		case p.StatusChanged <- p.status:
		default:
		}
	}
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

// resolveProfileIPs resolves the VPN server through the configured DNS servers
// first: with the kill-switch armed, the system resolver's upstream may not be
// on the allowlist, while cfg.DnsServers always are.
func resolveProfileIPs(address string, dnsServers []string) []string {
	resolved, err := utils.ResolveDomain(address, dnsServers)
	if err != nil {
		return nil
	}
	return append(append([]string{}, resolved.IPv4...), resolved.IPv6...)
}

// bootstrapHosts returns a dns.hosts entry for a VPN server addressed by name.
// Literal IPs need no bootstrap, and failed resolutions are simply skipped.
func bootstrapHosts(address string, ips []string) map[string][]string {
	address = strings.TrimSpace(address)
	if address == "" || net.ParseIP(address) != nil || len(ips) == 0 {
		return nil
	}
	return map[string][]string{address: ips}
}
