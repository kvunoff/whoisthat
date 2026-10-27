package tunmode

import (
	"errors"
	"fmt"
	"sync"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	"whoisthat-core/utils"
)

type TunModeManager struct {
	mu                     sync.Mutex
	tun_name               string
	tun_ip                 string
	tun_ipv6               string
	default_interface      string
	default_interface_ip   string
	default_interface_ipv6 string
	proxy_ipv4s            []string
	proxy_ipv6s            []string
	dnsHijack              DnsHijack
	sudoUid                int
	StatusChanged          chan bool
	IsEnabled              bool
}

func (t *TunModeManager) Init() {
	t.tun_ip = "198.18.0.1"
	t.tun_ipv6 = "fd00::1"
	t.StatusChanged = make(chan bool)
}

func (t *TunModeManager) IsEnabledLocked() bool {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.IsEnabled
}

func (t *TunModeManager) Start(proxy_ipv4s []string, proxy_ipv6s []string, dns DnsHijack) error {
	logger.Info("tun: starting...")

	// Tear down the previous session before replacing the fields its cleanup
	// depends on (old TUN name, DNS routes, proxy IPs).
	if err := t.clearNetworkRules(); err != nil {
		return fmt.Errorf("there was an error clearing network rules %w", err)
	}

	t.tun_name = appconfig.GetConfig().TunName
	interface_name, interface_ip, interface_ipv6, err := GetDefaultInterfaceAndIP()
	if err != nil {
		return fmt.Errorf("failed on getting default interface %w", err)
	}

	t.default_interface_ip = interface_ip
	t.default_interface_ipv6 = interface_ipv6
	t.default_interface = interface_name
	t.proxy_ipv4s = proxy_ipv4s
	t.proxy_ipv6s = proxy_ipv6s

	dns.Exclude = append(append([]string{}, dns.Exclude...), proxy_ipv4s...)
	dns.Exclude = append(dns.Exclude, proxy_ipv6s...)
	t.dnsHijack = sanitizeDnsHijack(dns)
	t.sudoUid = utils.DedicatedUid()

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

	// The legacy (manual) path keeps the configured resolver reachable through
	// the physical interface. The Happ path uses the DNS sink inside TUN and
	// must not install these routes.
	if t.dnsHijack.Legacy {
		// Delete first: a previous crash may have left the same routes behind.
		_ = deleteDnsIpRoutes(t.dnsHijack.Sink4, t.default_interface_ip)
		_ = deleteDnsIpRoutes6(t.dnsHijack.Sink6, t.default_interface_ipv6)
		err = setupDnsIpRoutes(t.dnsHijack.Sink4, t.dnsHijack.Sink6, t.default_interface_ip, t.default_interface_ipv6)
		if err != nil {
			t.clearNetworkRules()
			return fmt.Errorf("there was an error setting up dns ip route %w", err)
		}
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

	err = setupDnsHijackRules(t.default_interface, t.dnsHijack)
	if err != nil {
		t.clearNetworkRules()
		return fmt.Errorf("there was an error setting up dns hijack rules %w", err)
	}
	if t.dnsHijack.Enabled {
		if err2 := setupResolvectlDns(t.tun_name, t.dnsHijack.Sink4); err2 != nil {
			logger.Warnf("tun: resolvectl dns setup: %v", err2)
		}
	}

	// In "include" split mode ONLY the chosen apps use the tunnel, so we must
	// NOT install a system-wide default route via the TUN — that route lives in
	// the dedicated table 200 keyed by fwmark instead (see applySplitRules).
	splitMode := appconfig.GetConfig().SplitTunnel.Mode
	if splitMode != "include" {
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
	} else {
		logger.Info("tun: include split mode — skipping system-wide TUN default route")
	}

	if err := t.applySplitRules(); err != nil {
		t.clearNetworkRules()
		return fmt.Errorf("there was an error applying split tunnel rules %w", err)
	}

	t.IsEnabled = true
	t.StatusChanged <- t.IsEnabled
	logger.Info("tun: network rules enabled")

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
	t.dnsHijack = DnsHijack{}
	t.IsEnabled = false
	t.StatusChanged <- t.IsEnabled
}

// ReapplyDnsHijack reconciles the port-53 hijack with the current split mode
// while TUN is running. The legacy (manual) hijack is independent of the split
// mode and is left untouched.
func (t *TunModeManager) ReapplyDnsHijack() error {
	t.mu.Lock()
	defer t.mu.Unlock()
	if !t.IsEnabled || !t.dnsHijack.Happ {
		return nil
	}

	want := appconfig.GetConfig().SplitTunnel.Mode != "include"
	if want == t.dnsHijack.Enabled {
		return nil
	}

	if want {
		plan := t.dnsHijack
		plan.Enabled = true
		if err := setupDnsHijackRules(t.default_interface, plan); err != nil {
			return err
		}
		if err := setupResolvectlDns(t.tun_name, plan.Sink4); err != nil {
			logger.Warnf("tun: resolvectl dns setup: %v", err)
		}
	} else {
		if err := cleanDnsHijackRules(); err != nil {
			return err
		}
		if err := revertResolvectlDns(t.tun_name); err != nil {
			logger.Warnf("tun: resolvectl dns revert: %v", err)
		}
	}

	t.dnsHijack.Enabled = want
	return nil
}

func (t *TunModeManager) clearNetworkRules() error {
	errs := []error{
		deleteTunIpRoute(t.tun_name, t.tun_ip),
		deleteTunIpRoute6(t.tun_name, t.tun_ipv6),
		deleteTun(t.tun_name),
		cleanDnsHijackRules(),
		deleteProxyIpRoutes(t.proxy_ipv4s, t.proxy_ipv6s, t.default_interface_ip, t.default_interface_ipv6),
		removeFwmarkRouting(1, t.default_interface, t.default_interface_ip),
		removeFwmarkRouting6(1, t.default_interface, t.default_interface_ipv6),
		cleanConntrackRules(t.tun_name, t.default_interface),
		t.removeSplitRules(),
		revertResolvectlDns(t.tun_name),
	}
	if t.dnsHijack.Legacy {
		errs = append(errs,
			deleteDnsIpRoutes(t.dnsHijack.Sink4, t.default_interface_ip),
			deleteDnsIpRoutes6(t.dnsHijack.Sink6, t.default_interface_ipv6),
		)
	}
	if t.sudoUid > 0 {
		errs = append(errs, removeUidRouting(t.sudoUid, t.default_interface, t.default_interface_ip))
		errs = append(errs, removeUidRouting6(t.sudoUid, t.default_interface, t.default_interface_ipv6))
	}
	return errors.Join(errs...)
}
