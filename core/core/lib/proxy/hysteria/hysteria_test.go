package hysteria

import (
	"testing"
	"time"

	"whoisthat-core/utils"
)

func TestHysteriaStartStop(t *testing.T) {
	hy := &HysteriaCore{Exited: make(chan error, 1)}
	dummyYaml := []byte(`
server: 127.0.0.1:49999
auth: test
socks5:
  listen: 127.0.0.1:49998
`)
	if err := hy.Start(dummyYaml); err != nil {
		if _, binErr := utils.GetHysteriaBin(); binErr != nil {
			t.Skip("hysteria binary not found, skipping test")
		}
		t.Fatalf("Start failed: %v", err)
	}
	if !hy.IsRunning() {
		t.Fatalf("expected hysteria to be running")
	}
	time.Sleep(100 * time.Millisecond)
	if !hy.IsRunning() {
		t.Fatalf("hysteria exited prematurely")
	}
	hy.Stop()
	time.Sleep(50 * time.Millisecond)
	if hy.IsRunning() {
		t.Fatalf("expected hysteria to be stopped")
	}
}
