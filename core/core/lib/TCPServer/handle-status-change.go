package TCPServer

import (
	"whoisthat-core/lib"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
)

func (s *Server) handleStatusChange() {
	for status := range s.proxy_manager.StatusChanged {
		logger.Info("Connection status:", status.Connection)
		s.BroadCast(lib.CreateJsonNotification("status-changed", status))
	}
}

func (s *Server) handleTunModeStatusChange() {
	for status := range s.tun_manager.StatusChanged {
		logger.Info("TUN mode:", status)
		s.BroadCast(lib.CreateJsonNotification("tun-status-changed", structs.TunStatus{IsEnabled: status}))
	}
}

func (s *Server) handleStatsChange() {
	for stats := range s.proxy_manager.StatsChanged {
		s.BroadCast(lib.CreateJsonNotification("traffic-stats", stats))
	}
}
