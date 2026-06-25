package main

import (
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
	"fmt"
	lumberjack "gopkg.in/natefinch/lumberjack.v2"
	"log"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"
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
