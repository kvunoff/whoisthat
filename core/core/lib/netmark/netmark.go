// Package netmark marks sockets owned by the daemon with the direct-outbound
// firewall mark. The kill-switch allowlist accepts mark 1, so runtime, geo and
// subscription downloads keep working while the block is armed without
// opening anything for user traffic.
//
// Keep DirectMark in sync with tunmode.killSwitchDirectMark (the tun package
// cannot be imported from here without an import cycle).
package netmark

import (
	"net"
	"net/http"
	"syscall"
	"time"

	"golang.org/x/sys/unix"

	"whoisthat-core/lib/logger"
)

const DirectMark = 1

// Dialer returns a dialer whose sockets carry SO_MARK=DirectMark.
func Dialer(timeout time.Duration) *net.Dialer {
	return &net.Dialer{
		Timeout: timeout,
		Control: func(_, _ string, c syscall.RawConn) error {
			var sockErr error
			if err := c.Control(func(fd uintptr) {
				sockErr = unix.SetsockoptInt(int(fd), unix.SOL_SOCKET, unix.SO_MARK, DirectMark)
			}); err != nil {
				return err
			}
			if sockErr != nil {
				// No CAP_NET_ADMIN: keep the connection attempt unmarked. It
				// only fails when the kill-switch is armed, which is visible
				// in the download error.
				logger.Warnf("netmark: cannot set SO_MARK: %v", sockErr)
			}
			return nil
		},
	}
}

// Transport is the default HTTP transport with the direct mark applied.
func Transport() *http.Transport {
	transport := http.DefaultTransport.(*http.Transport).Clone()
	transport.DialContext = Dialer(30 * time.Second).DialContext
	return transport
}
