package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

// killSwitchProxy is the slice of ProxyManager the kill-switch needs; keeping
// it an interface lets the command logic be tested without a real core.
type killSwitchProxy interface {
	GetStatus() structs.ProxyStatus
	GetProxyIPs() []string
}

// Test seams: the command layer decides *when* to arm, so its tests must not
// touch the real firewall.
var (
	setupKillSwitchBlock  = tunmode.SetupKillSwitchBlock
	removeKillSwitchBlock = tunmode.RemoveKillSwitchBlock
	probeKillSwitchArmed  = tunmode.KillSwitchArmed
)

func (cmd *Cmd) SetKillSwitch(data structs.SetKillSwitchData, proxy_manager killSwitchProxy) {
	// Serialise with Connect/Disconnect: arming mid-connect with the previous
	// profile's IPs would drop the connection being established.
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()

	if utils.TestMode() {
		appconfig.SetKillSwitch(data.Enabled)
		cmd.send("kill-switch-updated", structs.SetKillSwitchData{Enabled: data.Enabled, Armed: false})
		return
	}

	if data.Enabled && appconfig.GetConfig().SplitTunnel.Mode == "include" {
		cmd.warn("set-kill-switch-failed", "kill-switch is incompatible with split-tunnel include mode; switch to exclude/off first")
		cmd.send("kill-switch-updated", cmd.killSwitchState())
		return
	}

	if !data.Enabled {
		if data.Force {
			// Explicit escape hatch for a broken firewall: remove best-effort
			// and report off even though the state could not be verified.
			_ = removeKillSwitchBlock()
			appconfig.SetKillSwitch(false)
			logger.Warn("kill-switch: force-disabled; firewall state was not verified")
			cmd.warn("set-kill-switch-failed", "kill-switch force-disabled; rules were removed best-effort")
			cmd.send("kill-switch-updated", structs.SetKillSwitchData{Enabled: false, Armed: false})
			return
		}
		if err := removeKillSwitchBlock(); err != nil {
			logger.Warn("kill-switch: failed to remove block:", err)
			// The rules may still be dropping traffic. Reporting "off" while the
			// block is armed would be a lie, so keep the switch on and let the
			// user retry (or use --force) once the cause is gone.
			cmd.failKillSwitch(true, "failed to remove kill-switch block")
			return
		}
		logger.Info("kill-switch: block removed")
		appconfig.SetKillSwitch(false)
		cmd.send("kill-switch-updated", structs.SetKillSwitchData{Enabled: false, Armed: false})
		return
	}

	// Enabled is the user's intent. Keep it even when arming fails: the actual
	// state is reported separately as kill_switch_armed, so the UI can show
	// "ON (not armed)" instead of silently claiming protection.
	cfg := appconfig.GetConfig()
	ips := []string{}
	connected := proxy_manager != nil && proxy_manager.GetStatus().Connection == "connected"
	if connected {
		ips = proxy_manager.GetProxyIPs()
	}
	if connected && len(ips) == 0 {
		// Arming an empty allowlist on a live connection would drop the tunnel.
		logger.Warn("kill-switch: server IP unknown, block not armed")
		cmd.warn("set-kill-switch-failed", "kill-switch not armed: server IP unknown")
		appconfig.SetKillSwitch(true)
		cmd.send("kill-switch-updated", structs.SetKillSwitchData{Enabled: true, Armed: false})
		return
	}

	if err := setupKillSwitchBlock(ips, cfg.TunName, cfg.DnsServers); err != nil {
		logger.Warn("kill-switch: failed to apply block:", err)
		cmd.warn("set-kill-switch-failed", "failed to arm kill-switch")
		appconfig.SetKillSwitch(true)
		cmd.send("kill-switch-updated", structs.SetKillSwitchData{Enabled: true, Armed: false})
		return
	}

	logger.Infof("kill-switch: block applied, whitelisted IPs: %v", ips)
	appconfig.SetKillSwitch(true)
	cmd.send("kill-switch-updated", structs.SetKillSwitchData{Enabled: true, Armed: true})
}

// killSwitchState reports the persisted intent plus the actual kernel state.
func (cmd *Cmd) killSwitchState() structs.SetKillSwitchData {
	enabled := appconfig.GetConfig().KillSwitchEnabled
	armed := false
	if enabled {
		var err error
		armed, err = probeKillSwitchArmed()
		if err != nil {
			logger.Warn("kill-switch: cannot inspect state:", err)
			armed = false
		}
	}
	return structs.SetKillSwitchData{Enabled: enabled, Armed: armed}
}

// failKillSwitch reports the state the daemon actually ended up in, so the UI
// and config.json never claim a state the firewall does not have.
func (cmd *Cmd) failKillSwitch(armed bool, reason string) {
	appconfig.SetKillSwitch(armed)
	cmd.send("kill-switch-updated", cmd.killSwitchState())
	cmd.warn("set-kill-switch-failed", reason)
}
