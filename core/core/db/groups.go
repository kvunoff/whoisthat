package db

import (
	"errors"
	"fmt"
	"os"
	"sort"
	"strconv"
	"strings"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
)

// UpdateGroupAndProfiles replaces a subscription group's profile list while
// preserving profile IDs across refreshes. IDs are matched by the proxy URI
// (not by position), so reordering a provider's list or adding a profile does
// not invalidate IDs used by external scripts, bookmarks, or the active
// connection. The currently connected profile (keep_profile_id) is always kept
// verbatim, even if the provider removed its URI.
func (db *DB) UpdateGroupAndProfiles(group_id int, profiles []structs.DBAddProfileData, keep_profile_id int) ([]structs.Profile, error) {
	db.mu.Lock()
	defer db.mu.Unlock()

	group, err := db.getGroupDataAndProfiles(group_id)
	if err != nil {
		return nil, fmt.Errorf("error getting group: %w", err)
	}
	group_config := group.Group

	profiles_added := []structs.Profile{}
	byID := make(map[int]structs.Profile, len(group.Profiles))
	byURI := make(map[string]int, len(group.Profiles))
	lastID := group_config.LastId
	for _, p := range group.Profiles {
		byID[p.Id] = p
		uri := normalizeProfileURI(p.Uri)
		if _, exists := byURI[uri]; !exists {
			byURI[uri] = p.Id
		}
		if p.Id > lastID {
			lastID = p.Id
		}
	}

	keepURI := ""
	if keep_profile_id != 0 {
		if kept, ok := byID[keep_profile_id]; ok {
			keepURI = normalizeProfileURI(kept.Uri)
			profiles_added = append(profiles_added, kept)
		} else {
			keep_profile_id = 0
		}
	}

	usedIDs := make(map[int]bool, len(profiles)+1)
	newOrder := make([]int, 0, len(profiles)+1)
	if keep_profile_id != 0 {
		usedIDs[keep_profile_id] = true
		newOrder = append(newOrder, keep_profile_id)
	}

	seenURIs := make(map[string]bool, len(profiles))
	for _, data := range profiles {
		uriKey := normalizeProfileURI(data.Uri)
		if keep_profile_id != 0 && uriKey == keepURI {
			continue
		}
		if seenURIs[uriKey] {
			// Duplicate URI inside one subscription: keep both entries but
			// give the duplicate a fresh ID so neither overwrites the other.
			uriKey = uriKey + "\x00dup"
		}
		seenURIs[uriKey] = true

		id := 0
		if existingID, ok := byURI[uriKey]; ok && !usedIDs[existingID] {
			id = existingID
		} else {
			lastID++
			id = lastID
		}
		usedIDs[id] = true

		profile := structs.Profile{
			Id:       id,
			GroupId:  data.GroupId,
			NanoID:   data.NanoID,
			Name:     data.Name,
			Protocol: data.Protocol,
			Uri:      data.Uri,
			Host:     data.Host,
			Address:  data.Address,
		}
		// Keep latency results when the same URI is still present.
		if old, ok := byID[id]; ok && normalizeProfileURI(old.Uri) == normalizeProfileURI(data.Uri) {
			profile.TestResult = old.TestResult
			profile.TestedAt = old.TestedAt
			profile.LossPct = old.LossPct
			profile.JitterMs = old.JitterMs
		}

		if err := db.writeEncryptedJSON(db.GetProfileFilePath(group_id, id), profile); err != nil {
			return profiles_added, fmt.Errorf("failed to write profile %d: %w", id, err)
		}
		newOrder = append(newOrder, id)
		profiles_added = append(profiles_added, profile)
	}

	group_config.LastId = lastID
	group_config.ProfileOrder = newOrder
	if err := db.saveGroupConfig(group_config); err != nil {
		return profiles_added, err
	}

	// Only remove stale profile files after the new group config is durable.
	// If anything failed above, the previous subscription remains readable.
	for _, p := range group.Profiles {
		if !usedIDs[p.Id] {
			_ = os.Remove(db.GetProfileFilePath(group_id, p.Id))
		}
	}
	return profiles_added, nil
}

func normalizeProfileURI(uri string) string {
	return strings.TrimSpace(uri)
}

func (db *DB) updateGroup(group structs.Group) error {
	group_config_path := db.GetGroupConfigFilePath(group.Id)
	if err := db.writeEncryptedJSON(group_config_path, group); err != nil {
		return fmt.Errorf("failed to write %s: %w", group_config_path, err)
	}
	return nil
}

func (db *DB) SaveGroup(group structs.Group) error {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.updateGroup(group)
}

func (db *DB) GetAllGroupsAndProfiles() ([]structs.GroupWithProfiles, error) {
	db.mu.Lock()
	defer db.mu.Unlock()

	var groups_with_profiles []structs.GroupWithProfiles = []structs.GroupWithProfiles{}

	dir_path := db.GetGroupsDirPath()

	entries, err := os.ReadDir(dir_path)
	if err != nil {
		return groups_with_profiles, fmt.Errorf("Error reading directory: %w", err)
	}

	groupsMap := make(map[int]structs.GroupWithProfiles)
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		group_id, err := strconv.Atoi(entry.Name())
		if err != nil {
			continue
		}
		group, err := db.getGroupDataAndProfiles(group_id)
		if err != nil {
			// A group directory without group_config.json is a stale leftover
			// from an interrupted delete/import. It is not a loadable group, so
			// skip it silently instead of spamming the log on every state query.
			if errors.Is(err, os.ErrNotExist) {
				continue
			}
			logger.Warn("warning while gathering all groups:", err)
			continue
		}
		groupsMap[group_id] = group
	}

	db_config, _ := db.loadDBConfig()
	var effectiveOrder []int
	for _, gid := range db_config.GroupOrder {
		if g, exists := groupsMap[gid]; exists {
			groups_with_profiles = append(groups_with_profiles, g)
			effectiveOrder = append(effectiveOrder, gid)
			delete(groupsMap, gid)
		}
	}

	if len(groupsMap) > 0 {
		var remaining []int
		for gid := range groupsMap {
			remaining = append(remaining, gid)
		}
		sort.Ints(remaining)
		for _, gid := range remaining {
			groups_with_profiles = append(groups_with_profiles, groupsMap[gid])
			effectiveOrder = append(effectiveOrder, gid)
		}
	}

	return groups_with_profiles, nil
}

func (db *DB) GetGroupDataAndProfiles(groupID int) (structs.GroupWithProfiles, error) {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.getGroupDataAndProfiles(groupID)
}

func (db *DB) getGroupDataAndProfiles(group_id int) (structs.GroupWithProfiles, error) {
	var group_with_profiles structs.GroupWithProfiles = structs.GroupWithProfiles{
		Profiles: []structs.Profile{},
	}
	dir_path := db.GetGroupDirPath(group_id)
	group_config, err := db.loadGroupConfig(group_id)
	if err != nil {
		return group_with_profiles, err
	}
	group_with_profiles.Group = group_config

	entries, err := os.ReadDir(dir_path)
	if err != nil {
		return group_with_profiles, fmt.Errorf("Error reading directory: %w", err)
	}

	profilesMap := make(map[int]structs.Profile)
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".json") || entry.Name() == "group_config.json" {
			continue
		}
		profile_id_str, _ := strings.CutSuffix(entry.Name(), ".json")
		profile_id, err := strconv.Atoi(profile_id_str)
		if err != nil {
			continue
		}
		profile, err := db.getProfile(group_id, profile_id)
		if err != nil {
			continue
		}
		profilesMap[profile_id] = profile
	}

	var effectiveOrder []int
	for _, pid := range group_config.ProfileOrder {
		if p, exists := profilesMap[pid]; exists {
			group_with_profiles.Profiles = append(group_with_profiles.Profiles, p)
			effectiveOrder = append(effectiveOrder, pid)
			delete(profilesMap, pid)
		}
	}

	if len(profilesMap) > 0 {
		var remaining []int
		for pid := range profilesMap {
			remaining = append(remaining, pid)
		}
		sort.Ints(remaining)
		for _, pid := range remaining {
			group_with_profiles.Profiles = append(group_with_profiles.Profiles, profilesMap[pid])
			effectiveOrder = append(effectiveOrder, pid)
		}
	}

	group_with_profiles.Group.ProfileOrder = effectiveOrder
	routingProfiles, _ := db.listRoutingProfileInfos(group_id, group_config.ActiveRoutingProfileId)
	group_with_profiles.RoutingProfiles = routingProfiles
	return group_with_profiles, nil
}

func (db *DB) DeleteGroup(id int) error {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.deleteGroup(id)
}

func (db *DB) deleteGroup(id int) error {
	group_config_dir := db.GetGroupDirPath(id)
	err := os.RemoveAll(group_config_dir)
	if err != nil {
		return fmt.Errorf("Failed to delete group dir %w", err)
	}
	if db_config, err := db.loadDBConfig(); err == nil {
		var newOrder []int
		for _, gid := range db_config.GroupOrder {
			if gid != id {
				newOrder = append(newOrder, gid)
			}
		}
		db_config.GroupOrder = newOrder
		_ = db.saveDBConfig(db_config)
	}
	return nil
}

func (db *DB) AddGroup(name string, subscription_url string) (structs.GroupAdded, error) {
	var group_added structs.GroupAdded
	db.mu.Lock()
	defer db.mu.Unlock()
	db_config, err := db.loadDBConfig()
	if err != nil {
		return group_added, err
	}
	db_config.LastGroupId++
	group_id := db_config.LastGroupId
	db_config.GroupOrder = append(db_config.GroupOrder, group_id)
	err = db.saveDBConfig(db_config)
	if err != nil {
		return group_added, err
	}

	group_dir_path := db.GetGroupDirPath(group_id)
	group_config_path := db.GetGroupConfigFilePath(group_id)
	err = os.RemoveAll(group_dir_path)
	if err != nil {
		return group_added, err
	}

	err = os.MkdirAll(group_dir_path, 0700)
	if err != nil {
		return group_added, err
	}

	group := structs.Group{
		Id:              group_id,
		SubscriptionUrl: subscription_url,
		Name:            name,
		LastId:          0,
		ProfileOrder:    []int{},
	}

	if err := db.writeEncryptedJSON(group_config_path, group); err != nil {
		return group_added, fmt.Errorf("failed to write %s: %w", group_config_path, err)
	}

	group_added = structs.GroupAdded{
		Id:              group_id,
		Name:            name,
		SubscriptionUrl: subscription_url,
	}

	return group_added, nil
}

func (db *DB) ReorderProfiles(group_id int, profile_ids []int) error {
	db.mu.Lock()
	defer db.mu.Unlock()

	group_config, err := db.loadGroupConfig(group_id)
	if err != nil {
		return fmt.Errorf("failed to load group %d: %w", group_id, err)
	}

	group_config.ProfileOrder = profile_ids
	return db.saveGroupConfig(group_config)
}

func (db *DB) ReorderGroups(group_ids []int) error {
	db.mu.Lock()
	defer db.mu.Unlock()

	db_config, err := db.loadDBConfig()
	if err != nil {
		return fmt.Errorf("failed to load db config: %w", err)
	}

	db_config.GroupOrder = group_ids
	return db.saveDBConfig(db_config)
}

func (db *DB) MoveProfile(from_group_id int, to_group_id int, profile_id int) (structs.Profile, error) {
	db.mu.Lock()
	defer db.mu.Unlock()

	var moved structs.Profile
	if from_group_id == to_group_id {
		return moved, fmt.Errorf("source and destination groups are the same")
	}

	old_profile, err := db.getProfile(from_group_id, profile_id)
	if err != nil {
		return moved, fmt.Errorf("failed to get profile %d in group %d: %w", profile_id, from_group_id, err)
	}

	to_group, err := db.loadGroupConfig(to_group_id)
	if err != nil {
		return moved, fmt.Errorf("failed to load target group %d: %w", to_group_id, err)
	}
	to_group.LastId++
	new_id := to_group.LastId

	moved = old_profile
	moved.GroupId = to_group_id
	moved.Id = new_id

	new_path := db.GetProfileFilePath(to_group_id, new_id)
	if err := db.writeEncryptedJSON(new_path, moved); err != nil {
		return moved, fmt.Errorf("failed to write moved profile %s: %w", new_path, err)
	}

	to_group.ProfileOrder = append(to_group.ProfileOrder, new_id)
	if err := db.saveGroupConfig(to_group); err != nil {
		_ = os.Remove(new_path)
		return moved, fmt.Errorf("failed to save target group %d config: %w", to_group_id, err)
	}

	old_path := db.GetProfileFilePath(from_group_id, profile_id)
	_ = os.Remove(old_path)
	if from_group, err := db.loadGroupConfig(from_group_id); err == nil {
		var newOrder []int
		for _, pid := range from_group.ProfileOrder {
			if pid != profile_id {
				newOrder = append(newOrder, pid)
			}
		}
		from_group.ProfileOrder = newOrder
		_ = db.saveGroupConfig(from_group)
	}

	return moved, nil
}

func (db *DB) UpdateGroupConfig(group_id int, name string, subscription_url string) (structs.Group, error) {
	db.mu.Lock()
	defer db.mu.Unlock()

	group, err := db.loadGroupConfig(group_id)
	if err != nil {
		return group, fmt.Errorf("failed to load group %d: %w", group_id, err)
	}
	if name != "" {
		group.Name = name
	}
	if subscription_url != "" {
		group.SubscriptionUrl = subscription_url
	}
	err = db.updateGroup(group)
	return group, err
}

func (db *DB) LoadGroupConfig(id int) (structs.Group, error) {
	db.mu.Lock()
	defer db.mu.Unlock()

	return db.loadGroupConfig(id)
}

func (db *DB) loadGroupConfig(id int) (structs.Group, error) {
	var group_data structs.Group
	group_conf_file := db.GetGroupConfigFilePath(id)
	if err := db.readEncryptedJSON(group_conf_file, &group_data); err != nil {
		return group_data, fmt.Errorf("Failed to load group data for %d: %w", id, err)
	}
	return group_data, nil
}

func (db *DB) saveGroupConfig(group structs.Group) error {
	group_conf_file := db.GetGroupConfigFilePath(group.Id)
	if err := db.writeEncryptedJSON(group_conf_file, group); err != nil {
		// Don't Fatal: a transient write error shouldn't kill the running VPN.
		// Surface the error to the caller so it can propagate up to the TUI.
		logger.Errorf("failed to write group config %s: %v", group_conf_file, err)
		return fmt.Errorf("failed to write group config %s: %w", group_conf_file, err)
	}
	return nil
}
