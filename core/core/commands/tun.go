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

func (cmd *Cmd) enableTun(profile structs.Profile, tun_manager *tunmode.TunModeManager) {
	resolved, err := resolveHostAndAddress(profile, appconfig.GetConfig().DnsServers)
	if err != nil {
		logger.Warn("tun: failed to resolve:", err)
		cmd.warn("enable-tun-failed", "failed to resolve profile host")
		return
	}

	logger.Info("tun: resolved", resolved)

	dns := "1.1.1.1"
	if len(appconfig.GetConfig().DnsServers) > 0 {
		dns = appconfig.GetConfig().DnsServers[0]
	}
	err = tun_manager.Start(resolved, dns)
	if err != nil {
		logger.Warn("tun: failed to start:", err)
		cmd.warn("enable-tun-failed", "Failed to enable tun mode")
		return
	}
}

func resolveHostAndAddress(profile structs.Profile, dnsServers []string) ([]string, error) {
	var ipv4s []string
	var errs []error
	if profile.Host != "" {
		resolved, err := utils.ResolveDomainIpv4(profile.Host, dnsServers)
		if err == nil {
			ipv4s = append(ipv4s, resolved...)
		} else {
			errs = append(errs, err)
		}
	}
	if profile.Address != "" {
		resolved, err := utils.ResolveDomainIpv4(profile.Address, dnsServers)
		if err == nil {
			ipv4s = append(ipv4s, resolved...)
		} else {
			errs = append(errs, err)
		}
	}
	if len(ipv4s) == 0 {
		return ipv4s, fmt.Errorf("failed to resolve any ipv4s: %w", errors.Join(errs...))
	}
	unique_ips := utils.RemoveDuplicates(ipv4s)
	return unique_ips, nil
}
