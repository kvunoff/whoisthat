package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
)

// SetSplitTunnel persists the requested split-tunnel mode and, if TUN is
// currently up, reconciles the live routing to match. The mode is validated in
// AppConfig (invalid -> "off"); the persisted value is echoed back so the TUI
// reflects any coercion.
func (cmd *Cmd) SetSplitTunnel(data structs.SetSplitTunnelData, tun_manager *tunmode.TunModeManager) {
	mode := appconfig.SetSplitTunnelMode(data.Mode)

	if tun_manager.IsEnabledLocked() {
		if err := tun_manager.ReapplySplit(); err != nil {
			logger.Warn("split: failed to reapply live rules:", err)
			cmd.warn("set-split-tunnel-failed", "failed to apply split tunnel routing")
			return
		}
		logger.Infof("split: live rules reapplied for mode %q", mode)
	}

	cmd.send("split-tunnel-updated", structs.SetSplitTunnelData{Mode: mode})
}
