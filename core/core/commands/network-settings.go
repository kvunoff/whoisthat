package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
)

// SetNetworkSettings persists the TUN MTU and DNS servers as one block. Active
// connections keep their current config; the next Connect/reload picks it up.
func (cmd *Cmd) SetNetworkSettings(data structs.SetNetworkSettingsData) {
	if err := appconfig.SetNetworkSettings(data.Config); err != nil {
		cmd.warn("set-network-settings-failed", err.Error())
		return
	}
	logger.Infof("network settings updated")
	cfg := appconfig.GetConfig()
	cmd.send("network-settings-updated", structs.NetworkSettingsUpdated{
		Config: structs.NetworkSettings{
			TunMTU:     cfg.TunMTU,
			DnsServers: cfg.DnsServers,
		},
	})
}
