package cmd

import (
	"errors"
	"fmt"
	"net"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	"whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

func (cmd *Cmd) DisableTun(data structs.DisableTunData, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) (err error) {
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()
	defer func() { cmd.finishOperation("disable-tun", err, proxy_manager, tun_manager) }()

	if proxy_manager != nil && proxy_manager.TunEnabled() {
		if err := proxy_manager.SetTunMode(appconfig.GetConfig().TunName, false); err != nil {
			cmd.warn("disable-tun-failed", err.Error())
			if proxy_manager.GetStatus().Connection == "connected" {
				return err
			}
			return errors.Join(err, tun_manager.Stop())
		}
	}
	return tun_manager.Stop()
}

func (cmd *Cmd) EnableTun(data structs.EnableTunData, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) (err error) {
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()
	defer func() { cmd.finishOperation("enable-tun", err, proxy_manager, tun_manager) }()

	status := proxy_manager.GetStatus()
	if status.Connection != "connected" {
		return cmd.tunFailure(errors.New("a profile must be connected for TUN mode to operate"))
	}
	if tun_manager.IsEnabledLocked() {
		return cmd.tunFailure(errors.New("TUN mode is already enabled"))
	}

	return cmd.enableTun(status.Profile, proxy_manager, tun_manager)
}

func (cmd *Cmd) EnableTunForProfile(profile structs.Profile, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) {
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()
	if tun_manager.IsEnabledLocked() {
		return
	}
	cmd.enableTun(profile, proxy_manager, tun_manager)
}

func pickDns(dnsServers []string) (dns4, dns6 string) {
	for _, s := range dnsServers {
		ip := net.ParseIP(s)
		if ip == nil {
			continue
		}
		if ip.To4() != nil {
			if dns4 == "" {
				dns4 = s
			}
		} else {
			if dns6 == "" {
				dns6 = s
			}
		}
		if dns4 != "" && dns6 != "" {
			break
		}
	}
	if dns4 == "" {
		dns4 = "1.1.1.1"
	}
	if dns6 == "" {
		dns6 = "2606:4700:4700::1111"
	}
	return
}

// dnsHijackFor selects the port-53 redirect strategy for the active profile.
// Manual routing (and Happ profiles without DNS settings) keep the legacy
// behaviour: DNAT to the configured resolver through the physical interface.
// Happ profiles with DNS use the reserved sink inside TUN, except in include
// split mode where the hijack must stay disabled.
func (cmd *Cmd) dnsHijackFor(profile structs.Profile) tunmode.DnsHijack {
	dns4, dns6 := pickDns(appconfig.GetConfig().DnsServers)
	legacy := tunmode.DnsHijack{
		Enabled: true,
		Legacy:  true,
		Sink4:   dns4,
		Sink6:   dns6,
	}

	// JSON-subscription profiles always carry (or receive) an Xray DNS
	// section, so port 53 must reach the tunnel's dns-out like a Happ profile.
	if profile.Managed {
		return tunmode.DnsHijack{
			Enabled:   appconfig.GetConfig().SplitTunnel.Mode != "include",
			Happ:      true,
			Sink4:     tunmode.DNSSink4,
			Sink6:     tunmode.DNSSink6,
			ScopeMark: 1,
		}
	}

	if cmd.DB == nil {
		return legacy
	}
	selection, err := cmd.DB.ResolveRouting(profile.GroupId)
	if err != nil {
		logger.Warnf("tun: failed to resolve routing for dns hijack: %v", err)
		return legacy
	}
	if selection.Source != "happ" || selection.Config.DNS == nil {
		return legacy
	}

	return tunmode.DnsHijack{
		Enabled:   appconfig.GetConfig().SplitTunnel.Mode != "include",
		Happ:      true,
		Sink4:     tunmode.DNSSink4,
		Sink6:     tunmode.DNSSink6,
		ScopeMark: 1,
	}
}

// refreshDnsHijackFor re-evaluates the kernel port-53 plan after routing
// configuration changes while TUN is already running.
func (cmd *Cmd) refreshDnsHijackFor(profile structs.Profile) {
	if cmd.TunManager == nil || !cmd.TunManager.IsEnabledLocked() {
		return
	}
	if err := cmd.TunManager.SetDnsHijack(cmd.dnsHijackFor(profile)); err != nil {
		logger.Warnf("tun: failed to refresh dns hijack: %v", err)
	}
}

func (cmd *Cmd) enableTun(profile structs.Profile, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) error {
	if !utils.HasNetAdmin() {
		return cmd.tunFailure(errors.New("TUN unavailable: check network capabilities and /dev/net/tun"))
	}
	var resolved *utils.ResolvedIPs
	var err error
	if proxy_manager != nil && len(proxy_manager.GetProxyIPs()) > 0 {
		// Reuse the exact IPs Connect resolved and the kill-switch allowlist
		// was built from: re-resolving here could produce a different address
		// under round-robin, leaving the TUN route or the allowlist stale.
		resolved = &utils.ResolvedIPs{}
		for _, raw := range proxy_manager.GetProxyIPs() {
			ip := net.ParseIP(raw)
			if ip == nil {
				continue
			}
			if ip.To4() != nil {
				resolved.IPv4 = append(resolved.IPv4, ip.String())
			} else {
				resolved.IPv6 = append(resolved.IPv6, ip.String())
			}
		}
	} else {
		resolved, err = resolveHostAndAddress(profile, appconfig.GetConfig().DnsServers)
	}
	if err != nil {
		logger.Warn("tun: failed to resolve:", err)
		return cmd.tunFailure(fmt.Errorf("failed to resolve profile host: %w", err))
	}
	if len(resolved.IPv4) == 0 && len(resolved.IPv6) == 0 {
		logger.Warn("tun: failed to resolve: no proxy server IPs")
		return cmd.tunFailure(errors.New("failed to resolve proxy server IPs"))
	}

	logger.Infof("tun: resolved %d IPv4, %d IPv6", len(resolved.IPv4), len(resolved.IPv6))

	err = tun_manager.StartWith(resolved.IPv4, resolved.IPv6, cmd.dnsHijackFor(profile), func() error {
		return proxy_manager.SetTunMode(appconfig.GetConfig().TunName, true)
	})
	if err != nil {
		return cmd.tunFailure(err)
	}
	return nil
}

func (cmd *Cmd) tunFailure(err error) error {
	cmd.warn("enable-tun-failed", err.Error())
	return err
}

func (cmd *Cmd) SetTunName(data structs.SetTunNameData) {
	if err := appconfig.SetTunName(data.TunName); err != nil {
		cmd.warn("set-tun-name-failed", err.Error())
		return
	}
	logger.Infof("tun name set to %q", data.TunName)
	cmd.send("tun-name-updated", structs.SetTunNameData{TunName: data.TunName})
}

func resolveHostAndAddress(profile structs.Profile, dnsServers []string) (*utils.ResolvedIPs, error) {
	result := &utils.ResolvedIPs{}
	var errs []error

	resolve := func(domain string) {
		if domain == "" {
			return
		}
		r, err := utils.ResolveDomain(domain, dnsServers)
		if err != nil {
			errs = append(errs, err)
			return
		}
		result.IPv4 = append(result.IPv4, r.IPv4...)
		result.IPv6 = append(result.IPv6, r.IPv6...)
	}

	resolve(profile.Host)
	resolve(profile.Address)

	if len(result.IPv4) == 0 && len(result.IPv6) == 0 {
		return result, fmt.Errorf("failed to resolve any IPs: %w", errors.Join(errs...))
	}
	result.IPv4 = utils.RemoveDuplicates(result.IPv4)
	result.IPv6 = utils.RemoveDuplicates(result.IPv6)
	return result, nil
}
