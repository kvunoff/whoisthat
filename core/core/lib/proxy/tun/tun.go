package tunmode

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	"whoisthat-core/utils"
	"errors"
	"fmt"
	"sync"
)

type TunModeManager struct {
	mu                   sync.Mutex
	tun2socks            Tun2Socks
	tun_name             string
	tun_ip               string
	default_interface    string
	default_interface_ip string
	proxy_ipv4s          []string
	dns                  string
	sudoUid              int
	StatusChanged        chan bool
	IsEnabled            bool
}

func (t *TunModeManager) Init() {
	t.tun_name = "whoisthattun"
	t.tun_ip = "198.18.0.1"
	t.StatusChanged = make(chan bool)
	t.tun2socks = Tun2Socks{
		Exited: make(chan error),
	}
}

func (t *TunModeManager) Start(proxy_ipv4s []string, dns string) error {
	logger.Info("tun: starting...")
	interface_name, interface_ip, err := GetDefaultInterfaceAndIP()
	if err != nil {
		return fmt.Errorf("failed on getting default interafce %w", err)
	}

	t.default_interface_ip = interface_ip
	t.default_interface = interface_name
	t.proxy_ipv4s = proxy_ipv4s
	t.dns = dns
	t.sudoUid = utils.DedicatedUid()

	err = t.clearNetworkRules()
	if err != nil {
		return fmt.Errorf("there was an error clearing network rules %w", err)
	}

	err = createTun(t.tun_name, t.tun_ip)
	if err != nil {
		t.clearNetworkRules()
		return fmt.Errorf("there was an error creating tun interface %w", err)
	}
	logger.Infof("tun: interface %s created (%s)", t.tun_name, t.tun_ip)

	err = setupProxyIpRoutes(t.proxy_ipv4s, t.default_interface_ip)
	if err != nil {
		t.clearNetworkRules()
		return fmt.Errorf("there was an error setting up proxy ip routes %w", err)
	}

	err = setupDnsIpRoutes(t.dns, t.default_interface_ip)
	if err != nil {
		t.clearNetworkRules()
		return fmt.Errorf("there was an error setting up dns ip route %w", err)
	}

	err = loosenRpFilter(t.tun_name, t.default_interface)
	if err != nil {
		t.clearNetworkRules()
		return fmt.Errorf("there was an error setting up dns ip route %w", err)
	}

	if t.sudoUid > 0 {
		err = addUidRouting(t.sudoUid, interface_name, interface_ip)
		if err != nil {
			logger.Warnf("uid routing unavailable (direct rules in TUN mode may not work): %v", err)
		} else {
			logger.Infof("tun: uid routing added for uid=%d", t.sudoUid)
		}
	}

	err = setupDnsHijackRules(t.default_interface, t.dns)
	if err != nil {
		return fmt.Errorf("there was an error setting up dns hijack rules %w", err)
	}

	err = setupTunIpRoute(t.tun_name, t.tun_ip)
	if err != nil {
		t.clearNetworkRules()
		return fmt.Errorf("there was an error setting up tun ip route %w", err)
	}

	if t.tun2socks.IsRunning() {
		t.tun2socks.Stop()
	}

	t.tun2socks = Tun2Socks{
		Exited: make(chan error),
	}

	if t.IsEnabled {
		t.IsEnabled = false
		t.StatusChanged <- t.IsEnabled
	}

	if err := t.tun2socks.Start(t.tun_name, appconfig.GetConfig().SocksPort); err != nil {
		return err
	}
	logger.Info("tun: tun2socks started")

	t.IsEnabled = true
	t.StatusChanged <- t.IsEnabled
	logger.Info("tun: enabled")

	go func() {
		for {
			_, ok := <-t.tun2socks.Exited
			if !ok {
				return
			}
			t.mu.Lock()
			t.IsEnabled = false
			t.StatusChanged <- t.IsEnabled
			t.mu.Unlock()
		}
	}()

	return nil
}

func (t *TunModeManager) Stop() {
	t.mu.Lock()
	defer t.mu.Unlock()
	if !t.IsEnabled {
		return
	}
	logger.Info("tun: disabling...")
	t.clearNetworkRules()
	t.tun2socks.Stop()
	t.IsEnabled = false
	t.StatusChanged <- t.IsEnabled
}

func (t *TunModeManager) clearNetworkRules() error {
	errs := []error{
		deleteTunIpRoute(t.tun_name, t.tun_ip),
		deleteTun(t.tun_name),
		deleteDnsIpRoutes(t.dns, t.default_interface_ip),
		cleanDnsHijackRules(t.default_interface, t.dns),
		deleteProxyIpRoutes(t.proxy_ipv4s, t.default_interface_ip),
	}
	if t.sudoUid > 0 {
		errs = append(errs, removeUidRouting(t.sudoUid, t.default_interface, t.default_interface_ip))
	}
	return errors.Join(errs...)
}
