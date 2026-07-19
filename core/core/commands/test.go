package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
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

// TestGroup enqueues tests for all profiles in the given group. Used by
// the `t` / `T` keybindings in the TUI when focused on a group, and by
// the auto-test-on-subscribe hook.
func (cmd *Cmd) TestGroup(data structs.TestGroupData, proxy_manager *proxy.ProxyManager) {
	all, err := cmd.DB.GetAllGroupsAndProfiles()
	if err != nil {
		logger.Warn("test-group: failed to load profiles:", err)
		cmd.warn("test-group-failed", err.Error())
		return
	}
	var profiles []structs.Profile
	for _, g := range all {
		if g.Group.Id == data.GroupId {
			profiles = g.Profiles
			break
		}
	}
	if len(profiles) == 0 {
		cmd.warn("test-group-failed", "no profiles in group")
		return
	}
	method := data.Method
	if method == "" {
		method = "http-get"
	}
	total := proxy_manager.TestGroup(profiles, method, data.SampleCount)
	proxy_manager.SeedTestProgress(data.GroupId, total)
	cmd.send("test-progress", structs.TestProgress{
		GroupId: data.GroupId,
		Tested:  0,
		Total:   total,
	})
}

// CancelTests signals in-flight test goroutines to abort. No reply is
// broadcast — the TUI decrements its in-flight counter as outstanding
// results arrive with the new epoch.
func (cmd *Cmd) CancelTests(data structs.CancelTestsData, proxy_manager *proxy.ProxyManager) {
	proxy_manager.CancelTests()
}

// SetTestConfig updates the live test configuration in the proxy manager,
// persists it to AppConfig so it survives core restarts, and broadcasts
// the new config to all clients.
func (cmd *Cmd) SetTestConfig(data structs.SetTestConfigData, proxy_manager *proxy.ProxyManager) {
	proxy_manager.SetTestConfig(data.Config)
	appconfig.SetTestConfig(appconfig.TestConfig{
		Concurrency:    data.Config.Concurrency,
		TimeoutSeconds: data.Config.TimeoutSeconds,
		SamplesPerTest: data.Config.SamplesPerTest,
		TestEndpoint:   data.Config.TestEndpoint,
		AutoTestOnSub:  data.Config.AutoTestOnSub,
	})
	cmd.send("test-config-updated", structs.TestConfigUpdated{Config: data.Config})
}
