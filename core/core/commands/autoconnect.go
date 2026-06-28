package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/structs"
)

func (cmd *Cmd) SetAutoconnect(data structs.SetAutoconnectData) {
	appconfig.SetAutoconnect(data.Enabled, data.GroupId, data.ProfileId, data.Mode)
	info := structs.AutoconnectInfo{
		Enabled: data.Enabled,
		Mode:    appconfig.GetConfig().AutoconnectMode,
	}
	cmd.send("autoconnect-updated", info)
}
