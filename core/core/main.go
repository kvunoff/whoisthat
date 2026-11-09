package main

import (
	"errors"
	"fmt"
	lumberjack "gopkg.in/natefinch/lumberjack.v2"
	"log"
	"net"
	"os"
	"os/signal"
	"path/filepath"
	"runtime/debug"
	"strconv"
	"strings"
	"syscall"
	"time"
	cmd "whoisthat-core/commands"
	"whoisthat-core/db"
	"whoisthat-core/lib"
	"whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/TCPServer"
	"whoisthat-core/lib/geo"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/lib/version"
	"whoisthat-core/lib/xraymgr"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

const usage = "Usage: whoisthat-core\n\nRuns in the foreground and serves the TUI/CLI over IPC.\n"

// versionLine is a stable contract: install.sh and release.yml parse it.
func versionLine() string {
	return fmt.Sprintf("whoisthat-core v%s (protocol %d)\n", version.CoreVersion, version.ProtocolVersion)
}

// handleArgs reports whether the process should exit before starting the daemon.
func handleArgs(args []string) (code int, done bool) {
	if len(args) == 0 {
		return 0, false
	}
	switch args[0] {
	case "--version", "-v", "version":
		fmt.Print(versionLine())
		return 0, true
	case "--help", "-h":
		fmt.Print(usage)
		return 0, true
	default:
		fmt.Fprintf(os.Stderr, "whoisthat-core: unknown argument %q (run with --help)\n", args[0])
		return 2, true
	}
}

// testMode keeps the daemon away from the network and the global firewall so
// the IPC smoke test can run it in a sandbox (see scripts/ipc-e2e.test.sh).
func testMode() bool { return utils.TestMode() }

func main() {
	if code, done := handleArgs(os.Args[1:]); done {
		os.Exit(code)
	}
	configDir := utils.ConfigDir()
	logPath := coreLogPath(configDir)

	log.SetOutput(&lumberjack.Logger{
		Filename:   logPath,
		MaxSize:    20,
		MaxBackups: 2,
		MaxAge:     0,
		Compress:   false,
	})

	log.SetFlags(log.LstdFlags)
	logger.SetLevel(os.Getenv("WHOISTHAT_LOG_LEVEL"))
	logger.Infof(
		"whoisthat-core v%s starting (protocol %d, log: %s)",
		version.CoreVersion,
		version.ProtocolVersion,
		logPath,
	)
	setupCrashLog()

	// A second daemon must not race the first one for the IPC socket, the TUN
	// device and the firewall. The lock is held until the process exits.
	// Config is loaded first so the lock probe knows about an ipc-socket-path
	// override from config.json.
	appconfig.LoadConfig()
	held, lock_dir, err := acquireInstanceLock()
	if err != nil {
		// The runtime dir may be unavailable; the socket probe below still
		// keeps us from stealing a live core's socket.
		logger.Warnf("instance lock unavailable: %v", err)
	} else if !held {
		logger.Infof("another whoisthat-core is already running (lock: %s); exiting", lock_dir)
		return // exit 0: a non-zero code would loop under systemd Restart=on-failure
	}

	stop_sig := make(chan bool, 1)
	utils.RaiseAmbientCaps()

	// Cores from older builds hold no flock. Refuse to start while their
	// socket is live: this check runs before the reconcile so we never tear
	// down rules that a running daemon owns.
	if utils.SocketIsLive(appconfig.SocketPath()) {
		logger.Infof("another whoisthat-core is already serving %s; exiting", appconfig.SocketPath())
		return
	}

	// One core per network namespace: the TUN name and nft table are global.
	// The marker is root-created (best effort); when present it names the uid
	// that owns the rules, so a second user must not reconcile them away.
	if !testMode() {
		uid := runtimeUid()
		if err := checkActiveUser(uid); err != nil {
			logger.Error("refusing to start:", err)
			return
		}
		claimActiveUser(uid)
	}

	// Self-heal: if no other core is already listening, tear down any leftover
	// firewall/routing rules from a previous core that exited without cleaning
	// up (hard quit, crash, power loss mid-TUN). Guarded on the IPC endpoint
	// being free so we never nuke a live core's rules.
	if !testMode() && !coreAlreadyRunning() {
		if err := tunmode.ReconcileOrphanedRules(appconfig.GetConfig().TunName); err != nil {
			logger.Warnf("startup reconcile: %v", err)
		} else {
			logger.Info("startup reconcile: orphaned rules cleared")
		}
		// Reconcile removes the kill-switch table; if the user asked for it,
		// re-arm immediately so a restart never silently exposes the machine.
		if appconfig.GetConfig().KillSwitchEnabled {
			cfg := appconfig.GetConfig()
			if err := tunmode.SetupKillSwitchBlock(nil, cfg.TunName, cfg.DnsServers); err != nil {
				logger.Warnf("kill-switch: failed to re-arm on startup: %v", err)
			} else {
				logger.Info("kill-switch: block re-armed on startup")
			}
		}
	}

	if !testMode() {
		geoDir := filepath.Join(configDir, "geo")
		if _, err := geo.EnsureAssets(geoDir); err != nil {
			logger.Warnf("main: geo assets not available: %v", err)
		}
	}
	database := db.DB{}
	database.Initialize()
	// No unconditional SaveConfig here: writing the in-memory config on every
	// start used to turn a failed read into permanent data loss.
	proxy_manager := proxy.ProxyManager{}
	proxy_manager.DB = &database
	proxy_manager.Init()
	// Install the kill-switch allowlist with the exact IPs Xray is about to
	// use, right before it starts. This keeps the block armed for the whole
	// connect instead of opening a leak window.
	proxy_manager.SetPreStartHandler(func(ips []string) error {
		if testMode() || !appconfig.GetConfig().KillSwitchEnabled {
			return nil
		}
		if len(ips) == 0 {
			return fmt.Errorf("%w: server IP unknown", proxy.ErrKillSwitchNotArmed)
		}
		cfg := appconfig.GetConfig()
		if err := tunmode.SetupKillSwitchBlock(ips, cfg.TunName, cfg.DnsServers); err != nil {
			return fmt.Errorf("%w: %v", proxy.ErrKillSwitchNotArmed, err)
		}
		logger.Infof("kill-switch: allowlist applied before start, %d IPs", len(ips))
		return nil
	})
	// Seed the live test config from the persisted AppConfig so user
	// customizations (samples/concurrency/endpoint/timeout) made in the
	// TUI's Settings tab survive core restarts.
	ac := appconfig.GetConfig()
	proxy_manager.SetTestConfig(structs.TestConfig{
		Concurrency:    ac.TestConfig.Concurrency,
		TimeoutSeconds: ac.TestConfig.TimeoutSeconds,
		SamplesPerTest: ac.TestConfig.SamplesPerTest,
		TestEndpoint:   ac.TestConfig.TestEndpoint,
		AutoTestOnSub:  ac.TestConfig.AutoTestOnSub,
	})
	tun_manager := tunmode.TunModeManager{}
	tun_manager.Init()

	xm := xraymgr.GetManager()
	if err := xm.Init(); err != nil {
		logger.Warnf("main: xraymgr init: %v", err)
	}
	if !testMode() && xm.GetStatus().Status != "ready" {
		logger.Infof("main: xray status is %s, ensuring runtime in background", xm.GetStatus().Status)
		xm.EnsureRuntimeAsync()
	}

	// Pre-flight: probe for external runtimes the core manages
	// (xray). Each miss is paired with an actionable
	// install hint and unicast as a warn to every newly connected TUI client.
	missing := TCPServer.CheckMissingBinaries()
	for _, mb := range missing {
		logger.Warnf("preflight: %s", mb.Hint)
	}

	server := TCPServer.NewServer(&database, &proxy_manager, &tun_manager, stop_sig, missing)
	server.Start()

	if !testMode() {
		go func() {
			// Auto-update first probes all unknown intervals. Waiting for
			// autoconnect means the VPN is up before any subscription traffic.
			bootAutoconnect(&database, &proxy_manager, &tun_manager, server)
			autoCmd := cmd.Cmd{DB: &database, Broadcast: server.Broadcast, TunManager: &tun_manager}
			autoCmd.AutoUpdateLoop(&proxy_manager)
		}()
	}

	go func() {
		sigs := make(chan os.Signal, 1)
		signal.Notify(sigs, syscall.SIGINT, syscall.SIGTERM)
		reason := ""
		select {
		case sig := <-sigs:
			reason = fmt.Sprintf("Received signal %v", sig)
		case <-stop_sig:
			reason = "Received stop request"
		}
		logger.Info("whoisthat-core shutting down:", reason)

		// Bound cleanup so a hung Stop() (stuck subprocess kill, blocked
		// firewall teardown) can't wedge shutdown forever. If cleanup
		// doesn't finish in 10s, exit anyway — the startup reconcile on the
		// next launch will tear down any leftover rules.
		done := make(chan struct{})
		go func() {
			// The kill-switch block is deliberately not removed here: when it
			// is enabled it must survive a stopped daemon. It is removed only
			// by an explicit kill-switch=false or the startup reconcile.
			proxy_manager.Stop()
			tun_manager.Stop()
			os.Remove(appconfig.SocketPath())
			server.Broadcast(lib.CreateJsonNotification("warn", structs.Warning{Key: "died", Content: reason}))
			close(done)
		}()
		select {
		case <-done:
		case <-time.After(10 * time.Second):
			logger.Warn("shutdown: cleanup timed out after 10s, exiting anyway")
			os.Remove(appconfig.SocketPath())
		}
		releaseActiveUser()
		os.Exit(0)
	}()
	select {}
}

// instanceLock stays open for the lifetime of the process. Closing it (or
// exiting) releases the flock so the next core can start.
var instanceLock *os.File

// acquireInstanceLock tries the primary runtime dir and also checks the known
// fallback locations for a lock held by an older/newer core. It returns
// (held, dir, error): a held lock is kept for the process lifetime, a busy one
// in any known dir means another core owns the rules.
func acquireInstanceLock() (bool, string, error) {
	dir := utils.RuntimeDir()
	for _, other := range knownRuntimeDirs() {
		if other == dir {
			continue
		}
		held, err := lockHeld(other)
		if err != nil {
			// An unreadable probe must not block startup; the socket check and
			// the lock in the primary dir still protect us.
			continue
		}
		if held {
			return false, other, nil
		}
	}
	if err := os.MkdirAll(dir, 0700); err != nil {
		return false, dir, err
	}
	f, err := os.OpenFile(filepath.Join(dir, "core.lock"), os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return false, dir, err
	}
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		f.Close()
		if errors.Is(err, syscall.EWOULDBLOCK) {
			return false, dir, nil
		}
		return false, dir, err
	}
	instanceLock = f
	return true, dir, nil
}

// lockHeld reports whether another process holds core.lock in dir. Stale lock
// files are not an obstacle: only an actual flock counts.
func lockHeld(dir string) (bool, error) {
	info, err := os.Stat(dir)
	if err != nil || !info.IsDir() {
		return false, nil
	}
	f, err := os.OpenFile(filepath.Join(dir, "core.lock"), os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return false, err
	}
	defer f.Close()
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		if errors.Is(err, syscall.EWOULDBLOCK) {
			return true, nil
		}
		return false, err
	}
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_UN); err != nil {
		return false, err
	}
	return false, nil
}

// knownRuntimeDirs lists every directory a core may have used for its socket.
// Probing all of them prevents a new core from reconciling rules that belong to
// a live core started with a different runtime dir.
func knownRuntimeDirs() []string {
	uid := strconv.Itoa(runtimeUid())
	dirs := []string{utils.RuntimeDir()}
	if p := strings.TrimSpace(appconfig.GetConfig().IPCSocketPath); p != "" {
		dirs = append(dirs, filepath.Dir(p))
	}
	dirs = append(dirs,
		filepath.Join("/run/whoisthat", uid, "whoisthat"),
		filepath.Join("/tmp", "whoisthat-"+uid),
	)
	if xdg := os.Getenv("XDG_RUNTIME_DIR"); filepath.IsAbs(xdg) {
		dirs = append(dirs, filepath.Join(xdg, "whoisthat"))
	}
	seen := make(map[string]bool, len(dirs))
	out := make([]string, 0, len(dirs))
	for _, dir := range dirs {
		if !seen[dir] {
			seen[dir] = true
			out = append(out, dir)
		}
	}
	return out
}

func runtimeUid() int {
	if uid := utils.RealUserUid(); uid > 0 {
		return uid
	}
	return os.Getuid()
}

const activeMarkerPath = "/run/whoisthat/active"

// checkActiveUser blocks a second uid from reconciling the rules of the active
// core. Best effort: without a root-created marker nothing is enforced.
func checkActiveUser(uid int) error {
	data, err := os.ReadFile(activeMarkerPath)
	if err != nil {
		return nil
	}
	owner := strings.TrimSpace(string(data))
	if owner == "" || owner == strconv.Itoa(uid) {
		return nil
	}
	return fmt.Errorf("another user (uid %s) owns whoisthat-core/tun; stop it before starting this one", owner)
}

// claimActiveUser is best effort: /run/whoisthat is root-owned, so a normal
// user can only update the marker when the helper made it writable.
// shortcut: the marker is only enforced when a root helper pre-creates it;
// upgrade by having install-init write it once multi-user sessions matter.
func claimActiveUser(uid int) {
	err := os.WriteFile(activeMarkerPath, []byte(strconv.Itoa(uid)+"\n"), 0644)
	if err != nil && !errors.Is(err, os.ErrNotExist) && !errors.Is(err, syscall.EACCES) {
		logger.Warnf("active-core marker: %v", err)
	}
}

func releaseActiveUser() {
	data, err := os.ReadFile(activeMarkerPath)
	if err != nil {
		return
	}
	if strings.TrimSpace(string(data)) != strconv.Itoa(runtimeUid()) {
		return
	}
	if err := os.Remove(activeMarkerPath); err != nil && !errors.Is(err, os.ErrNotExist) {
		logger.Warnf("active-core marker cleanup: %v", err)
	}
}

// coreAlreadyRunning reports whether another whoisthat-core is already
// coreLogPath returns the first writable location. A read-only config
// directory (which exists, so MkdirAll succeeds) must not silently swallow the
// log: probe the actual file and fall back to a per-user temp directory.
func coreLogPath(configDir string) string {
	candidate := filepath.Join(configDir, "core.log")
	if err := os.MkdirAll(configDir, 0700); err == nil {
		probe, err := os.OpenFile(candidate, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0600)
		if err == nil {
			_ = probe.Close()
			return candidate
		}
	}

	fallbackDir := filepath.Join(os.TempDir(), fmt.Sprintf("whoisthat-%d", utils.RealUserUid()))
	_ = os.MkdirAll(fallbackDir, 0700)
	utils.EnsurePerms(fallbackDir, 0700)
	return filepath.Join(fallbackDir, "core.log")
}

// setupCrashLog captures unhandled panics and fatal runtime errors. They go to
// stderr, which is /dev/null when the TUI spawned us, so without this a core
// crash leaves no trace anywhere.
func setupCrashLog() {
	dataDir := utils.DataDir()
	if err := os.MkdirAll(dataDir, 0700); err != nil {
		return
	}
	path := filepath.Join(dataDir, "core-crash.log")

	// Bound crash-loop growth.
	if st, err := os.Stat(path); err == nil && st.Size() > 1<<20 {
		_ = os.Rename(path, path+".1")
	}

	file, err := os.OpenFile(path, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0600)
	if err != nil {
		return
	}
	// SetCrashOutput duplicates the fd, so the original can be closed.
	_ = debug.SetCrashOutput(file, debug.CrashOptions{})
	_ = file.Close()
}

// coreAlreadyRunning reports whether another whoisthat-core is already
// listening on the IPC endpoint. Probes the UDS (the default transport) and,
// if TCP is enabled, the TCP port too — a hit on either means a live core owns
// the firewall/routing rules.
func coreAlreadyRunning() bool {
	cfg := appconfig.GetConfig()
	if utils.SocketIsLive(appconfig.SocketPath()) {
		return true
	}
	for _, dir := range knownRuntimeDirs() {
		if utils.SocketIsLive(filepath.Join(dir, "core.sock")) {
			return true
		}
	}
	if cfg.TCPEnabled {
		if conn, err := net.DialTimeout("tcp", fmt.Sprintf("127.0.0.1:%d", cfg.CoreTCPPort), 300*time.Millisecond); err == nil {
			conn.Close()
			return true
		}
	}
	return false
}

func bootAutoconnect(database *db.DB, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager, server *TCPServer.Server) {
	cfg := appconfig.GetConfig()
	if !cfg.AutoconnectEnabled || cfg.AutoconnectProfileId == 0 {
		return
	}

	time.Sleep(5 * time.Second)

	command_handler := cmd.Cmd{DB: database, Broadcast: server.Broadcast, TunManager: tun_manager}
	connectData := structs.ConnectData{
		Profile: structs.ProfileID{
			Id:      cfg.AutoconnectProfileId,
			GroupId: cfg.AutoconnectGroupId,
		},
	}

	connected := false
	for attempt := 0; attempt < 3; attempt++ {
		logger.Infof("boot-autoconnect: connect attempt %d (gid=%d pid=%d)", attempt+1, cfg.AutoconnectGroupId, cfg.AutoconnectProfileId)
		command_handler.Connect(connectData, proxy_manager, tun_manager)
		status := proxy_manager.GetStatus()
		if status.Connection == "connected" {
			logger.Info("boot-autoconnect: connected")
			connected = true
			break
		}
		logger.Warnf("boot-autoconnect: connect attempt %d failed, retrying in 5s", attempt+1)
		time.Sleep(5 * time.Second)
	}

	if !connected {
		logger.Warn("boot-autoconnect: all connect attempts failed")
		return
	}

	if cfg.AutoconnectMode != "tun" {
		return
	}

	if !utils.CanTun() {
		logger.Warn("boot-autoconnect: TUN mode was requested but the core binary lacks network capabilities.")
		logger.Warn("boot-autoconnect: this usually happens after rebuilding the core binary (inode changes, caps lost).")
		logger.Warnf("boot-autoconnect: fix: sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep %s", os.Args[0])
		return
	}

	profile, err := database.GetProfile(cfg.AutoconnectGroupId, cfg.AutoconnectProfileId)
	if err != nil {
		logger.Warn("boot-autoconnect: failed to get profile for tun:", err)
		return
	}

	for attempt := 0; attempt < 5; attempt++ {
		if tun_manager.IsEnabledLocked() {
			logger.Info("boot-autoconnect: tun mode enabled")
			return
		}
		logger.Infof("boot-autoconnect: tun attempt %d", attempt+1)
		command_handler.EnableTunForProfile(profile, proxy_manager, tun_manager)
		if tun_manager.IsEnabledLocked() {
			logger.Info("boot-autoconnect: tun mode enabled")
			return
		}
		logger.Warnf("boot-autoconnect: tun attempt %d failed, retrying in 5s", attempt+1)
		time.Sleep(5 * time.Second)
	}
	logger.Warn("boot-autoconnect: all tun attempts failed")
}
