package TCPServer

import (
	"encoding/binary"
	"io"
	"net"
	"sync"
	"testing"
	"time"
)

// newTestServer returns a Server with an empty clients map. DB and managers
// stay nil: the tests below never reach command dispatch, only Broadcast and
// connection lifecycle.
func newTestServer() *Server {
	return &Server{
		clients: make(map[string]*clientConn),
	}
}

// closeClient tears down a test clientConn deterministically: closing the out
// channel lets writeLoop exit its `for range` (production code never closes
// out, which is fine for a long-lived server but leaks goroutines in tests),
// and shutdown closes the underlying conn idempotently.
func closeClient(cc *clientConn) {
	if cc == nil {
		return
	}
	close(cc.out)
	cc.shutdown()
}

// drainingReader pulls everything the writeLoop writes to its pipe end and
// discards it, keeping the outbound queue from filling up (the "fast" client).
func drainingReader(t *testing.T, r net.Conn) {
	t.Helper()
	go func() {
		_, _ = io.Copy(io.Discard, r)
	}()
}

// readFramed reads one 4-byte-length-prefixed message from r. Returns the
// payload, or an error if the framing/timeout fails.
func readFramed(t *testing.T, r net.Conn, timeout time.Duration) ([]byte, bool) {
	t.Helper()
	_ = r.SetReadDeadline(time.Now().Add(timeout))
	lengthBuf := make([]byte, 4)
	if _, err := io.ReadFull(r, lengthBuf); err != nil {
		return nil, false
	}
	length := binary.BigEndian.Uint32(lengthBuf)
	payload := make([]byte, length)
	if _, err := io.ReadFull(r, payload); err != nil {
		return nil, false
	}
	return payload, true
}

// TestBroadcast_FramingExact verifies the writeLoop emits the documented wire
// format (4-byte big-endian length + payload) using a single reader, and that
// every broadcast to a draining client is delivered in order.
func TestBroadcast_FramingExact(t *testing.T) {
	s := newTestServer()
	a, b := net.Pipe()
	defer a.Close()
	defer b.Close()

	cc := newClientConn(a)
	defer closeClient(cc)
	s.clients["c1"] = cc

	payloads := [][]byte{[]byte(`{"k":"v"}`), []byte("hello"), []byte(`{"x":1}`)}
	for _, p := range payloads {
		s.Broadcast(p)
	}

	for _, want := range payloads {
		got, ok := readFramed(t, b, time.Second)
		if !ok {
			t.Fatal("did not receive a framed message")
		}
		if string(got) != string(want) {
			t.Fatalf("payload = %q, want %q", got, want)
		}
	}
}

// TestBroadcast_DropsSlowClient: a client whose outbound queue is full must be
// dropped by Broadcast (removed from map + shutdown conn) rather than blocking.
//
// We construct the clientConn WITHOUT starting its writeLoop and pre-fill out
// to exactly its capacity, so the "queue full" condition is deterministic
// rather than a race with the writeLoop draining into net.Pipe.
func TestBroadcast_DropsSlowClient(t *testing.T) {
	s := newTestServer()
	a, b := net.Pipe()
	defer b.Close()

	cc := &clientConn{conn: a, out: make(chan []byte, 64)} // no writeLoop goroutine
	s.clients["slow"] = cc

	// Fill the queue to capacity.
	for i := 0; i < cap(cc.out); i++ {
		cc.out <- []byte("x")
	}
	// The next Broadcast must find the queue full and drop the client.
	s.Broadcast([]byte("overflow"))

	s.mutex.Lock()
	_, present := s.clients["slow"]
	s.mutex.Unlock()
	if present {
		t.Fatal("expected slow client to be dropped from the map")
	}

	if !cc.closed {
		t.Fatal("expected dropped client conn to be closed (cc.closed)")
	}
}

// TestShutdown_Idempotent: concurrent shutdown calls on the same clientConn
// must not race or panic on a double-close (verified under -race).
func TestShutdown_Idempotent(t *testing.T) {
	a, _ := net.Pipe()
	defer a.Close()
	cc := newClientConn(a)
	defer closeClient(cc)

	const n = 50
	var wg sync.WaitGroup
	wg.Add(n)
	start := make(chan struct{})
	for i := 0; i < n; i++ {
		go func() {
			defer wg.Done()
			<-start
			cc.shutdown()
		}()
	}
	close(start)
	wg.Wait()

	if !cc.closed {
		t.Fatal("cc.closed = false, want true after shutdowns")
	}
}

// TestHandleConnection_DeferDeleteGuard exercises the cur == cc pointer guard
// (TCPServer.go:154): when a new clientConn reuses a clientID, the old
// connection's deferred cleanup must NOT delete the replacement from the map.
func TestHandleConnection_DeferDeleteGuard(t *testing.T) {
	s := newTestServer()

	// Two pipes; both will be registered under the same clientID "dup".
	a1, b1 := net.Pipe()
	a2, b2 := net.Pipe()
	defer b1.Close()
	defer b2.Close()

	cc1 := newClientConn(a1)
	cc2 := newClientConn(a2)
	defer closeClient(cc2)

	s.clients["dup"] = cc1
	handleReturned := make(chan struct{})
	go func() {
		s.handleConnection(cc1, "dup")
		close(handleReturned)
	}()

	// Ensure handleConnection is parked reading from cc1's conn.
	time.Sleep(50 * time.Millisecond)

	// Reuse the id with the replacement clientConn.
	s.mutex.Lock()
	s.clients["dup"] = cc2
	s.mutex.Unlock()

	// Closing b1 causes cc1's read (handleConnection) to hit io.EOF and return.
	b1.Close()

	select {
	case <-handleReturned:
	case <-time.After(time.Second):
		t.Fatal("handleConnection did not return after conn closed")
	}

	// The deferred cleanup must have left the replacement in place.
	s.mutex.Lock()
	cur, ok := s.clients["dup"]
	s.mutex.Unlock()
	if !ok || cur != cc2 {
		t.Fatalf("clients[dup] = (%p, %v), want cc2 (%p)", cur, ok, cc2)
	}

	// Stop cc1's writeLoop goroutine deterministically (its conn is already
	// closed by shutdown; closing out lets `for range` exit).
	closeClient(cc1)
}

// TestBroadcast_ConcurrentManyClients hammers Broadcast against many clients
// under the race detector. Mix of draining and non-draining clients to also
// exercise the slow-drop path concurrently.
func TestBroadcast_ConcurrentManyClients(t *testing.T) {
	s := newTestServer()

	const numClients = 32
	conns := make([]*clientConn, 0, numClients)
	for i := 0; i < numClients; i++ {
		a, b := net.Pipe()
		if i%2 == 0 {
			drainingReader(t, b) // fast clients: drain
		}
		// slow clients: never read → some get dropped under load
		cc := newClientConn(a)
		conns = append(conns, cc)
		id := "c" + itoa(i)
		s.mutex.Lock()
		s.clients[id] = cc
		s.mutex.Unlock()
	}
	// Tear down every clientConn so no writeLoop goroutine outlives the test.
	t.Cleanup(func() {
		for _, cc := range conns {
			closeClient(cc)
		}
	})

	var wg sync.WaitGroup
	const senders = 8
	wg.Add(senders)
	start := make(chan struct{})
	for i := 0; i < senders; i++ {
		go func(seed int) {
			defer wg.Done()
			<-start
			for j := 0; j < 100; j++ {
				s.Broadcast([]byte{byte(seed), byte(j)})
			}
		}(i)
	}
	close(start)
	wg.Wait()

	// No assertion on final client count — slow clients may legitimately be
	// dropped. The point is that -race finds no data races and nothing deadlocks.
}

// itoa is a tiny strconv-free int→string for test IDs.
func itoa(i int) string {
	if i == 0 {
		return "0"
	}
	var buf [20]byte
	pos := len(buf)
	for i > 0 {
		pos--
		buf[pos] = byte('0' + i%10)
		i /= 10
	}
	return string(buf[pos:])
}
