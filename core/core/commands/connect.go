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

	if killSwitchEnabled && len(proxyIPs) > 0 {
		if err := tunmode.SetupKillSwitchBlock(proxyIPs); err != nil {
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

	if err := proxy_manager.Connect(profile, tunName); err != nil {
		logger.Warn("connect failed:", err)
		cmd.warn("connect-failed", err.Error())
		return
	}

	if killSwitchEnabled {
		proxyIPs := proxy_manager.GetProxyIPs()
		if len(proxyIPs) > 0 {
			if err := tunmode.SetupKillSwitchBlock(proxyIPs); err != nil {
				logger.Warn("kill-switch: failed to apply block:", err)
			} else {
				logger.Info("kill-switch: block applied, whitelisted IPs:", proxyIPs)
			}
		} else {
			logger.Warn("kill-switch: no proxy IPs resolved, skipping block")
		}
	}

	if was_tun_enabled {
		cmd.enableTun(profile, tun_manager)
	}
}
