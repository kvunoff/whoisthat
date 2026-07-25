package TCPServer

import (
	"fmt"
	"sync"
	"time"
	"whoisthat-core/lib"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
)

// warnThrottleWindow is the minimum interval between two test-failed warn
// broadcasts that share the same FailReason text. Without this, a batch of 50
// hysteria2 profiles with no hysteria binary installed would flood the TUI
// status bar with 50 identical warnings in under a second — the user sees a
// stroboscopic flicker instead of a single actionable message. Distinct reason
// texts still pass through immediately (e.g. one "binary not found" plus one
// "listener did not bind" inside the same 5s window).
const warnThrottleWindow = 5 * time.Second

type warnThrottle struct {
	mu     sync.Mutex
	lastAt map[string]time.Time
}

var testFailThrottle = &warnThrottle{lastAt: map[string]time.Time{}}

// allow returns true if `reason` was last emitted more than warnThrottleWindow
// ago (or never). On true it also records the current time. Threadsafe.
func (t *warnThrottle) allow(reason string) bool {
	t.mu.Lock()
	defer t.mu.Unlock()
	now := time.Now()
	if last, ok := t.lastAt[reason]; ok && now.Sub(last) < warnThrottleWindow {
		return false
	}
	t.lastAt[reason] = now
	return true
}

func (s *Server) handleTestResults() {
	for result := range s.proxy_manager.TestResultChannel {
		err := s.DB.UpdateProfile(result.Profile)
		if err != nil {
			continue
		}
		if result.Success {
			logger.Infof("test %s: %dms (±%dms jitter, %d%% loss) (%s://%s)",
				result.Profile.Name, result.Profile.TestResult,
				result.Profile.JitterMs, result.Profile.LossPct,
				result.Profile.Protocol, result.Profile.Address)
		} else {
			logger.Warnf("test %s: failed after %d sample(s), %d%% loss (%s://%s)",
				result.Profile.Name, result.SampleCount, result.Profile.LossPct,
				result.Profile.Protocol, result.Profile.Address)
		}
		profile_updated := structs.ProfileUpdated{
			Profile: result.Profile,
		}
		s.Broadcast(lib.CreateJsonNotification("profile-updated", profile_updated))

		// Surface a test-failure diagnostic to the TUI as a warn broadcast
		// when the core knows why it failed. Throttle by reason text so a
		// 50-profile batch that fails for the same reason (e.g. missing
		// hysteria binary) emits at most one warn every 5s — not a wall of
		// 50 status-bar flashes.
		if !result.Success && result.FailReason != "" {
			content := fmt.Sprintf("%s: %s", result.Profile.Name, result.FailReason)
			if testFailThrottle.allow(result.FailReason) {
				s.Broadcast(lib.CreateJsonNotification("warn", structs.Warning{
					Key:     "test-failed",
					Content: content,
				}))
			} else {
				logger.Infof("test %s: suppressing duplicate test-failed warn (reason: %s)",
					result.Profile.Name, result.FailReason)
			}
		}

		// Track progress for the profile's group, if a batch is active.
		if n, total, ok := s.proxy_manager.IncrementTestProgress(result.Profile.GroupId); ok {
			s.Broadcast(lib.CreateJsonNotification("test-progress", structs.TestProgress{
				GroupId: result.Profile.GroupId,
				Tested:  n,
				Total:   total,
			}))
		}
	}
}