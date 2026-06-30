package main

import (
	"fmt"
	lumberjack "gopkg.in/natefinch/lumberjack.v2"
	"log"
	"os"
	"os/signal"
	"path/filepath"
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
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

func main() {
	configDir, err := os.UserConfigDir()
	if err != nil {
		configDir = filepath.Join(os.Getenv("HOME"), ".config")
	}
	logPath := filepath.Join(configDir, "whoisthat", "core.log")
	if err := os.MkdirAll(filepath.Dir(logPath), 0755); err != nil {
		logPath = "/tmp/whoisthat-core.log"
	}

	log.SetOutput(&lumberjack.Logger{
		Filename:   logPath,
		MaxSize:    20,
		MaxBackups: 1,
		MaxAge:     0,
		Compress:   false,
	})

	log.SetFlags(log.Ltime)
	logger.SetLevel(os.Getenv("WHOISTHAT_LOG_LEVEL"))
	logger.Info("whoisthat-core starting")

	stop_sig := make(chan bool, 1)
	utils.RaiseAmbientCaps()
	appconfig.LoadConfig()
	geoDir := filepath.Join(configDir, "whoisthat", "geo")
	if _, err := geo.EnsureAssets(geoDir); err != nil {
		logger.Warnf("main: geo assets not available: %v", err)
	}
	database := db.DB{}
	database.Initialize()
	appconfig.SaveConfig()
	proxy_manager := proxy.ProxyManager{}
	proxy_manager.DB = &database
	proxy_manager.Init()
	tun_manager := tunmode.TunModeManager{}
	tun_manager.Init()

	server := TCPServer.NewServer(&database, &proxy_manager, &tun_manager, stop_sig)
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
		tunmode.RemoveKillSwitchBlock()
		proxy_manager.Stop()
		tun_manager.Stop()
		server.Broadcast(lib.CreateJsonNotification("warn", structs.Warning{Key: "died", Content: reason}))
		os.Exit(0)
	}()
	select {}
}

func bootAutoconnect(database *db.DB, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager, server *TCPServer.Server) {
	cfg := appconfig.GetConfig()
	if !cfg.AutoconnectEnabled || cfg.AutoconnectProfileId == 0 {
		return
	}

	time.Sleep(5 * time.Second)

	command_handler := cmd.Cmd{DB: database, Broadcast: server.Broadcast}
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
		command_handler.EnableTunForProfile(profile, tun_manager)
		if tun_manager.IsEnabledLocked() {
			logger.Info("boot-autoconnect: tun mode enabled")
			return
		}
		logger.Warnf("boot-autoconnect: tun attempt %d failed, retrying in 5s", attempt+1)
		time.Sleep(5 * time.Second)
	}
	logger.Warn("boot-autoconnect: all tun attempts failed")
}
