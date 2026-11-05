package netmark

import (
	"net"
	"testing"
	"time"

	"golang.org/x/sys/unix"
)

func TestTransportIsConfigured(t *testing.T) {
	if Transport() == nil {
		t.Fatal("Transport() = nil")
	}
}

func TestDialerMarksSockets(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	defer listener.Close()

	conn, err := Dialer(time.Second).Dial("tcp", listener.Addr().String())
	if err != nil {
		t.Fatalf("dial: %v", err)
	}
	defer conn.Close()

	tcp, ok := conn.(*net.TCPConn)
	if !ok {
		t.Fatalf("connection is %T, want *net.TCPConn", conn)
	}
	raw, err := tcp.SyscallConn()
	if err != nil {
		t.Fatalf("SyscallConn: %v", err)
	}
	var mark int
	if err := raw.Control(func(fd uintptr) {
		mark, _ = unix.GetsockoptInt(int(fd), unix.SOL_SOCKET, unix.SO_MARK)
	}); err != nil {
		t.Fatalf("control: %v", err)
	}
	if mark != DirectMark {
		t.Skipf("SO_MARK not permitted here (mark=%d)", mark)
	}
}
