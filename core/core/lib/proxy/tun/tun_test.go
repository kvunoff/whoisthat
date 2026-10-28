package tunmode

import "testing"

// The CLI waits for tun-status-changed after disable-tun. When TUN is already
// off, Stop() must still answer instead of silently returning.
func TestStopNotifiesWhenAlreadyDisabled(t *testing.T) {
	var tm TunModeManager
	tm.Init()

	tm.Stop()

	select {
	case enabled := <-tm.StatusChanged:
		if enabled {
			t.Fatal("expected disabled status")
		}
	default:
		t.Fatal("Stop() emitted no status when TUN was already disabled")
	}
}
