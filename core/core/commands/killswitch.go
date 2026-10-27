package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
)

func (cmd *Cmd) SetKillSwitch(data structs.SetKillSwitchData, proxy_manager *proxy.ProxyManager) {
	appconfig.SetKillSwitch(data.Enabled)

	if !data.Enabled {
		if err := tunmode.RemoveKillSwitchBlock(); err != nil {
			logger.Warn("kill-switch: failed to remove block:", err)
		} else {
			logger.Info("kill-switch: block removed")
		}
		cmd.send("kill-switch-updated", data)
		return
	}

	// Enabling the kill-switch while connected must arm the firewall
	// immediately; otherwise the UI reports "ON" while traffic is still
	// unprotected until the next reconnect.
	if proxy_manager != nil && proxy_manager.GetStatus().Connection == "connected" {
		proxyIPs := proxy_manager.GetProxyIPs()
		if len(proxyIPs) == 0 {
			logger.Warn("kill-switch: no proxy IPs resolved; block will be applied on the next connect")
		} else {
			cfg := appconfig.GetConfig()
			if err := tunmode.SetupKillSwitchBlock(proxyIPs, cfg.TunName, cfg.DnsServers); err != nil {
				logger.Warn("kill-switch: failed to apply block:", err)
			} else {
				logger.Infof("kill-switch: block applied, whitelisted IPs: %v", proxyIPs)
			}
		}
	}

	cmd.send("kill-switch-updated", data)
}
