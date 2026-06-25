package TCPServer

import (
	"bufio"
	cmd "whoisthat-core/commands"
	"whoisthat-core/db"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"sync"
)

type Server struct {
	clients       map[string]net.Conn
	DB            *db.DB
	mutex         sync.Mutex
	proxy_manager *proxy.ProxyManager
	tun_manager   *tunmode.TunModeManager
	stop_sig      chan<- bool
}

func NewServer(database *db.DB, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager, stop_sig chan<- bool) *Server {
	return &Server{
		DB:            database,
		clients:       make(map[string]net.Conn),
		proxy_manager: proxy_manager,
		tun_manager:   tun_manager,
		stop_sig:      stop_sig,
	}
}

func (s *Server) Start() {
	app_config := appconfig.GetConfig()
	port := app_config.CoreTCPPort

	listen4, err := net.Listen("tcp4", fmt.Sprintf("127.0.0.1:%d", port))
	if err != nil {
		logger.Fatal("failed to listen on IPv4:", err)
	}

	listen6, err := net.Listen("tcp6", fmt.Sprintf("[::1]:%d", port))
	if err != nil {
		logger.Warn("failed to listen on IPv6:", err)
	}

	logger.Infof("listening on port %d (v4 + v6)", port)

	go s.handleTunModeStatusChange()
	go s.handleStatusChange()
	go s.handleTestResults()
	go s.handleStatsChange()

	accept := func(listener net.Listener) {
		for {
			conn, err := listener.Accept()
			if err != nil {
				logger.Warn("failed to accept connection:", err)
				continue
			}
			s.mutex.Lock()
			clientID := conn.RemoteAddr().String()
			s.clients[clientID] = conn
			s.mutex.Unlock()
			go s.handleConnection(conn, clientID)
		}
	}

	go accept(listen4)
	if listen6 != nil {
		go accept(listen6)
	}
}

func (s *Server) Broadcast(msg []byte) {
	s.mutex.Lock()
	defer s.mutex.Unlock()

	for _, conn := range s.clients {

		length := make([]byte, 4)
		binary.BigEndian.PutUint32(length, uint32(len(msg)))

		_, err := conn.Write(length)
		if err != nil {
			conn.Close()
			continue
		}
		_, err = conn.Write(msg)
		if err != nil {
			conn.Close()
			continue
		}
	}
}

func (s *Server) handleConnection(conn net.Conn, clientID string) {
	defer func() {
		conn.Close()
		s.mutex.Lock()
		delete(s.clients, clientID)
		s.mutex.Unlock()
	}()

	command_handler := cmd.Cmd{DB: s.DB, Broadcast: s.Broadcast}
	reader := bufio.NewReader(conn)

	for {
		lengthBuf := make([]byte, 4)
		_, err := io.ReadFull(reader, lengthBuf)

		if err != nil {
			if err != io.EOF {
				logger.Warn("Failed to read length:", err)
			}
			return
		}

		length := binary.BigEndian.Uint32(lengthBuf)
		if length == 0 || length > 100*1024*1024 {
			logger.Warn("Invalid length", length)
			return
		}

		payload := make([]byte, length)

		_, err = io.ReadFull(reader, payload)

		if err != nil {
			logger.Warn("Failed to read the payload:", err)
			return
		}

		var raw_tcp_message structs.TCPMessage

		if err := json.Unmarshal(payload, &raw_tcp_message); err != nil {
			logger.Warn("Invalid JSON:", err)
			return
		}

		switch raw_tcp_message.Msg {
		case "die":
			s.stop_sig <- true
		case "add-profiles":
			var data structs.AddProfilesData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.AddProfiles(data)

		case "delete-profiles":
			var data structs.DeleteProfilesData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.DeleteProfiles(data)

		case "add-group":
			var data structs.AddGroupData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.AddGroup(data)

		case "delete-group":
			var data structs.DeleteGroupData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.DeleteGroup(data)

		case "connect":
			var data structs.ConnectData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.Connect(data, s.proxy_manager, s.tun_manager)

		case "disconnect":
			var data structs.DisconnectData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.Disconnect(data, s.proxy_manager, s.tun_manager)

		case "test-profile":
			var data structs.TestProfileData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			go command_handler.TestProfile(data, s.proxy_manager)

		case "get-application-state":
			var data structs.GetApplicationStateData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.GetApplicationState(data, s.proxy_manager, s.tun_manager)

		case "update-subscription":
			var data structs.UpdateSubscriptionData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			go command_handler.UpdateSubscription(data, s.proxy_manager)

		case "enable-tun":
			var data structs.EnableTunData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.EnableTun(data, s.proxy_manager, s.tun_manager)

		case "disable-tun":
			var data structs.DisableTunData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.DisableTun(data, s.tun_manager)

		case "is-root":
			var data structs.IsRootData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.IsRoot(data)

		case "set-tun-name":
			var data structs.SetTunNameData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.SetTunName(data)

		case "set-hwid":
			var data structs.SetHwidData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.SetHwid(data)

		case "update-profile":
			var data structs.UpdateProfileData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.UpdateProfile(data)

		case "update-group":
			var data structs.UpdateGroupData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.UpdateGroup(data)

		case "set-kill-switch":
			var data structs.SetKillSwitchData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.SetKillSwitch(data)

		case "get-routing":
			var data structs.GetRoutingData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.GetRouting(data)

		case "update-routing":
			var data structs.UpdateRoutingData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.UpdateRouting(data)

		default:
			logger.Warn("Unknown message:", raw_tcp_message.Msg)
		}
	}
}
