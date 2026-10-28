package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
)

func (cmd *Cmd) SetKillSwitch(data structs.SetKillSwitchData, proxy_manager *proxy.ProxyManager) {
	// Serialise with Connect/Disconnect: arming mid-connect with the previous
	// profile's IPs would drop the connection being established.
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()

	if data.Enabled && appconfig.GetConfig().SplitTunnel.Mode == "include" {
		cmd.warn("set-kill-switch-failed", "kill-switch is incompatible with split-tunnel include mode; switch to exclude/off first")
		cmd.send("kill-switch-updated", structs.SetKillSwitchData{Enabled: appconfig.GetConfig().KillSwitchEnabled})
		return
	}

	if !data.Enabled {
		if err := tunmode.RemoveKillSwitchBlock(); err != nil {
			logger.Warn("kill-switch: failed to remove block:", err)
			// The rules may still be dropping traffic. Reporting "off" while the
			// block is armed would be a lie, so keep the switch on and let the
			// user retry once the cause is gone.
			cmd.failKillSwitch(true, "failed to remove kill-switch block")
			return
		}
		logger.Info("kill-switch: block removed")
		appconfig.SetKillSwitch(false)
		cmd.send("kill-switch-updated", data)
		return
	}

	// Nothing to arm while disconnected: the block is installed on connect.
	if proxy_manager == nil || proxy_manager.GetStatus().Connection != "connected" {
		appconfig.SetKillSwitch(true)
		cmd.send("kill-switch-updated", data)
		return
	}

	// Enabling the kill-switch while connected must arm the firewall
	// immediately; otherwise the UI reports "ON" while traffic is still
	// unprotected until the next reconnect.
	proxyIPs := proxy_manager.GetProxyIPs()
	if len(proxyIPs) == 0 {
		logger.Warn("kill-switch: server IP unknown, block not armed")
		cmd.failKillSwitch(false, "kill-switch not armed: server IP unknown")
		return
	}

	cfg := appconfig.GetConfig()
	if err := tunmode.SetupKillSwitchBlock(proxyIPs, cfg.TunName, cfg.DnsServers); err != nil {
		logger.Warn("kill-switch: failed to apply block:", err)
		cmd.failKillSwitch(false, "failed to arm kill-switch")
		return
	}

	logger.Infof("kill-switch: block applied, whitelisted IPs: %v", proxyIPs)
	appconfig.SetKillSwitch(true)
	cmd.send("kill-switch-updated", data)
}

// failKillSwitch reports the state the daemon actually ended up in, so the UI
// and config.json never claim a state the firewall does not have.
func (cmd *Cmd) failKillSwitch(armed bool, reason string) {
	appconfig.SetKillSwitch(armed)
	cmd.send("kill-switch-updated", structs.SetKillSwitchData{Enabled: armed})
	cmd.warn("set-kill-switch-failed", reason)
}
