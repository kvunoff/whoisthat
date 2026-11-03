package cmd

import (
	"whoisthat-core/lib/xrayjson"
	"whoisthat-core/structs"
)

// GetProfileDetails returns sanitized stream metadata for a managed JSON
// profile. Non-managed profiles and read failures return an empty details
// object so the TUI can keep its URI-based fallback.
func (cmd *Cmd) GetProfileDetails(data structs.ProfileID) {
	details := structs.ProfileDetails{GroupId: data.GroupId, Id: data.Id}
	profile, err := cmd.DB.GetProfile(data.GroupId, data.Id)
	if err == nil && profile.Managed {
		if raw, loadErr := cmd.DB.LoadProfileXrayConfig(data.GroupId, data.Id); loadErr == nil {
			extracted := xrayjson.StreamDetails(raw)
			extracted.GroupId = data.GroupId
			extracted.Id = data.Id
			details = extracted
		}
	}
	cmd.send("profile-details", details)
}
