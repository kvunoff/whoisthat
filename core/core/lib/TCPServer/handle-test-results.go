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
			logger.Infof("test %s: %dms (%s://%s)",
				result.Profile.Name, result.Profile.TestResult,
				result.Profile.Protocol, result.Profile.Address)
		} else {
			logger.Warnf("test %s: failed (%s://%s)",
				result.Profile.Name, result.Profile.Protocol, result.Profile.Address)
		}
		profile_updated := structs.ProfileUpdated{
			Profile: result.Profile,
		}
		s.Broadcast(lib.CreateJsonNotification("profile-updated", profile_updated))
	}
}
