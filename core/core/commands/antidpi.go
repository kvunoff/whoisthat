package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
)

// SetAntiDPI persists the global fragment/noise settings. Active connections
// keep their current masks; the next Connect/reload picks up the new config.
func (cmd *Cmd) SetAntiDPI(data structs.SetAntiDPIData) {
	if err := appconfig.SetAntiDPI(data.Config); err != nil {
		cmd.warn("set-anti-dpi-failed", err.Error())
		return
	}
	logger.Infof("anti-dpi settings updated")
	cmd.send("anti-dpi-updated", structs.AntiDPIUpdated{Config: appconfig.GetConfig().AntiDPI})
}
