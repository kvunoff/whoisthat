package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/structs"
)

func (cmd *Cmd) SetHwid(data structs.SetHwidData) {
	if data.Enabled != nil {
		appconfig.EnableHwid(*data.Enabled)
	}
	if data.Reset {
		appconfig.ResetHwid()
	}
	if data.UserAgent != nil {
		appconfig.SetUserAgent(*data.UserAgent)
	}
	cmd.broadcastHwid()
}

func (cmd *Cmd) broadcastHwid() {
	cfg := appconfig.GetConfig()
	hwidInfo := structs.HwidData{
		Enabled:   cfg.HwidEnabled,
		Hwid:      cfg.Hwid,
		UserAgent: cfg.UserAgent,
		Platform:  appconfig.Platform(),
		Kernel:    appconfig.KernelVersion(),
		Model:     appconfig.DistroModel(),
	}
	cmd.send("hwid-updated", hwidInfo)
}

func GetHwidInfo() structs.HwidData {
	cfg := appconfig.GetConfig()
	return structs.HwidData{
		Enabled:   cfg.HwidEnabled,
		Hwid:      cfg.Hwid,
		UserAgent: cfg.UserAgent,
		Platform:  appconfig.Platform(),
		Kernel:    appconfig.KernelVersion(),
		Model:     appconfig.DistroModel(),
	}
}
