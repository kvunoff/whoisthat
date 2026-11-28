package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/structs"
)

func (cmd *Cmd) SetAutoconnect(data structs.SetAutoconnectData) {
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()
	if data.Enabled && data.ProfileId <= 0 {
		cmd.warn("set-autoconnect-failed", "autoconnect requires a target profile")
	} else if err := appconfig.SetAutoconnect(data.Enabled, data.GroupId, data.ProfileId, data.Mode); err != nil {
		cmd.warn("set-autoconnect-failed", err.Error())
	}
	cmd.send("autoconnect-updated", autoconnectInfo(appconfig.GetConfig()))
}

func autoconnectInfo(cfg appconfig.AppConfig) structs.AutoconnectInfo {
	info := structs.AutoconnectInfo{Enabled: cfg.AutoconnectEnabled, Mode: cfg.AutoconnectMode}
	if cfg.AutoconnectProfileId > 0 {
		info.Target = &structs.ProfileID{Id: cfg.AutoconnectProfileId, GroupId: cfg.AutoconnectGroupId}
	}
	return info
}
