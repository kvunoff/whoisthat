package TCPServer

import (
	"bufio"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"sync"
	cmd "whoisthat-core/commands"
	"whoisthat-core/db"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
)

type Server struct {
	clients        map[string]*clientConn
	DB             *db.DB
	mutex          sync.Mutex
	proxy_manager  *proxy.ProxyManager
	tun_manager    *tunmode.TunModeManager
	stop_sig       chan<- bool
	missingBinaries []MissingBinary
}

// clientConn wraps a net.Conn with its own dedicated outbound goroutine fed
// by a buffered channel. This decouples Broadcast from slow/stuck clients:
// Broadcast just enqueues non-blockingly under the mutex and returns; the
// per-client goroutine does the actual blocking Write. A client whose recv
// buffer fills (e.g. the TUI's command connection that never reads) no
// longer stalls the whole server.
type clientConn struct {
	conn    net.Conn
	out     chan []byte
	closed  bool
	closeMu sync.Mutex
}

func newClientConn(conn net.Conn) *clientConn {
	c := &clientConn{
		conn: conn,
		out:  make(chan []byte, 64),
	}
	go c.writeLoop()
	return c
}

func (c *clientConn) writeLoop() {
	for msg := range c.out {
		length := make([]byte, 4)
		binary.BigEndian.PutUint32(length, uint32(len(msg)))
		if _, err := c.conn.Write(length); err != nil {
			c.shutdown()
			return
		}
		if _, err := c.conn.Write(msg); err != nil {
			c.shutdown()
			return
		}
	}
}

func (c *clientConn) shutdown() {
	c.closeMu.Lock()
	defer c.closeMu.Unlock()
	if !c.closed {
		c.closed = true
		c.conn.Close()
	}
}

func NewServer(database *db.DB, proxy_manager *proxy.ProxyManager, tun_manager *tunmode.TunModeManager, stop_sig chan<- bool, missingBinaries []MissingBinary) *Server {
	return &Server{
		DB:              database,
		clients:         make(map[string]*clientConn),
		proxy_manager:   proxy_manager,
		tun_manager:     tun_manager,
		stop_sig:        stop_sig,
		missingBinaries: missingBinaries,
	}
}

func (s *Server) Start() {
	app_config := appconfig.GetConfig()

	go s.handleTunModeStatusChange()
	go s.handleStatusChange()
	go s.handleTestResults()
	go s.handleStatsChange()

	// clientID must be unique per connection. UDS peers all report the same
	// (empty) RemoteAddr, so fall back to a monotonic counter when the address
	// is not distinguishing.
	var idSeq uint64
	accept := func(listener net.Listener, kind string) {
		for {
			conn, err := listener.Accept()
			if err != nil {
				logger.Warn("failed to accept connection:", err)
				continue
			}
			cc := newClientConn(conn)
			s.mutex.Lock()
			seq := idSeq
			idSeq++
			clientID := fmt.Sprintf("%s#%d:%s", kind, seq, conn.RemoteAddr())
			s.clients[clientID] = cc
			s.mutex.Unlock()
			go s.handleConnection(cc, clientID)
		}
	}

	// Unix domain socket — the secure default transport. Access is gated by
	// filesystem permissions (0600, user-owned dir), so only the owning user
	// can command the cap_net_admin-holding core.
	sockPath := appconfig.SocketPath()
	if err := os.MkdirAll(filepath.Dir(sockPath), 0700); err != nil {
		logger.Warn("failed to create socket dir:", err)
	}
	// Remove a stale socket left by a previous core. Safe: startup already
	// confirmed no live core is listening before we reach here.
	_ = os.Remove(sockPath)
	if uds, err := net.Listen("unix", sockPath); err != nil {
		logger.Fatal("failed to listen on unix socket:", err)
	} else {
		if err := os.Chmod(sockPath, 0600); err != nil {
			logger.Warn("failed to chmod socket:", err)
		}
		logger.Infof("listening on unix socket %s", sockPath)
		go accept(uds, "uds")
	}

	// Legacy TCP — opt-in only (remote/advanced use). Unauthenticated, so it
	// stays off unless the user explicitly enables it.
	if app_config.TCPEnabled {
		port := app_config.CoreTCPPort
		if listen4, err := net.Listen("tcp4", fmt.Sprintf("127.0.0.1:%d", port)); err != nil {
			logger.Warn("failed to listen on IPv4:", err)
		} else {
			go accept(listen4, "tcp4")
		}
		if listen6, err := net.Listen("tcp6", fmt.Sprintf("[::1]:%d", port)); err != nil {
			logger.Warn("failed to listen on IPv6:", err)
		} else {
			go accept(listen6, "tcp6")
		}
		logger.Infof("listening on tcp port %d (v4 + v6)", port)
	}
}

// Broadcast enqueues msg onto every client's outbound channel non-blockingly.
// A client whose channel is full (slow reader) is dropped — its goroutine
// will close the connection on the next failed write. We never block inside
// the server mutex, so a stuck client can't deadlock the rest of the system.
func (s *Server) Broadcast(msg []byte) {
	s.mutex.Lock()
	defer s.mutex.Unlock()

	for id, cc := range s.clients {
		select {
		case cc.out <- msg:
		default:
			// Client is too far behind — drop it. The writeLoop will detect
			// the closed channel / failed write and tear down the conn.
			logger.Warnf("dropping slow client %s (outbound queue full)", id)
			cc.shutdown()
			delete(s.clients, id)
		}
	}
}

func (s *Server) handleConnection(cc *clientConn, clientID string) {
	conn := cc.conn
	defer func() {
		cc.shutdown()
		s.mutex.Lock()
		if cur, ok := s.clients[clientID]; ok && cur == cc {
			delete(s.clients, clientID)
		}
		s.mutex.Unlock()
	}()

	// Pre-flight unicast: warn this newly-connected client about any external
	// binaries the core needs but couldn't find at startup. Sent directly to
	// cc.out (NOT Broadcast) so each new TUI gets the message exactly once on
	// connect — without it, the user has no idea why hy2/TUN tests silently
	// fail until they read core.log.
	if len(s.missingBinaries) > 0 {
		sendMissingBinaryWarnings(cc, s.missingBinaries)
	}

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

		case "test-group":
			var data structs.TestGroupData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			go command_handler.TestGroup(data, s.proxy_manager)

		case "cancel-tests":
			var data structs.CancelTestsData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.CancelTests(data, s.proxy_manager)

		case "set-test-config":
			var data structs.SetTestConfigData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.SetTestConfig(data, s.proxy_manager)

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

		case "set-split-tunnel":
			var data structs.SetSplitTunnelData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.SetSplitTunnel(data, s.tun_manager)

		case "set-autoconnect":
			var data structs.SetAutoconnectData
			if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
				logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
				return
			}
			command_handler.SetAutoconnect(data)

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
