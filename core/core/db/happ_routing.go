package db

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"whoisthat-core/lib/happdns"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
)

func (db *DB) GetRoutingDir(groupID int) string {
	return filepath.Join(db.GetGroupDirPath(groupID), "routing")
}

func (db *DB) GetRoutingProfilesDir(groupID int) string {
	return filepath.Join(db.GetRoutingDir(groupID), "profiles")
}

func (db *DB) GetRoutingGeoDir(groupID int, profileID string) string {
	return filepath.Join(db.GetRoutingDir(groupID), "geo", profileID)
}

func (db *DB) GetRoutingProfileFilePath(groupID int, id string) string {
	return filepath.Join(db.GetRoutingProfilesDir(groupID), id+".json")
}

// dnsForProfile returns the stored DNS config, lazily decoding RawJSON for
// profiles imported before DNS support existed. A broken raw payload simply
// means "no DNS section": resolution must never fail because of it.
func dnsForProfile(p structs.RoutingProfile) *structs.HappDNSConfig {
	if p.DNS != nil {
		return p.DNS
	}
	if len(p.RawJSON) == 0 {
		return nil
	}
	cfg, _ := happdns.ParseRawJSON(p.RawJSON)
	return cfg
}

func (db *DB) saveRoutingProfile(p structs.RoutingProfile) error {
	dir := db.GetRoutingProfilesDir(p.GroupId)
	if err := os.MkdirAll(dir, 0700); err != nil {
		return fmt.Errorf("create routing profiles dir: %w", err)
	}
	path := db.GetRoutingProfileFilePath(p.GroupId, p.Id)
	return db.writeEncryptedJSON(path, p)
}

func (db *DB) SaveRoutingProfile(p structs.RoutingProfile) error {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.saveRoutingProfile(p)
}

func (db *DB) loadRoutingProfile(groupID int, id string) (structs.RoutingProfile, error) {
	var p structs.RoutingProfile
	path := db.GetRoutingProfileFilePath(groupID, id)
	if err := db.readEncryptedJSON(path, &p); err != nil {
		return p, err
	}
	return p, nil
}

func (db *DB) LoadRoutingProfile(groupID int, id string) (structs.RoutingProfile, error) {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.loadRoutingProfile(groupID, id)
}

func (db *DB) listRoutingProfiles(groupID int) ([]structs.RoutingProfile, error) {
	var profiles []structs.RoutingProfile
	dir := db.GetRoutingProfilesDir(groupID)
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return []structs.RoutingProfile{}, nil
		}
		return nil, fmt.Errorf("read routing profiles dir: %w", err)
	}

	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".json") {
			continue
		}
		id := strings.TrimSuffix(entry.Name(), ".json")
		p, err := db.loadRoutingProfile(groupID, id)
		if err != nil {
			logger.Warnf("failed to load routing profile %s in group %d: %v", id, groupID, err)
			continue
		}
		profiles = append(profiles, p)
	}

	sort.Slice(profiles, func(i, j int) bool {
		if profiles[i].CreatedAt != profiles[j].CreatedAt {
			return profiles[i].CreatedAt < profiles[j].CreatedAt
		}
		return profiles[i].Name < profiles[j].Name
	})

	return profiles, nil
}

func (db *DB) ListRoutingProfiles(groupID int) ([]structs.RoutingProfile, error) {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.listRoutingProfiles(groupID)
}

func (db *DB) listRoutingProfileInfos(groupID int, activeID string) ([]structs.RoutingProfileInfo, error) {
	profiles, err := db.listRoutingProfiles(groupID)
	if err != nil {
		return nil, err
	}
	var infos []structs.RoutingProfileInfo
	for _, p := range profiles {
		dns := dnsForProfile(p)
		info := structs.RoutingProfileInfo{
			Id:             p.Id,
			Name:           p.Name,
			State:          p.State,
			LastError:      p.LastError,
			Active:         p.Id == activeID,
			RulesCount:     len(p.Rules),
			GlobalProxy:    p.GlobalProxy,
			DomainStrategy: p.DomainStrategy,
			LastUpdated:    p.LastUpdated,
			GeoUpdatedAt:   p.GeoUpdatedAt,
		}
		if dns != nil {
			info.FakeDNSEnabled = dns.FakeDNSEnabled
			info.RemoteDNS = happdns.RemoteLabel(dns)
			info.DomesticDNS = happdns.DomesticLabel(dns)
		}
		infos = append(infos, info)
	}
	return infos, nil
}

func (db *DB) ListRoutingProfileInfos(groupID int, activeID string) ([]structs.RoutingProfileInfo, error) {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.listRoutingProfileInfos(groupID, activeID)
}

func (db *DB) findRoutingProfileByName(groupID int, name string) (structs.RoutingProfile, error) {
	profiles, err := db.listRoutingProfiles(groupID)
	if err != nil {
		return structs.RoutingProfile{}, err
	}
	// Exact match first
	for _, p := range profiles {
		if p.Name == name {
			return p, nil
		}
	}
	// Case-insensitive match fallback
	for _, p := range profiles {
		if strings.EqualFold(p.Name, name) {
			return p, nil
		}
	}
	return structs.RoutingProfile{}, os.ErrNotExist
}

func (db *DB) FindRoutingProfileByName(groupID int, name string) (structs.RoutingProfile, error) {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.findRoutingProfileByName(groupID, name)
}

func (db *DB) deleteRoutingProfile(groupID int, id string) error {
	filePath := db.GetRoutingProfileFilePath(groupID, id)
	_ = os.Remove(filePath)

	geoDir := db.GetRoutingGeoDir(groupID, id)
	_ = os.RemoveAll(geoDir)

	group, err := db.loadGroupConfig(groupID)
	if err == nil {
		changed := false
		if group.ActiveRoutingProfileId == id {
			group.ActiveRoutingProfileId = ""
			changed = true
		}
		if group.PendingActiveRoutingProfileId == id {
			group.PendingActiveRoutingProfileId = ""
			changed = true
		}
		if changed {
			_ = db.updateGroup(group)
		}
	}
	return nil
}

func (db *DB) DeleteRoutingProfile(groupID int, id string) error {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.deleteRoutingProfile(groupID, id)
}

func (db *DB) setActiveRoutingProfile(groupID int, id string) error {
	group, err := db.loadGroupConfig(groupID)
	if err != nil {
		return fmt.Errorf("load group %d: %w", groupID, err)
	}

	if id != "" {
		p, err := db.loadRoutingProfile(groupID, id)
		if err != nil {
			return fmt.Errorf("load routing profile %s: %w", id, err)
		}
		if p.GroupId != groupID {
			return fmt.Errorf("routing profile %s does not belong to group %d", id, groupID)
		}
		if len(p.Rules) == 0 {
			return errors.New("cannot activate routing profile with no rules")
		}
		if p.GeoDir != "" {
			absGeoDir := filepath.Join(db.Path, p.GeoDir)
			if _, err := os.Stat(filepath.Join(absGeoDir, "geoip.dat")); err != nil {
				return fmt.Errorf("geoip.dat missing in %s: %w", absGeoDir, err)
			}
			if _, err := os.Stat(filepath.Join(absGeoDir, "geosite.dat")); err != nil {
				return fmt.Errorf("geosite.dat missing in %s: %w", absGeoDir, err)
			}
		}
	}

	group.ActiveRoutingProfileId = id
	group.PendingActiveRoutingProfileId = ""
	return db.updateGroup(group)
}

func (db *DB) SetActiveRoutingProfile(groupID int, id string) error {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.setActiveRoutingProfile(groupID, id)
}

func (db *DB) setGroupRoutingMode(groupID int, mode string) error {
	mode = strings.ToLower(strings.TrimSpace(mode))
	if mode != "auto" && mode != "manual" && mode != "subscription" {
		return fmt.Errorf("invalid routing mode %q: must be auto, manual, or subscription", mode)
	}

	group, err := db.loadGroupConfig(groupID)
	if err != nil {
		return fmt.Errorf("load group %d: %w", groupID, err)
	}

	group.RoutingMode = mode
	return db.updateGroup(group)
}

func (db *DB) SetGroupRoutingMode(groupID int, mode string) error {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.setGroupRoutingMode(groupID, mode)
}

func (db *DB) resolveRouting(groupID int) (structs.RoutingSelection, error) {
	if groupID == 0 {
		cfg, err := db.loadRouting()
		return structs.RoutingSelection{
			Config: cfg,
			Source: "manual",
		}, err
	}

	group, err := db.loadGroupConfig(groupID)
	if err != nil {
		logger.Warnf("resolveRouting: failed to load group %d: %v, falling back to manual", groupID, err)
		cfg, _ := db.loadRouting()
		return structs.RoutingSelection{
			Config: cfg,
			Source: "manual",
		}, nil
	}

	mode := group.RoutingMode
	if mode == "" {
		mode = "auto"
	}

	if mode == "manual" {
		cfg, err := db.loadRouting()
		return structs.RoutingSelection{
			Config: cfg,
			Source: "manual",
		}, err
	}

	// For auto and subscription modes, inspect active profile
	if group.ActiveRoutingProfileId != "" {
		p, err := db.loadRoutingProfile(groupID, group.ActiveRoutingProfileId)
		if err == nil && p.GroupId == groupID && len(p.Rules) > 0 {
			absGeoDir := ""
			geoOK := true
			if p.GeoDir != "" {
				absGeoDir = filepath.Join(db.Path, p.GeoDir)
				if _, err := os.Stat(filepath.Join(absGeoDir, "geoip.dat")); err != nil {
					geoOK = false
				}
				if _, err := os.Stat(filepath.Join(absGeoDir, "geosite.dat")); err != nil {
					geoOK = false
				}
			}

			if geoOK {
				return structs.RoutingSelection{
					Config: structs.RoutingConfig{
						DomainStrategy: p.DomainStrategy,
						Rules:          p.Rules,
						DNS:            dnsForProfile(p),
					},
					Source:      "happ",
					ProfileId:   p.Id,
					GeoDir:      absGeoDir,
					GlobalProxy: p.GlobalProxy,
				}, nil
			} else {
				logger.Warnf("resolveRouting: profile %s geo assets missing in %s", p.Id, absGeoDir)
			}
		} else if err != nil {
			logger.Warnf("resolveRouting: failed to load active profile %s: %v", group.ActiveRoutingProfileId, err)
		}
	}

	// Fallback when active Happ profile is not available or not ready
	if mode == "auto" {
		cfg, err := db.loadRouting()
		return structs.RoutingSelection{
			Config: cfg,
			Source: "manual",
		}, err
	}

	// Strict subscription mode: manual rules must not be applied; apply LAN-only rule
	return structs.RoutingSelection{
		Config: structs.RoutingConfig{
			DomainStrategy: "IPIfNonMatch",
			Rules: []structs.RoutingRule{
				{
					Type:        "field",
					IP:          structs.LANRuleIP,
					OutboundTag: "direct",
					Enabled:     true,
				},
			},
		},
		Source:      "happ",
		GlobalProxy: true,
	}, nil
}

func (db *DB) ResolveRouting(groupID int) (structs.RoutingSelection, error) {
	db.mu.Lock()
	defer db.mu.Unlock()
	return db.resolveRouting(groupID)
}
