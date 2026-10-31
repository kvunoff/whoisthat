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

func main() {
	for _, arg := range os.Args[1:] {
		if arg == "--version" || arg == "-v" || arg == "version" || arg == "--help" || arg == "-h" {
			fmt.Printf("whoisthat-core v%s (protocol %d)\n", version.CoreVersion, version.ProtocolVersion)
			return
		}
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
	held, err := acquireInstanceLock()
	if err != nil {
		// The runtime dir may be unavailable; the socket probe below still
		// keeps us from stealing a live core's socket.
		logger.Warnf("instance lock unavailable: %v", err)
	} else if !held {
		logger.Info("another whoisthat-core is already running; exiting")
		return // exit 0: a non-zero code would loop under systemd Restart=on-failure
	}

	stop_sig := make(chan bool, 1)
	utils.RaiseAmbientCaps()
	appconfig.LoadConfig()

	// Cores from older builds hold no flock. Refuse to start while their
	// socket is live: this check runs before the reconcile so we never tear
	// down rules that a running daemon owns.
	if utils.SocketIsLive(appconfig.SocketPath()) {
		logger.Infof("another whoisthat-core is already serving %s; exiting", appconfig.SocketPath())
		return
	}

	// Self-heal: if no other core is already listening, tear down any leftover
	// firewall/routing rules from a previous core that exited without cleaning
	// up (hard quit, crash, power loss mid-TUN). Guarded on the IPC endpoint
	// being free so we never nuke a live core's rules.
	if !coreAlreadyRunning() {
		if err := tunmode.ReconcileOrphanedRules(appconfig.GetConfig().TunName); err != nil {
			logger.Warnf("startup reconcile: %v", err)
		} else {
			logger.Info("startup reconcile: orphaned rules cleared")
		}
	}

	geoDir := filepath.Join(configDir, "geo")
	if _, err := geo.EnsureAssets(geoDir); err != nil {
		logger.Warnf("main: geo assets not available: %v", err)
	}
	database := db.DB{}
	database.Initialize()
	// No unconditional SaveConfig here: writing the in-memory config on every
	// start used to turn a failed read into permanent data loss.
	proxy_manager := proxy.ProxyManager{}
	proxy_manager.DB = &database
	proxy_manager.Init()
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
	if xm.GetStatus().Status != "ready" {
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

	go bootAutoconnect(&database, &proxy_manager, &tun_manager, server)

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
			tunmode.RemoveKillSwitchBlock()
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
		os.Exit(0)
	}()
	select {}
}

// instanceLock stays open for the lifetime of the process. Closing it (or
// exiting) releases the flock so the next core can start.
var instanceLock *os.File

// acquireInstanceLock returns (false, nil) when another core already holds the
// lock, and a non-nil error only for real I/O failures.
func acquireInstanceLock() (bool, error) {
	dir := utils.RuntimeDir()
	if err := os.MkdirAll(dir, 0700); err != nil {
		return false, err
	}
	f, err := os.OpenFile(filepath.Join(dir, "core.lock"), os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return false, err
	}
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		f.Close()
		if errors.Is(err, syscall.EWOULDBLOCK) {
			return false, nil
		}
		return false, err
	}
	instanceLock = f
	return true, nil
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
	_ = os.Chmod(fallbackDir, 0700)
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
