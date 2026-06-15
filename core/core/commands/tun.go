package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
	"errors"
	"fmt"
	"net"
)

func (cmd *Cmd) DisableTun(data structs.DisableTunData, tun_manager *tunmode.TunModeManager) {
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()

	tun_manager.Stop()
}

func (cmd *Cmd) EnableTun(data structs.EnableTunData, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) {
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()

	status := proxy_manager.GetStatus()
	if status.Connection != "connected" {
		cmd.warn("enable-tun-failed", "A profile must be connected for tun mode to operate")
		return
	}
	if tun_manager.IsEnabledLocked() {
		cmd.warn("enable-tun-failed", "tun mode is already enabled")
		return
	}

	cmd.enableTun(status.Profile, tun_manager)
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

func (cmd *Cmd) enableTun(profile structs.Profile, tun_manager *tunmode.TunModeManager) {
	resolved, err := resolveHostAndAddress(profile, appconfig.GetConfig().DnsServers)
	if err != nil {
		logger.Warn("tun: failed to resolve:", err)
		cmd.warn("enable-tun-failed", "failed to resolve profile host")
		return
	}

	logger.Infof("tun: resolved %d IPv4, %d IPv6", len(resolved.IPv4), len(resolved.IPv6))

	dns4, dns6 := pickDns(appconfig.GetConfig().DnsServers)
	err = tun_manager.Start(resolved.IPv4, resolved.IPv6, dns4, dns6)
	if err != nil {
		logger.Warn("tun: failed to start:", err)
		cmd.warn("enable-tun-failed", "Failed to enable tun mode")
		return
	}
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
