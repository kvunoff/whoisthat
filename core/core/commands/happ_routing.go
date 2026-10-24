package cmd

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"sync"
	"time"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/happ"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	"whoisthat-core/structs"
)

var happRoutingMutex sync.Mutex

func (cmd *Cmd) ImportHappRouting(data structs.ImportHappRoutingData, proxy_manager *proxy.ProxyManager) {
	if len(data.Link) > 2*1024*1024 {
		cmd.warn("import-happ-routing-failed", "Routing link payload too large")
		return
	}

	happRoutingMutex.Lock()
	defer happRoutingMutex.Unlock()

	grp, err := cmd.DB.LoadGroupConfig(data.GroupId)
	if err != nil {
		cmd.warn("import-happ-routing-failed", "Group not found")
		return
	}

	rp, isOff, err := happ.UpsertFromLink(cmd.DB, data.GroupId, data.Link, time.Now().Unix())
	if err != nil {
		logger.Warnf("import-happ-routing: failed to import link: %v", err)
		cmd.warn("import-happ-routing-failed", fmt.Sprintf("Failed to import routing link: %v", err))
		return
	}

	tunName := appconfig.GetConfig().TunName
	isConnected := false
	if proxy_manager != nil {
		st := proxy_manager.GetStatus()
		isConnected = (st.Connection == "connected" && st.Profile.GroupId == data.GroupId)
	}

	if isOff {
		if isConnected && proxy_manager != nil {
			_ = proxy_manager.ReloadRouting(tunName)
		}
	} else if rp.State == "updating" {
		go func(gID int, profile structs.RoutingProfile) {
			ctx, cancel := context.WithTimeout(context.Background(), 7*time.Minute)
			defer cancel()
			err := happ.EnsureProfileGeo(ctx, cmd.DB, &profile)
			if err != nil {
				logger.Warnf("async geo download failed for profile %s: %v", profile.Name, err)
			}
			happRoutingMutex.Lock()
			defer happRoutingMutex.Unlock()
			g, err := cmd.DB.LoadGroupConfig(gID)
			if err == nil {
				if g.PendingActiveRoutingProfileId == profile.Id && profile.State == "ready" {
					_ = cmd.DB.SetActiveRoutingProfile(gID, profile.Id)
					g.ActiveRoutingProfileId = profile.Id
				}
				if proxy_manager != nil {
					st := proxy_manager.GetStatus()
					if st.Connection == "connected" && st.Profile.GroupId == gID && g.ActiveRoutingProfileId == profile.Id {
						_ = proxy_manager.ReloadRouting(appconfig.GetConfig().TunName)
					}
				}
				rProfiles, _ := cmd.DB.ListRoutingProfileInfos(gID, g.ActiveRoutingProfileId)
				gwp, _ := cmd.DB.GetGroupDataAndProfiles(gID)
				cmd.send("subscription-updated", structs.SubscriptionUpdated{
					GroupId:         gID,
					Group:           g,
					Profiles:        gwp.Profiles,
					RoutingProfiles: rProfiles,
				})
			}
		}(data.GroupId, rp)
	} else if rp.State == "ready" {
		if refGroup, err := cmd.DB.LoadGroupConfig(data.GroupId); err == nil {
			grp = refGroup
		}
		if isConnected && grp.ActiveRoutingProfileId == rp.Id && proxy_manager != nil {
			_ = proxy_manager.ReloadRouting(tunName)
		}
	}

	if refGroup, err := cmd.DB.LoadGroupConfig(data.GroupId); err == nil {
		grp = refGroup
	}

	rProfiles, _ := cmd.DB.ListRoutingProfileInfos(data.GroupId, grp.ActiveRoutingProfileId)
	gwp, _ := cmd.DB.GetGroupDataAndProfiles(data.GroupId)
	cmd.send("subscription-updated", structs.SubscriptionUpdated{
		GroupId:         data.GroupId,
		Group:           grp,
		Profiles:        gwp.Profiles,
		RoutingProfiles: rProfiles,
	})
}

func (cmd *Cmd) SetGroupRoutingMode(data structs.SetGroupRoutingModeData, proxy_manager *proxy.ProxyManager) {
	mode := strings.ToLower(strings.TrimSpace(data.Mode))
	if mode != "auto" && mode != "manual" && mode != "subscription" {
		cmd.warn("set-group-routing-mode-failed", "Invalid routing mode (must be auto, manual, or subscription)")
		return
	}

	happRoutingMutex.Lock()
	defer happRoutingMutex.Unlock()

	_, err := cmd.DB.LoadGroupConfig(data.GroupId)
	if err != nil {
		cmd.warn("set-group-routing-mode-failed", "Group not found")
		return
	}

	err = cmd.DB.SetGroupRoutingMode(data.GroupId, mode)
	if err != nil {
		logger.Warnf("set-group-routing-mode: %v", err)
		cmd.warn("set-group-routing-mode-failed", fmt.Sprintf("Failed to set routing mode: %v", err))
		return
	}

	if proxy_manager != nil {
		st := proxy_manager.GetStatus()
		if st.Connection == "connected" && st.Profile.GroupId == data.GroupId {
			_ = proxy_manager.ReloadRouting(appconfig.GetConfig().TunName)
		}
	}

	grp, err := cmd.DB.LoadGroupConfig(data.GroupId)
	if err == nil {
		cmd.send("group-updated", grp)
	}
}

func (cmd *Cmd) SetActiveRoutingProfile(data structs.SetActiveRoutingProfileData, proxy_manager *proxy.ProxyManager) {
	happRoutingMutex.Lock()
	defer happRoutingMutex.Unlock()

	_, err := cmd.DB.LoadGroupConfig(data.GroupId)
	if err != nil {
		cmd.warn("set-active-routing-profile-failed", "Group not found")
		return
	}

	targetID := data.ProfileId
	if targetID == "off" {
		targetID = ""
	}

	err = cmd.DB.SetActiveRoutingProfile(data.GroupId, targetID)
	if err != nil {
		logger.Warnf("set-active-routing-profile: %v", err)
		cmd.warn("set-active-routing-profile-failed", fmt.Sprintf("Failed to set active routing profile: %v", err))
		return
	}

	if proxy_manager != nil {
		st := proxy_manager.GetStatus()
		if st.Connection == "connected" && st.Profile.GroupId == data.GroupId {
			_ = proxy_manager.ReloadRouting(appconfig.GetConfig().TunName)
		}
	}

	grp, err := cmd.DB.LoadGroupConfig(data.GroupId)
	if err == nil {
		cmd.send("group-updated", grp)
	}

	if targetID != "" {
		if rp, err := cmd.DB.LoadRoutingProfile(data.GroupId, targetID); err == nil {
			var hp happ.HappProfile
			_ = json.Unmarshal(rp.RawJSON, &hp)
			if (hp.Geoipurl != "" || hp.Geositeurl != "") && (rp.State == "error" || rp.GeoUpdatedAt == 0) {
				go func(gID int, profile structs.RoutingProfile) {
					profile.State = "updating"
					profile.LastError = ""
					_ = cmd.DB.SaveRoutingProfile(profile)
					if g, err := cmd.DB.LoadGroupConfig(gID); err == nil {
						rProfiles, _ := cmd.DB.ListRoutingProfileInfos(gID, g.ActiveRoutingProfileId)
						gwp, _ := cmd.DB.GetGroupDataAndProfiles(gID)
						cmd.send("subscription-updated", structs.SubscriptionUpdated{
							GroupId:         gID,
							Group:           g,
							Profiles:        gwp.Profiles,
							RoutingProfiles: rProfiles,
						})
					}

					ctx, cancel := context.WithTimeout(context.Background(), 7*time.Minute)
					defer cancel()
					err := happ.EnsureProfileGeo(ctx, cmd.DB, &profile)
					if err != nil {
						logger.Warnf("async geo download failed for profile %s: %v", profile.Name, err)
					}
					happRoutingMutex.Lock()
					defer happRoutingMutex.Unlock()
					g, err := cmd.DB.LoadGroupConfig(gID)
					if err == nil {
						if proxy_manager != nil {
							st := proxy_manager.GetStatus()
							if st.Connection == "connected" && st.Profile.GroupId == gID && g.ActiveRoutingProfileId == profile.Id {
								_ = proxy_manager.ReloadRouting(appconfig.GetConfig().TunName)
							}
						}
						rProfiles, _ := cmd.DB.ListRoutingProfileInfos(gID, g.ActiveRoutingProfileId)
						gwp, _ := cmd.DB.GetGroupDataAndProfiles(gID)
						cmd.send("subscription-updated", structs.SubscriptionUpdated{
							GroupId:         gID,
							Group:           g,
							Profiles:        gwp.Profiles,
							RoutingProfiles: rProfiles,
						})
					}
				}(data.GroupId, rp)
			}
		}
	}
}

func (cmd *Cmd) DeleteRoutingProfile(data structs.DeleteRoutingProfileData, proxy_manager *proxy.ProxyManager) {
	happRoutingMutex.Lock()
	defer happRoutingMutex.Unlock()

	grp, err := cmd.DB.LoadGroupConfig(data.GroupId)
	if err != nil {
		cmd.warn("delete-routing-profile-failed", "Group not found")
		return
	}

	_, err = cmd.DB.LoadRoutingProfile(data.GroupId, data.ProfileId)
	if err != nil {
		cmd.warn("delete-routing-profile-failed", "Routing profile not found")
		return
	}

	wasActive := (grp.ActiveRoutingProfileId == data.ProfileId)

	err = cmd.DB.DeleteRoutingProfile(data.GroupId, data.ProfileId)
	if err != nil {
		logger.Warnf("delete-routing-profile: %v", err)
		cmd.warn("delete-routing-profile-failed", fmt.Sprintf("Failed to delete routing profile: %v", err))
		return
	}

	if wasActive && proxy_manager != nil {
		st := proxy_manager.GetStatus()
		if st.Connection == "connected" && st.Profile.GroupId == data.GroupId {
			_ = proxy_manager.ReloadRouting(appconfig.GetConfig().TunName)
		}
	}

	grp, err = cmd.DB.LoadGroupConfig(data.GroupId)
	if err == nil {
		rProfiles, _ := cmd.DB.ListRoutingProfileInfos(data.GroupId, grp.ActiveRoutingProfileId)
		gwp, _ := cmd.DB.GetGroupDataAndProfiles(data.GroupId)
		cmd.send("subscription-updated", structs.SubscriptionUpdated{
			GroupId:         data.GroupId,
			Group:           grp,
			Profiles:        gwp.Profiles,
			RoutingProfiles: rProfiles,
		})
	}
}
