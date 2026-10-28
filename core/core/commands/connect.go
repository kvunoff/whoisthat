package cmd

import (
	"fmt"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
)

func (cmd *Cmd) Disconnect(data structs.DisconnectData, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) {
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()

	killSwitchEnabled := proxy_manager.KillSwitchEnabled()
	proxyIPs := proxy_manager.GetProxyIPs()

	proxy_manager.Stop()
	tun_manager.Stop()

	// Fail closed: with no connection the machine must not leak, so the block is
	// armed even when the server IP list is empty (LAN/DNS/TUN stay allowed).
	if killSwitchEnabled {
		cfg := appconfig.GetConfig()
		if err := tunmode.SetupKillSwitchBlock(proxyIPs, cfg.TunName, cfg.DnsServers); err != nil {
			logger.Warn("kill-switch: failed to apply block on disconnect:", err)
		} else {
			logger.Info("kill-switch: block applied after disconnect")
		}
	}
}

func (cmd *Cmd) Connect(data structs.ConnectData, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) {
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()

	profile, err := cmd.DB.GetProfile(data.Profile.GroupId, data.Profile.Id)
	if err != nil {
		logger.Warn("connect: failed to get profile:", err)
		cmd.warn("connect-failed", fmt.Sprintf("profile lookup failed: %v", err))
		return
	}
	was_tun_enabled := tun_manager.IsEnabledLocked()

	// 1. If switching while connected or if TUN was enabled, stop the active proxy
	// process first so it releases /dev/net/tun before we clean up network rules and delete the device.
	if proxy_manager != nil {
		proxy_manager.Stop()
	}
	if was_tun_enabled {
		tun_manager.Stop()
	}

	killSwitchEnabled := appconfig.GetConfig().KillSwitchEnabled
	tunName := appconfig.GetConfig().TunName

	if killSwitchEnabled {
		if err := tunmode.RemoveKillSwitchBlock(); err != nil {
			logger.Warn("kill-switch: failed to remove old block:", err)
		}
	}

	// Connect the new profile in base proxy mode first (tunEnabled = false).
	// If TUN was previously active, cmd.enableTun below will create the TUN device
	// and routing rules first, then reload the proxy with TUN inbound. This guarantees
	// Xray doesn't race against tun_manager.Start over creating/opening /dev/net/tun.
	if err := proxy_manager.Connect(profile, tunName, false); err != nil {
		logger.Warn("connect failed:", err)
		cmd.warn("connect-failed", err.Error())

		// The old connection was already stopped and the kill-switch block was
		// removed above. Re-arm it with the last known server IPs so a failed
		// switch does not silently leave the machine unprotected. With no known
		// IPs the block is still fail-closed (only loopback/LAN/DNS allowed).
		if killSwitchEnabled {
			proxyIPs := proxy_manager.GetProxyIPs()
			if ksErr := tunmode.SetupKillSwitchBlock(proxyIPs, tunName, appconfig.GetConfig().DnsServers); ksErr != nil {
				logger.Warn("kill-switch: failed to re-arm after connect failure:", ksErr)
			} else {
				logger.Infof("kill-switch: re-armed after connect failure, whitelisted IPs: %v", proxyIPs)
			}
		}
		return
	}

	if killSwitchEnabled {
		proxyIPs := proxy_manager.GetProxyIPs()
		if len(proxyIPs) > 0 {
			if err := tunmode.SetupKillSwitchBlock(proxyIPs, tunName, appconfig.GetConfig().DnsServers); err != nil {
				logger.Warn("kill-switch: failed to apply block:", err)
			} else {
				logger.Info("kill-switch: block applied, whitelisted IPs:", proxyIPs)
			}
		} else {
			// Do NOT arm a fail-closed block here: the connection is alive, but
			// the server IP is unknown, so the block would drop the tunnel the
			// user just established.
			logger.Warn("kill-switch: server IP unknown, block not armed (traffic is not covered)")
			cmd.warn("set-kill-switch-failed", "kill-switch not armed: server IP unknown")
		}
	}

	if was_tun_enabled {
		cmd.enableTun(profile, proxy_manager, tun_manager)
	}
}
