package cmd

import (
	"fmt"
	"whoisthat-core/db"
	"whoisthat-core/lib"
	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/proxy/mainproxy"
	"whoisthat-core/structs"
)

type Cmd struct {
	Broadcast func([]byte)
	DB        *db.DB
}

func (cmd *Cmd) DeleteGroup(data structs.DeleteGroupData) {
	err := cmd.DB.DeleteGroup(data.Id)
	if err != nil {
		logger.Warnf("failed to delete group %d: %s", data.Id, err.Error())
		cmd.warn("delete-group-failed", "there was an error deleting group")
		return
	}
	group_deleted := structs.GroupDeleted{
		Id: data.Id,
	}
	cmd.send("group-deleted", group_deleted)
}

func (cmd *Cmd) AddGroup(data structs.AddGroupData) {
	group_added, err := cmd.DB.AddGroup(data.Name, data.SubscriptionUrl)
	if err != nil {
		logger.Warnf("failed to add group: %s", err.Error())
		cmd.warn("add-group-failed", "there was an error adding group")
		return
	}
	cmd.send("group-added", group_added)
}

func (cmd *Cmd) AddProfiles(data structs.AddProfilesData) {
	var profiles_added structs.ProfilesAdded = lib.AddProfiles(cmd.DB, data)
	cmd.send("profiles-added", profiles_added)
}

func (cmd *Cmd) UpdateProfile(data structs.UpdateProfileData) {
	profile_data, err := cmd.DB.RenameProfile(data.Profile, data.Name)
	if err != nil {
		logger.Warnf("failed to update profile %d,%d: %s", data.Profile.GroupId, data.Profile.Id, err.Error())
		cmd.warn("update-profile-failed", "there was an error updating the profile")
		return
	}
	profile_updated := structs.ProfileUpdated{
		Profile: profile_data,
	}
	cmd.send("profile-updated", profile_updated)
}

func (cmd *Cmd) UpdateGroup(data structs.UpdateGroupData) {
	group, err := cmd.DB.UpdateGroupConfig(data.Id, data.Name, data.SubscriptionUrl)
	if err != nil {
		logger.Warnf("failed to update group %d: %s", data.Id, err.Error())
		cmd.warn("update-group-failed", "there was an error updating the group")
		return
	}
	cmd.send("group-updated", group)
}

func (cmd *Cmd) DeleteProfiles(data structs.DeleteProfilesData) {
	var deleted structs.ProfilesDeleted
	for _, profile := range data.Profiles {
		err := cmd.DB.DeleteProfile(profile.GroupId, profile.Id)
		if err == nil {
			deleted.DeletedProfiles = append(deleted.DeletedProfiles, profile)
		} else {
			logger.Warn("failed to delete profile:", err)
		}
	}
	cmd.send("profiles-deleted", deleted)
}

func (cmd *Cmd) ReorderProfiles(data structs.ReorderProfilesData) {
	err := cmd.DB.ReorderProfiles(data.GroupId, data.ProfileIds)
	if err != nil {
		logger.Warnf("failed to reorder profiles in group %d: %s", data.GroupId, err.Error())
		cmd.warn("reorder-profiles-failed", "there was an error reordering profiles")
		return
	}
	cmd.send("profiles-reordered", structs.ProfilesReordered{
		GroupId:    data.GroupId,
		ProfileIds: data.ProfileIds,
	})
}

func (cmd *Cmd) ReorderGroups(data structs.ReorderGroupsData) {
	err := cmd.DB.ReorderGroups(data.GroupIds)
	if err != nil {
		logger.Warnf("failed to reorder groups: %s", err.Error())
		cmd.warn("reorder-groups-failed", "there was an error reordering groups")
		return
	}
	cmd.send("groups-reordered", structs.GroupsReordered{
		GroupIds: data.GroupIds,
	})
}

func (cmd *Cmd) MoveProfile(data structs.MoveProfileData, pm *mainproxy.ProxyManager) {
	moved, err := cmd.DB.MoveProfile(data.FromGroupId, data.ToGroupId, data.ProfileId)
	if err != nil {
		logger.Warnf("failed to move profile %d from %d to %d: %s", data.ProfileId, data.FromGroupId, data.ToGroupId, err.Error())
		cmd.warn("move-profile-failed", fmt.Sprintf("failed to move profile: %s", err.Error()))
		return
	}
	if pm != nil {
		pm.UpdateMovedProfile(structs.ProfileID{GroupId: data.FromGroupId, Id: data.ProfileId}, moved)
	}
	cmd.send("profile-moved", structs.ProfileMoved{
		OldProfile: structs.ProfileID{GroupId: data.FromGroupId, Id: data.ProfileId},
		NewProfile: moved,
	})
}

func (cmd *Cmd) warn(key string, msg string) {
	cmd.Broadcast(lib.CreateJsonNotification("warn", structs.Warning{Key: key, Content: msg}))
}

func (cmd *Cmd) send(msg string, obj any) {
	cmd.Broadcast(lib.CreateJsonNotification(msg, obj))
}
