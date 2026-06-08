package cmd

import (
	"whoisthat-core/lib"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	"whoisthat-core/structs"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"
)

var subscription_mutex = sync.Mutex{}

func (cmd *Cmd) UpdateSubscription(data structs.UpdateSubscriptionData, proxy_manager *proxy.ProxyManager) {
	if subscription_mutex.TryLock() == false {
		cmd.warn("command-ignored", "update-subscription")
		return
	}
	defer subscription_mutex.Unlock()

	group, err := cmd.DB.LoadGroupConfig(data.GroupId)
	if err != nil {
		cmd.warn("update-subscription-failed", "Failed to load group config for updating subscription")
		return
	}

	subscription_content, resp, err := fetchSubscription(group)
	if err != nil {
		logger.Warnf("update-subscription: failed to GET %s: %v", group.SubscriptionUrl, err)
		cmd.warn("update-subscription-failed", fmt.Sprintf("Failed to get subscription: %v", err))
		return
	}

	// Parse subscription-userinfo header (traffic / expiry)
	parseSubUserInfo(resp, &group)
	group.SubLastUpdated = time.Now().Unix()
	cmd.DB.SaveGroup(group)

	db_profiles := lib.GetDBAddProfileDatasFromStr(subscription_content, data.GroupId)
	if len(db_profiles) == 0 {
		logger.Warnf("update-subscription: parsed 0 profiles from content (len=%d)", len(subscription_content))
		cmd.warn("update-subscription-failed", "No profiles found in subscription")
		return
	}
	status := proxy_manager.GetStatus()
	keep_profile_id := 0
	if status.Connection == "connected" && status.Profile.GroupId == data.GroupId {
		keep_profile_id = status.Profile.Id
	}

	profiles, err := cmd.DB.UpdateGroupAndProfiles(data.GroupId, db_profiles, keep_profile_id)
	if err != nil {
		logger.Warn("update-subscription: failed to update DB:", err)
		cmd.warn("update-subscription-failed", "Failed to add new subscription content to database")
		return
	}
	logger.Infof("subscription %s: %d profiles", group.Name, len(profiles))
	cmd.send("subscription-updated", structs.SubscriptionUpdated{GroupId: data.GroupId, Group: group, Profiles: profiles})

}

func fetchSubscription(group structs.Group) (string, *http.Response, error) {
	req, err := http.NewRequest("GET", group.SubscriptionUrl, nil)
	if err != nil {
		return "", nil, err
	}

	cfg := appconfig.GetConfig()
	if cfg.HwidEnabled && cfg.Hwid != "" {
		req.Header.Set("x-hwid", cfg.Hwid)
		req.Header.Set("x-device-os", appconfig.Platform())
		req.Header.Set("x-ver-os", appconfig.KernelVersion())
		req.Header.Set("x-device-model", appconfig.DistroModel())
	}
	req.Header.Set("user-agent", cfg.UserAgent)

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return "", nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != 200 {
		return "", nil, fmt.Errorf("HTTP %d", resp.StatusCode)
	}

	bodyBytes, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", nil, err
	}

	return string(bodyBytes), resp, nil
}

func parseSubUserInfo(resp *http.Response, group *structs.Group) {
	if resp == nil {
		return
	}
	hdr := resp.Header.Get("subscription-userinfo")
	if hdr == "" {
		return
	}
	for _, part := range strings.Split(hdr, ";") {
		part = strings.TrimSpace(part)
		kv := strings.SplitN(part, "=", 2)
		if len(kv) != 2 {
			continue
		}
		val, err := strconv.ParseInt(strings.TrimSpace(kv[1]), 10, 64)
		if err != nil {
			continue
		}
		switch strings.TrimSpace(kv[0]) {
		case "upload":
			group.SubUpload = val
		case "download":
			group.SubDownload = val
		case "total":
			group.SubTotal = val
		case "expire":
			group.SubExpires = val
		}
	}
}
