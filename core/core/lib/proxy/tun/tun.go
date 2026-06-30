package tunmode

import (
	"errors"
	"fmt"
	"os/exec"
	"sync"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	"whoisthat-core/utils"
)

type TunModeManager struct {
	mu                     sync.Mutex
	tun2socks              Tun2Socks
	tun_name               string
	tun_ip                 string
	tun_ipv6               string
	default_interface      string
	default_interface_ip   string
	default_interface_ipv6 string
	proxy_ipv4s            []string
	proxy_ipv6s            []string
	dns                    string
	dns_ipv6               string
	sudoUid                int
	StatusChanged          chan bool
	IsEnabled              bool
}

func (t *TunModeManager) Init() {
	t.tun_ip = "198.18.0.1"
	t.tun_ipv6 = "fd00::1"
	t.StatusChanged = make(chan bool)
	t.tun2socks = Tun2Socks{
		Exited: make(chan error),
	}
}

func (t *TunModeManager) IsEnabledLocked() bool {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.IsEnabled
}

func (t *TunModeManager) Start(proxy_ipv4s []string, proxy_ipv6s []string, dns string, dns_ipv6 string) error {
	t.tun_name = appconfig.GetConfig().TunName
	logger.Info("tun: starting...")
	interface_name, interface_ip, interface_ipv6, err := GetDefaultInterfaceAndIP()
	if err != nil {
		return fmt.Errorf("failed on getting default interface %w", err)
	}

	t.default_interface_ip = interface_ip
	t.default_interface_ipv6 = interface_ipv6
	t.default_interface = interface_name
	t.proxy_ipv4s = proxy_ipv4s
	t.proxy_ipv6s = proxy_ipv6s
	t.dns = dns
	t.dns_ipv6 = dns_ipv6
	t.sudoUid = utils.DedicatedUid()

	err = t.clearNetworkRules()
	if err != nil {
		return fmt.Errorf("there was an error clearing network rules %w", err)
	}

	err = createTun(t.tun_name, t.tun_ip, t.tun_ipv6)
	if err != nil {
		t.clearNetworkRules()
		return fmt.Errorf("there was an error creating tun interface %w", err)
	}
	logger.Infof("tun: interface %s created (%s, %s)", t.tun_name, t.tun_ip, t.tun_ipv6)

	err = setupProxyIpRoutes(t.proxy_ipv4s, t.proxy_ipv6s, t.default_interface_ip, t.default_interface_ipv6)
	if err != nil {
		t.clearNetworkRules()
		return fmt.Errorf("there was an error setting up proxy ip routes %w", err)
	}

	err = setupDnsIpRoutes(t.dns, t.dns_ipv6, t.default_interface_ip, t.default_interface_ipv6)
	if err != nil {
		t.clearNetworkRules()
		return fmt.Errorf("there was an error setting up dns ip route %w", err)
	}

	err = loosenRpFilter(t.tun_name, t.default_interface)
	if err != nil {
		t.clearNetworkRules()
		return fmt.Errorf("there was an error loosening rp filter %w", err)
	}

	err = addFwmarkRouting(1, interface_name, interface_ip)
	if err != nil {
		t.clearNetworkRules()
		return fmt.Errorf("there was an error adding fwmark routing: %w", err)
	}
	if t.default_interface_ipv6 != "" {
		err = addFwmarkRouting6(1, interface_name, t.default_interface_ipv6)
		if err != nil {
			t.clearNetworkRules()
			return fmt.Errorf("there was an error adding ipv6 fwmark routing: %w", err)
		}
	}

	err = setupConntrackRules(t.tun_name, t.default_interface)
	if err != nil {
		t.clearNetworkRules()
		return fmt.Errorf("there was an error setting up conntrack bypass rules: %w", err)
	}

	if t.sudoUid > 0 {
		err = addUidRouting(t.sudoUid, interface_name, interface_ip)
		if err != nil {
			logger.Warnf("uid routing unavailable (direct rules in TUN mode may not work): %v", err)
		} else {
			logger.Infof("tun: uid routing added for uid=%d", t.sudoUid)
		}
		if t.default_interface_ipv6 != "" {
			if err2 := addUidRouting6(t.sudoUid, interface_name, t.default_interface_ipv6); err2 != nil {
				logger.Warnf("tun: ipv6 uid routing failed: %v", err2)
			}
		}
	}

	err = setupDnsHijackRules(t.default_interface, t.dns)
	if err != nil {
		t.clearNetworkRules()
		return fmt.Errorf("there was an error setting up dns hijack rules %w", err)
	}
	if t.dns_ipv6 != "" {
		if err2 := setupDnsHijackRules6(t.default_interface, t.dns_ipv6); err2 != nil {
			logger.Warnf("tun: ipv6 dns hijack failed: %v", err2)
		}
	}

	err = setupTunIpRoute(t.tun_name, t.tun_ip)
	if err != nil {
		t.clearNetworkRules()
		return fmt.Errorf("there was an error setting up tun ip route %w", err)
	}
	if t.tun_ipv6 != "" {
		if err2 := setupTunIpRoute6(t.tun_name, t.tun_ipv6); err2 != nil {
			t.clearNetworkRules()
			return fmt.Errorf("there was an error setting up tun ipv6 route %w", err2)
		}
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
	// Kill any leftover tun2socks that might be holding the TUN interface
	exec.Command("pkill", "-9", "tun2socks").Run()

	errs := []error{
		deleteTunIpRoute(t.tun_name, t.tun_ip),
		deleteTunIpRoute6(t.tun_name, t.tun_ipv6),
		deleteTun(t.tun_name),
		deleteDnsIpRoutes(t.dns, t.default_interface_ip),
		deleteDnsIpRoutes6(t.dns_ipv6, t.default_interface_ipv6),
		cleanDnsHijackRules(t.default_interface, t.dns),
		cleanDnsHijackRules6(t.default_interface, t.dns_ipv6),
		deleteProxyIpRoutes(t.proxy_ipv4s, t.proxy_ipv6s, t.default_interface_ip, t.default_interface_ipv6),
		removeFwmarkRouting(1, t.default_interface, t.default_interface_ip),
		removeFwmarkRouting6(1, t.default_interface, t.default_interface_ipv6),
		cleanConntrackRules(t.tun_name, t.default_interface),
	}
	if t.sudoUid > 0 {
		errs = append(errs, removeUidRouting(t.sudoUid, t.default_interface, t.default_interface_ip))
		errs = append(errs, removeUidRouting6(t.sudoUid, t.default_interface, t.default_interface_ipv6))
	}
	return errors.Join(errs...)
}
