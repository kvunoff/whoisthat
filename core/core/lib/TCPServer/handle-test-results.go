package TCPServer

import (
	"whoisthat-core/lib"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
)

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
