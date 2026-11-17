package cmd

import (
	"errors"
	"fmt"
	"time"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
)

func (cmd *Cmd) Disconnect(data structs.DisconnectData, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) {
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()

	proxy_manager.Stop()
	tun_manager.Stop()
	// The kill-switch block (when enabled) is deliberately left in place: with
	// no connection the machine must not leak, and removing the TUN device
	// already disables its allowlist rule.
}

// connectTimeout bounds the whole connect (handshake and TUN bring-up). A
// half-open handshake never returns, and the core has a single connection
// lock: without a bound one dead server wedges every arm/disarm/settings write.
var connectTimeout = 60 * time.Second

// connectBody is a test seam so the deadline can be exercised without a core.
var connectBody = (*Cmd).connectLocked

func (cmd *Cmd) Connect(data structs.ConnectData, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) {
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()

	// The body cannot be cancelled (the handshake lives in xray and the TUN
	// helpers), so it is abandoned on timeout: the lock is released here and
	// the stuck call may finish, or stay stuck, on its own. Upkeep: replace with
	// real context plumbing through ProxyManager.Connect when one exists.
	done := make(chan struct{})
	go func() {
		defer close(done)
		connectBody(cmd, data, proxy_manager, tun_manager)
	}()
	select {
	case <-done:
	case <-time.After(connectTimeout):
		logger.Warnf("connect: no result after %s; releasing the connection lock", connectTimeout)
		cmd.warn("connect-failed", fmt.Sprintf("connect timed out after %s; the server may be unreachable", connectTimeout))
	}
}

func (cmd *Cmd) connectLocked(data structs.ConnectData, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager) {
	profile, err := cmd.DB.GetProfile(data.Profile.GroupId, data.Profile.Id)
	if err != nil {
		logger.Warn("connect: failed to get profile:", err)
		cmd.warn("connect-failed", fmt.Sprintf("profile lookup failed: %v", err))
		return
	}
	was_tun_enabled := tun_manager.IsEnabledLocked()

	// 1. If switching while connected or if TUN was enabled, stop the active proxy
	// process first so it releases /dev/net/tun before we clean up network rules and delete the device.
	if proxy_manager != nil {
		proxy_manager.Stop()
	}
	if was_tun_enabled {
		tun_manager.Stop()
	}

	tunName := appconfig.GetConfig().TunName

	// Connect the new profile in base proxy mode first (tunEnabled = false).
	// If TUN was previously active, cmd.enableTun below will create the TUN device
	// and routing rules first, then reload the proxy with TUN inbound. This guarantees
	// Xray doesn't race against tun_manager.Start over creating/opening /dev/net/tun.
	//
	// The kill-switch is not touched here: it stays armed across the switch and
	// the pre-start hook (main.go) installs the new server's allowlist right
	// before Xray starts.
	if err := proxy_manager.Connect(profile, tunName, false); err != nil {
		logger.Warn("connect failed:", err)
		cmd.warn("connect-failed", err.Error())
		if errors.Is(err, proxy.ErrKillSwitchNotArmed) {
			cmd.warn("set-kill-switch-failed", "kill-switch is enabled but could not be armed; connect aborted")
		}
		return
	}

	if was_tun_enabled {
		cmd.enableTun(profile, proxy_manager, tun_manager)
	}
}
