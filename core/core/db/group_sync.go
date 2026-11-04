package db

import (
	"fmt"
	"os"
	"strings"

	"whoisthat-core/structs"
)

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
	// byURI keeps every known id per URI in ProfileOrder, so duplicate URIs
	// inside one subscription get a stable id per occurrence instead of a new
	// id on every refresh. JSON-subscription profiles have no URI and are
	// matched by ProviderKey instead.
	byURI := make(map[string][]int, len(group.Profiles))
	byKey := make(map[string][]int, len(group.Profiles))
	lastID := group_config.LastId
	for _, p := range group.Profiles {
		byID[p.Id] = p
		if p.Managed {
			byKey[p.ProviderKey] = append(byKey[p.ProviderKey], p.Id)
		} else {
			uri := normalizeProfileURI(p.Uri)
			byURI[uri] = append(byURI[uri], p.Id)
		}
		if p.Id > lastID {
			lastID = p.Id
		}
	}

	keepURI := ""
	keepKey := ""
	if keep_profile_id != 0 {
		if kept, ok := byID[keep_profile_id]; ok {
			if kept.Managed {
				keepKey = kept.ProviderKey
			} else {
				keepURI = normalizeProfileURI(kept.Uri)
			}
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

	// nextID indexes into byURI/byKey: occurrences of the same identity in the
	// new subscription consume old ids in order, so the n-th duplicate keeps
	// the n-th old id across refreshes.
	nextID := make(map[string]int, len(profiles))
	nextKey := make(map[string]int, len(profiles))
	for _, data := range profiles {
		if keep_profile_id != 0 {
			if data.Managed && keepKey != "" && data.ProviderKey == keepKey {
				continue
			}
			if !data.Managed && keepKey == "" && normalizeProfileURI(data.Uri) == keepURI {
				continue
			}
		}

		id := 0
		if data.Managed {
			for id == 0 {
				candidates := byKey[data.ProviderKey]
				idx := nextKey[data.ProviderKey]
				if idx >= len(candidates) {
					break
				}
				nextKey[data.ProviderKey] = idx + 1
				if !usedIDs[candidates[idx]] {
					id = candidates[idx]
				}
			}
		} else {
			uriKey := normalizeProfileURI(data.Uri)
			for id == 0 {
				candidates := byURI[uriKey]
				idx := nextID[uriKey]
				if idx >= len(candidates) {
					break
				}
				nextID[uriKey] = idx + 1
				if !usedIDs[candidates[idx]] {
					id = candidates[idx]
				}
			}
		}
		if id == 0 {
			lastID++
			id = lastID
		}
		usedIDs[id] = true

		profile := structs.Profile{
			Id:          id,
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
		// Keep latency results when the same identity is still present.
		if old, ok := byID[id]; ok && sameProfileIdentity(old, data) {
			profile.TestResult = old.TestResult
			profile.TestedAt = old.TestedAt
			profile.LossPct = old.LossPct
			profile.JitterMs = old.JitterMs
		}

		if err := db.writeEncryptedJSON(db.GetProfileFilePath(group_id, id), profile); err != nil {
			return profiles_added, fmt.Errorf("failed to write profile %d: %w", id, err)
		}
		if profile.Managed {
			if err := db.saveProfileXrayConfig(group_id, id, data.Config); err != nil {
				return profiles_added, fmt.Errorf("failed to write xray config %d: %w", id, err)
			}
		} else {
			db.deleteProfileXrayConfig(group_id, id)
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
			db.deleteProfileXrayConfig(group_id, p.Id)
		}
	}
	return profiles_added, nil
}

func normalizeProfileURI(uri string) string {
	return strings.TrimSpace(uri)
}

func sameProfileIdentity(old structs.Profile, data structs.DBAddProfileData) bool {
	if old.Managed != data.Managed {
		return false
	}
	if data.Managed {
		return old.ProviderKey == data.ProviderKey
	}
	return normalizeProfileURI(old.Uri) == normalizeProfileURI(data.Uri)
}
