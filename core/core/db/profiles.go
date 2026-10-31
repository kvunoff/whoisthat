package db

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
)

// saveProfileXrayConfig persists the full provider configuration of a managed
// profile. It is encrypted like every other database file because it contains
// proxy credentials.
func (db *DB) saveProfileXrayConfig(groupID, profileID int, raw json.RawMessage) error {
	path := db.GetProfileXrayConfigPath(groupID, profileID)
	if err := os.MkdirAll(filepath.Dir(path), 0700); err != nil {
		return fmt.Errorf("create xray config dir: %w", err)
	}
	return db.writeEncryptedJSON(path, raw)
}

// LoadProfileXrayConfig reads the provider configuration of a managed profile.
func (db *DB) LoadProfileXrayConfig(groupID, profileID int) (json.RawMessage, error) {
	db.mu.Lock()
	defer db.mu.Unlock()

	var raw json.RawMessage
	if err := db.readEncryptedJSON(db.GetProfileXrayConfigPath(groupID, profileID), &raw); err != nil {
		return nil, err
	}
	return raw, nil
}

func (db *DB) deleteProfileXrayConfig(groupID, profileID int) {
	if err := os.Remove(db.GetProfileXrayConfigPath(groupID, profileID)); err != nil && !os.IsNotExist(err) {
		logger.Warnf("failed to remove xray config for profile %d in group %d: %v", profileID, groupID, err)
	}
}

func (db *DB) RenameProfile(profile structs.ProfileID, name string) (structs.Profile, error) {
	db.mu.Lock()
	defer db.mu.Unlock()

	profile_data, err := db.getProfile(profile.GroupId, profile.Id)
	if err != nil {
		return profile_data, err
	}

	profile_data.Name = name
	if err := db.updateProfile(profile_data); err != nil {
		return profile_data, err
	}
	return profile_data, nil
}

func (db *DB) UpdateProfile(profile structs.Profile) error {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.updateProfile(profile)
}

func (db *DB) updateProfile(profile structs.Profile) error {
	profile_config_path := db.GetProfileFilePath(profile.GroupId, profile.Id)
	old_profile_data, err := db.getProfile(profile.GroupId, profile.Id)
	if err != nil {
		return fmt.Errorf("update error: Profile does not exist gid: %d, id: %d: %w", profile.GroupId, profile.Id, err)
	}
	if old_profile_data.NanoID != profile.NanoID {
		return fmt.Errorf("update error gid: %d, id: %d: Profile nano id mismatch old: %s, new: %s", profile.GroupId, profile.Id, old_profile_data.NanoID, profile.NanoID)
	}

	if err := db.writeEncryptedJSON(profile_config_path, profile); err != nil {
		return fmt.Errorf("update error: failed to write %s: %w", profile_config_path, err)
	}
	return nil
}

func (db *DB) GetProfile(group_id int, id int) (structs.Profile, error) {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.getProfile(group_id, id)
}

func (db *DB) DeleteProfile(group_id int, id int) error {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.deleteProfile(group_id, id)
}

func (db *DB) deleteProfile(group_id int, id int) error {
	profile_config_path := db.GetProfileFilePath(group_id, id)
	err := os.Remove(profile_config_path)
	if err != nil {
		if os.IsNotExist(err) {
			logger.Warnf("Profile removal warning: %s did not exist", profile_config_path)
		} else {
			return fmt.Errorf("Failed to delete config file %w", err)
		}
	}
	db.deleteProfileXrayConfig(group_id, id)
	if group_config, err := db.loadGroupConfig(group_id); err == nil {
		var newOrder []int
		for _, pid := range group_config.ProfileOrder {
			if pid != id {
				newOrder = append(newOrder, pid)
			}
		}
		group_config.ProfileOrder = newOrder
		_ = db.saveGroupConfig(group_config)
	}
	return nil
}

func (db *DB) AddProfile(data structs.DBAddProfileData) (structs.Profile, error) {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.addProfile(data)
}

func (db *DB) getProfile(group_id int, id int) (structs.Profile, error) {
	var profile structs.Profile
	profile_config_path := db.GetProfileFilePath(group_id, id)
	if err := db.readEncryptedJSON(profile_config_path, &profile); err != nil {
		return profile, fmt.Errorf("Error reading profile config with gid: %d, id: %d: %w", group_id, id, err)
	}
	return profile, nil
}

func (db *DB) addProfile(data structs.DBAddProfileData) (structs.Profile, error) {
	var profile_added structs.Profile
	group_data, err := db.loadGroupConfig(data.GroupId)
	if err != nil {
		return profile_added, err
	}
	group_data.LastId++
	profile_id := group_data.LastId
	group_data.ProfileOrder = append(group_data.ProfileOrder, profile_id)
	err = db.saveGroupConfig(group_data)
	if err != nil {
		return profile_added, err
	}

	profile_path := db.GetProfileFilePath(group_data.Id, profile_id)
	err = os.Remove(db.GetProfileFilePath(group_data.Id, profile_id))
	if err != nil && !os.IsNotExist(err) {
		return profile_added, err
	}
	profile := structs.Profile{
		Id:          profile_id,
		GroupId:     data.GroupId,
		NanoID:      data.NanoID,
		Name:        data.Name,
		Protocol:    data.Protocol,
		Uri:         data.Uri,
		Host:        data.Host,
		Address:     data.Address,
		Port:        data.Port,
		Managed:     data.Managed && len(data.Config) > 0,
		ProviderKey: data.ProviderKey,
	}

	if err := db.writeEncryptedJSON(profile_path, profile); err != nil {
		return profile_added, fmt.Errorf("failed to write %s: %w", profile_path, err)
	}
	if profile.Managed {
		if err := db.saveProfileXrayConfig(data.GroupId, profile_id, data.Config); err != nil {
			return profile_added, fmt.Errorf("failed to write xray config %d: %w", profile_id, err)
		}
	}

	profile_added = structs.Profile{
		Uri:         data.Uri,
		GroupId:     data.GroupId,
		NanoID:      data.NanoID,
		Id:          profile_id,
		Protocol:    profile.Protocol,
		Name:        profile.Name,
		Host:        profile.Host,
		Address:     profile.Address,
		Port:        profile.Port,
		Managed:     profile.Managed,
		ProviderKey: profile.ProviderKey,
		TestResult:  0,
	}

	return profile_added, nil
}
