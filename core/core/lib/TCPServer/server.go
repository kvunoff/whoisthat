package TCPServer

import (
	"bufio"
	"encoding/binary"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"sync"

	cmd "whoisthat-core/commands"
	"whoisthat-core/db"
	"whoisthat-core/lib"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	tunmode "whoisthat-core/lib/proxy/tun"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

type Server struct {
	clients         map[string]*clientConn
	DB              *db.DB
	mutex           sync.Mutex
	proxy_manager   *proxy.ProxyManager
	tun_manager     *tunmode.TunModeManager
	stop_sig        chan<- bool
	missingBinaries []MissingBinary
	// tcpToken is the shared secret required to speak TCP IPC. Empty when
	// TCP is disabled or token setup failed (in which case TCP is not opened).
	tcpToken string
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
	// writeMu serialises writeLoop with the synchronous handshake responses
	// so a broadcast cannot interleave its frame bytes with hello-ok/error.
	writeMu sync.Mutex
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
		if err := c.writeFrame(msg); err != nil {
			c.shutdown()
			return
		}
	}
}

func (c *clientConn) writeFrame(msg []byte) error {
	length := make([]byte, 4)
	binary.BigEndian.PutUint32(length, uint32(len(msg)))

	c.writeMu.Lock()
	defer c.writeMu.Unlock()
	if _, err := c.conn.Write(length); err != nil {
		return err
	}
	_, err := c.conn.Write(msg)
	return err
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
	s.handleXrayStatus()

	// If the proxy subprocess dies on its own, the TUN device and firewall
	// state must not be left half-configured: clear TUN and, when enabled,
	// re-arm the kill-switch (fail closed) before the UI reconnects.
	s.proxy_manager.SetUnexpectedExitHandler(func(reason string) {
		if s.tun_manager.IsEnabledLocked() {
			s.tun_manager.Stop()
		}
		cfg := appconfig.GetConfig()
		if cfg.KillSwitchEnabled {
			proxyIPs := s.proxy_manager.GetProxyIPs()
			if err := tunmode.SetupKillSwitchBlock(proxyIPs, cfg.TunName, cfg.DnsServers); err != nil {
				logger.Warn("kill-switch: failed to re-arm after proxy exit:", err)
			} else {
				logger.Infof("kill-switch: re-armed after proxy exit, whitelisted IPs: %v", proxyIPs)
			}
		}
		s.Broadcast(lib.CreateJsonNotification("warn", structs.Warning{
			Key:     "proxy-exited",
			Content: reason + "; TUN state cleared",
		}))
	})

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
			go s.handleConnection(cc, clientID, kind != "uds")
		}
	}

	// Unix domain socket — the secure default transport. Access is gated by
	// filesystem permissions (0600, user-owned dir), so only the owning user
	// can command the cap_net_admin-holding core.
	sockPath := appconfig.SocketPath()
	if err := os.MkdirAll(filepath.Dir(sockPath), 0700); err != nil {
		logger.Warn("failed to create socket dir:", err)
	}
	// Remove a stale socket left by a previous core. A live one must never be
	// replaced: a core from an older build holds no flock, and stealing its
	// socket would leave two daemons fighting over the same TUN/firewall state.
	if utils.SocketIsLive(sockPath) {
		logger.Warn("refusing to replace a live core socket at ", sockPath)
		os.Exit(0) // this process is a redundant instance; nothing of ours exists yet
	}
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

	// Legacy TCP — opt-in only (remote/advanced use). Every TCP client must
	// prove knowledge of the per-user token before it can issue commands.
	if app_config.TCPEnabled {
		token, err := appconfig.EnsureIPCToken()
		if err != nil {
			logger.Error("TCP IPC disabled: cannot create authentication token:", err)
		} else {
			s.tcpToken = token
		}
	}
	if app_config.TCPEnabled && s.tcpToken != "" {
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

func (s *Server) handleConnection(cc *clientConn, clientID string, requireAuth bool) {
	conn := cc.conn
	defer func() {
		cc.shutdown()
		s.mutex.Lock()
		if cur, ok := s.clients[clientID]; ok && cur == cc {
			delete(s.clients, clientID)
		}
		s.mutex.Unlock()
	}()

	reader := bufio.NewReader(conn)
	if requireAuth && !s.authenticateTCP(cc, reader) {
		return
	}

	// Pre-flight unicast: warn this newly-connected client about any external
	// binaries the core needs but couldn't find at startup. Sent directly to
	// cc.out (NOT Broadcast) so each new TUI gets the message exactly once on
	// connect — without it, the user has no idea why hy2/TUN tests silently
	// fail until they read core.log.
	if len(s.missingBinaries) > 0 {
		sendMissingBinaryWarnings(cc, s.missingBinaries)
	}

	baseHandler := cmd.Cmd{DB: s.DB, Broadcast: s.Broadcast, TunManager: s.tun_manager}

	for {
		raw_tcp_message, err := readTCPMessage(reader)
		if err != nil {
			if err != io.EOF {
				logger.Warn("Failed to read message:", err)
			}
			return
		}

		// Each request gets its own copy: async commands (test-profile,
		// test-group, update-subscription) outlive this iteration and must not
		// read the id of a later request.
		handler := baseHandler
		handler.RequestID = raw_tcp_message.ID
		// Name and id only: payloads may contain subscription URIs/tokens.
		logger.Debugf("ipc: %s (id=%q) from %s", raw_tcp_message.Msg, raw_tcp_message.ID, conn.RemoteAddr())
		if !s.dispatch(handler, raw_tcp_message) {
			return
		}
	}
}
