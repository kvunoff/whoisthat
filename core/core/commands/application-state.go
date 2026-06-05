package cmd

import (
	proxy "whoisthat-core/lib/proxy/mainproxy"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
	"log"
)

func (cmd *Cmd) GetApplicationState(data structs.GetApplicationStateData, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) {
	groups, err := cmd.DB.GetAllGroupsAndProfiles()
	if err != nil {
		log.Println(err.Error())
		cmd.warn("read-application-state-failed", "failed to read application state")
		return
	}

	application_state := structs.ApplicationState{
		Groups:           groups,
		ConnectionStatus: proxy_manager.GetStatus(),
		TunStatus:        tun_manager.IsEnabled,
	}

	cmd.send("application-state", application_state)
}
