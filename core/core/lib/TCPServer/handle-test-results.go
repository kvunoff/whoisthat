package TCPServer

import (
	"whoisthat-core/lib"
	"whoisthat-core/structs"
	"log"
)

func (s *Server) handleTestResults() {
	log.Println("listening to test results")
	for result := range s.proxy_manager.TestResultChannel {
		err := s.DB.UpdateProfile(result.Profile)
		if err != nil {
			continue
		}
		profile_updated := structs.ProfileUpdated{
			Profile: result.Profile,
		}
		s.BroadCast(lib.CreateJsonNotification("profile-updated", profile_updated))
	}
}
