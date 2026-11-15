package TCPServer

import (
	"net"
	"os"
	"path/filepath"
	"testing"
)

func TestPeerUID(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "s")
	ln, err := net.ListenUnix("unix", &net.UnixAddr{Name: path, Net: "unix"})
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()

	accepted := make(chan *net.UnixConn, 1)
	go func() {
		conn, err := ln.AcceptUnix()
		if err == nil {
			accepted <- conn
		}
	}()

	client, err := net.DialUnix("unix", nil, &net.UnixAddr{Name: path, Net: "unix"})
	if err != nil {
		t.Fatal(err)
	}
	defer client.Close()

	server := <-accepted
	defer server.Close()

	uid, ok := peerUID(server)
	if !ok || uid != uint32(os.Getuid()) {
		t.Fatalf("peerUID = (%d,%v), want (%d,true)", uid, ok, os.Getuid())
	}
}

func TestPeerUIDOnPipe(t *testing.T) {
	a, b := net.Pipe()
	defer a.Close()
	defer b.Close()
	if _, ok := peerUID(a); ok {
		t.Fatal("a non-unix connection must not report a peer uid")
	}
}
