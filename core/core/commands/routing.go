package cmd

import (
	"fmt"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	"whoisthat-core/structs"
)

func (cmd *Cmd) GetRouting(data structs.GetRoutingData) {
	cfg, err := cmd.DB.LoadRouting()
	if err != nil {
		logger.Warnf("failed to load routing: %v", err)
		cmd.warn("get-routing-failed", "failed to load routing config")
		return
	}
	cmd.send("routing-updated", structs.RoutingUpdated{Config: cfg})
}

func (cmd *Cmd) UpdateRouting(data structs.UpdateRoutingData, proxy_manager *proxy.ProxyManager) {
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()

	if err := cmd.DB.SaveRouting(data.Config); err != nil {
		logger.Warnf("failed to save routing: %v", err)
		cmd.warn("update-routing-failed", "failed to save routing config")
		return
	}
	cmd.send("routing-updated", structs.RoutingUpdated{Config: data.Config})

	if proxy_manager != nil {
		tunName := appconfig.GetConfig().TunName
		if err := proxy_manager.ReloadRouting(tunName); err != nil {
			logger.Warnf("failed to reload routing in running proxy: %v", err)
			cmd.warn("update-routing-failed", fmt.Sprintf("failed to reload routing: %v", err))
		}
	}
}
