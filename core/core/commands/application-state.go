package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/lib/version"
	"whoisthat-core/lib/xraymgr"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

func (cmd *Cmd) GetApplicationState(data structs.GetApplicationStateData, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) {
	groups, err := cmd.DB.GetAllGroupsAndProfiles()
	if err != nil {
		// The TUI mirrors split/tun/autoconnect from this frame, so dropping the
		// whole reply makes every setting look reset until the next reconnect.
		// Degrade to an empty profile list instead.
		logger.Warn("failed to read groups for application state:", err)
		cmd.warn("read-groups-failed", "failed to read groups")
		groups = []structs.GroupWithProfiles{}
	}

	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()
	cfg := appconfig.GetConfig()
	application_state := structs.ApplicationState{
		Groups:           groups,
		ConnectionStatus: proxy_manager.GetStatus(),
		TunStatus:        tun_manager.IsEnabledLocked(),
		HwidInfo:         GetHwidInfo(),
		KillSwitch:       proxy_manager.KillSwitchEnabled(),
		KillSwitchArmed:  killSwitchArmed(proxy_manager),
		SplitTunnel:      appconfig.GetConfig().SplitTunnel.Mode,
		IPCFeatures:      []string{"operation-result", "config-state"},
		ProxyPorts:       &structs.ProxyPorts{SocksPort: max(cfg.SocksPort, 0), HttpPort: max(cfg.HttpPort, 0)},
		Autoconnect:      autoconnectInfo(cfg),
		XrayStatus:       xraymgr.GetManager().GetStatus(),
		CoreVersion:      version.CoreVersion,
		ProtocolVersion:  version.ProtocolVersion,
		TunName:          appconfig.GetConfig().TunName,
		TestConfig:       proxy_manager.GetTestConfig(),
		AntiDPI:          appconfig.GetConfig().AntiDPI,
		NetworkSettings: structs.NetworkSettings{
			TunMTU:     appconfig.GetConfig().TunMTU,
			DnsServers: appconfig.GetConfig().DnsServers,
		},
		ConfigStatus: structs.ConfigStatus{State: appconfig.Status().State, Error: appconfig.Status().Error},
		DBStatus:     structs.DBStatus(cmd.DB.Status()),
		Traffic:      proxy_manager.GetTraffic(),
	}

	cmd.send("application-state", application_state)
}

// killSwitchArmed never over-claims protection: an inspection failure is
// reported as "not armed" and leaves a log line for doctor/diagnostics.
func killSwitchArmed(proxy_manager *proxy.ProxyManager) bool {
	if utils.TestMode() || proxy_manager == nil || !proxy_manager.KillSwitchEnabled() {
		return false
	}
	armed, err := probeKillSwitchArmed()
	if err != nil {
		logger.Warn("kill-switch: cannot inspect state:", err)
		return false
	}
	return armed
}
