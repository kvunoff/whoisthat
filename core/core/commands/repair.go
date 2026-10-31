package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
)

// RepairConfig restores config.json from the last known good copy and pushes a
// fresh application-state so the TUI immediately shows the recovered values.
func (cmd *Cmd) RepairConfig(proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) {
	if err := appconfig.RestoreFromBackup(); err != nil {
		logger.Warnf("repair-config: %v", err)
		cmd.warn("config-repair-failed", err.Error())
		return
	}
	logger.Warn("config: settings restored from backup")
	cmd.GetApplicationState(structs.GetApplicationStateData{}, proxy_manager, tun_manager)
}

// RestoreDB replaces the database with the newest daily snapshot.
func (cmd *Cmd) RestoreDB(proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) {
	day, err := cmd.DB.RestoreSnapshot()
	if err != nil {
		logger.Warnf("restore-db: %v", err)
		cmd.warn("db-restore-failed", err.Error())
		return
	}
	logger.Warnf("db: restored snapshot %s", day)
	cmd.GetApplicationState(structs.GetApplicationStateData{}, proxy_manager, tun_manager)
}
