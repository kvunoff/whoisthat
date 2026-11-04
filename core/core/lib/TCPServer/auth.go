package TCPServer

import (
	"bufio"
	"crypto/subtle"
	"encoding/json"
	"fmt"
	"time"

	"whoisthat-core/lib"
	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/version"
	"whoisthat-core/structs"
)

// ipcAuthTimeout bounds how long a TCP client may take to send its hello.
const ipcAuthTimeout = 5 * time.Second

// authenticateTCP enforces the token handshake for the opt-in TCP transport.
// UDS clients are already gated by filesystem permissions and skip this.
func (s *Server) authenticateTCP(cc *clientConn, reader *bufio.Reader) bool {
	_ = cc.conn.SetReadDeadline(time.Now().Add(ipcAuthTimeout))
	defer cc.conn.SetReadDeadline(time.Time{})

	msg, err := readTCPMessage(reader)
	if err != nil {
		logger.Warn("TCP IPC: failed to read hello:", err)
		return false
	}
	if msg.Msg != "hello" {
		s.sendAuthError(cc, "expected hello handshake")
		return false
	}

	var hello structs.HelloData
	if err := json.Unmarshal(msg.Data, &hello); err != nil {
		s.sendAuthError(cc, "invalid hello payload")
		return false
	}
	if s.tcpToken == "" || subtle.ConstantTimeCompare([]byte(hello.Token), []byte(s.tcpToken)) != 1 {
		logger.Warnf("TCP IPC: rejected client %s: invalid token", cc.conn.RemoteAddr())
		s.sendAuthError(cc, "invalid authentication token")
		return false
	}
	if hello.ProtocolVersion != version.ProtocolVersion {
		s.sendAuthError(cc, fmt.Sprintf("protocol mismatch: server=%d client=%d", version.ProtocolVersion, hello.ProtocolVersion))
		return false
	}

	resp := lib.CreateJsonNotification("hello-ok", structs.HelloResponse{ProtocolVersion: version.ProtocolVersion})
	_ = cc.conn.SetWriteDeadline(time.Now().Add(time.Second))
	err = cc.writeFrame(resp)
	_ = cc.conn.SetWriteDeadline(time.Time{})
	return err == nil
}

func (s *Server) sendAuthError(cc *clientConn, reason string) {
	msg := lib.CreateJsonNotification("hello-error", structs.HelloResponse{Error: reason})
	_ = cc.conn.SetWriteDeadline(time.Now().Add(time.Second))
	_ = cc.writeFrame(msg)
	_ = cc.conn.SetWriteDeadline(time.Time{})
}
