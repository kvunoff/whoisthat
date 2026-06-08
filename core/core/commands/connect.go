package cmd

import (
	proxy "whoisthat-core/lib/proxy/mainproxy"
	"whoisthat-core/lib/logger"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
)

func (cmd *Cmd) Disconnect(data structs.DisconnectData, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) {
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()

	proxy_manager.Stop()
	tun_manager.Stop()
}

func (cmd *Cmd) Connect(data structs.ConnectData, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) {
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()

	profile, err := cmd.DB.GetProfile(data.Profile.GroupId, data.Profile.Id)
	if err != nil {
		logger.Warn("connect: failed to get profile:", err)
		cmd.warn("connect-failed", "Failed to connect")
		return
	}
	was_tun_enabled := tun_manager.IsEnabled

	if was_tun_enabled {
		tun_manager.Stop()
	}

	if err := proxy_manager.Connect(profile); err != nil {
		logger.Warn("connect failed:", err)
		cmd.warn("connect-failed", "Failed to connect")
		return
	}

	if was_tun_enabled {
		cmd.enableTun(profile, tun_manager)
	}
}
