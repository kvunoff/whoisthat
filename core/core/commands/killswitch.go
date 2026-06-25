package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
)

func (cmd *Cmd) SetKillSwitch(data structs.SetKillSwitchData) {
	appconfig.SetKillSwitch(data.Enabled)

	if !data.Enabled {
		if err := tunmode.RemoveKillSwitchBlock(); err != nil {
			logger.Warn("kill-switch: failed to remove block:", err)
		} else {
			logger.Info("kill-switch: block removed")
		}
	}

	cmd.send("kill-switch-updated", data)
}
