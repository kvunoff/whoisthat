package cmd

import (
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	"whoisthat-core/structs"
)

func (cmd *Cmd) TestProfile(data structs.TestProfileData, proxy_manager *proxy.ProxyManager) {
	profile, err := cmd.DB.GetProfile(data.Profile.GroupId, data.Profile.Id)
	if err != nil {
		logger.Warn("test: failed to get profile:", err)
		cmd.warn("test-failed", err.Error())
		return
	}
	method := data.Method
	if method == "" {
		method = "http-get"
	}
	proxy_manager.TestProfile(profile, method)
}
