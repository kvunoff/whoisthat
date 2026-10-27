package cmd

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
)

// SetSplitTunnel persists the requested split-tunnel mode and, if TUN is
// currently up, reconciles the live routing to match. The mode is validated in
// AppConfig (invalid -> "off"); the persisted value is echoed back so the TUI
// reflects any coercion.
func (cmd *Cmd) SetSplitTunnel(data structs.SetSplitTunnelData, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) {
	mode := appconfig.SetSplitTunnelMode(data.Mode)

	if tun_manager.IsEnabledLocked() {
		if err := tun_manager.ReapplySplit(); err != nil {
			logger.Warn("split: failed to reapply live rules:", err)
			cmd.warn("set-split-tunnel-failed", "failed to apply split tunnel routing")
			return
		}
		// include/exclude changes whether FakeDNS is safe, so Xray's DNS section
		// must be regenerated as well as the kernel hijack.
		if proxy_manager != nil {
			if err := proxy_manager.ReloadRouting(appconfig.GetConfig().TunName); err != nil {
				logger.Warn("split: failed to reload proxy for new DNS mode:", err)
			}
		}
		if err := tun_manager.ReapplyDnsHijack(); err != nil {
			logger.Warn("split: failed to reapply dns hijack:", err)
			cmd.warn("set-split-tunnel-failed", "failed to apply split tunnel dns routing")
			return
		}
		logger.Infof("split: live rules reapplied for mode %q", mode)
	}

	cmd.send("split-tunnel-updated", structs.SetSplitTunnelData{Mode: mode})
}
