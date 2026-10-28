package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/lib/version"
	"whoisthat-core/lib/xraymgr"
	"whoisthat-core/structs"
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

	application_state := structs.ApplicationState{
		Groups:           groups,
		ConnectionStatus: proxy_manager.GetStatus(),
		TunStatus:        tun_manager.IsEnabledLocked(),
		HwidInfo:         GetHwidInfo(),
		KillSwitch:       proxy_manager.KillSwitchEnabled(),
		SplitTunnel:      appconfig.GetConfig().SplitTunnel.Mode,
		Autoconnect: structs.AutoconnectInfo{
			Enabled: appconfig.GetConfig().AutoconnectEnabled,
			Mode:    appconfig.GetConfig().AutoconnectMode,
		},
		XrayStatus:      xraymgr.GetManager().GetStatus(),
		CoreVersion:     version.CoreVersion,
		ProtocolVersion: version.ProtocolVersion,
		TunName:         appconfig.GetConfig().TunName,
		TestConfig:      proxy_manager.GetTestConfig(),
	}

	cmd.send("application-state", application_state)
}
