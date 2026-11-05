package cmd

import (
	"context"
	"encoding/base64"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"
	"unicode"
	"whoisthat-core/db"
	"whoisthat-core/lib"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/happ"
	"whoisthat-core/lib/logger"
	"whoisthat-core/lib/netmark"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	"whoisthat-core/lib/xrayjson"
	"whoisthat-core/structs"
	"whoisthat-core/utils"
)

var subscription_mutex = sync.Mutex{}

var (
	userinfoRe = regexp.MustCompile(`://[^/\s@]+@`)
	queryRe    = regexp.MustCompile(`\?[^\s"']*`)
)

// redactSecret removes credentials from a URL before it reaches a log line or
// a notification: provider tokens live in the query string or in userinfo.
func redactSecret(s string) string {
	s = userinfoRe.ReplaceAllString(s, "://…@")
	return queryRe.ReplaceAllString(s, "?…")
}

// UpdateSubscription is the manual/foreground refresh path. It blocks on the
// subscription lock so a CLI `sub refresh` or a just-added group is never
// dropped because the background scheduler happens to be fetching.
func (cmd *Cmd) UpdateSubscription(data structs.UpdateSubscriptionData, proxy_manager *proxy.ProxyManager) {
	subscription_mutex.Lock()
	defer subscription_mutex.Unlock()
	cmd.updateSubscriptionLocked(data, proxy_manager, false)
}

// AutoUpdateSubscription is the background path. It never waits for a manual
// update, re-checks the group under the lock, and returns whether an attempt
// was made so the scheduler can drive retries/backoff.
func (cmd *Cmd) AutoUpdateSubscription(data structs.UpdateSubscriptionData, proxy_manager *proxy.ProxyManager, force bool) (ok bool, attempted bool) {
	if !subscription_mutex.TryLock() {
		return true, false
	}
	defer subscription_mutex.Unlock()

	group, err := cmd.DB.LoadGroupConfig(data.GroupId)
	if err != nil {
		logger.Warnf("auto-update: cannot load group %d: %v", data.GroupId, err)
		return false, true
	}
	if group.SubscriptionUrl == "" || group.SubAutoUpdateDisabled {
		return true, false
	}
	if force {
		// A manual update may have learned the interval while this target sat
		// in the probe queue.
		if effectiveUpdateInterval(group) > 0 {
			return true, false
		}
	} else if !subscriptionDue(group, time.Now()) {
		return true, false
	}
	return cmd.updateSubscriptionLocked(data, proxy_manager, true), true
}

func (cmd *Cmd) updateSubscriptionLocked(data structs.UpdateSubscriptionData, proxy_manager *proxy.ProxyManager, background bool) bool {
	group, err := cmd.DB.LoadGroupConfig(data.GroupId)
	if err != nil {
		cmd.warn("update-subscription-failed", "Failed to load group config for updating subscription")
		return false
	}

	subscription_content, resp, isJSON, err := fetchSubscription(group)
	if err != nil {
		logger.Warnf("update-subscription: failed to GET %s: %s", redactSecret(group.SubscriptionUrl), redactSecret(err.Error()))
		cmd.warn("update-subscription-failed", "Failed to get subscription")
		return false
	}

	// Extract Happ routing links from response headers and body. A JSON
	// subscription still uses its routing header for custom geo assets.
	happLinks := happ.ExtractLinksFromResponse(resp)
	happLinks = append(happLinks, happ.ExtractLinks([]byte(subscription_content))...)

	db_profiles := lib.GetDBAddProfileDatasFromStr(subscription_content, data.GroupId)
	format := "uri"
	if isJSON {
		format = "json"
	}
	for _, profile := range db_profiles {
		if profile.Managed {
			format = "json"
			break
		}
	}

	// Persist subscription-derived metadata against a fresh group snapshot.
	// Never save the pre-fetch struct: a concurrent rename, auto-update toggle
	// or routing-mode change must not be overwritten.
	fields := subscriptionFieldsFromResponse(resp, subscription_content, group.SubscriptionUrl, format)
	if len(db_profiles) == 0 {
		// An empty body says nothing about the subscription format.
		fields.Format = nil
	}
	group, err = cmd.DB.MergeSubscriptionFields(data.GroupId, fields)
	if err != nil {
		logger.Warn("update-subscription: failed to save subscription metadata:", err)
		cmd.warn("update-subscription-failed", "Failed to save subscription metadata")
		return false
	}

	if len(db_profiles) == 0 {
		logger.Warnf("update-subscription: parsed 0 profiles from content (len=%d)", len(subscription_content))
		cmd.warn("update-subscription-failed", "No profiles found in subscription")
		return false
	}

	keep_profile_id := 0
	if proxy_manager != nil {
		status := proxy_manager.GetStatus()
		if status.Connection == "connected" && status.Profile.GroupId == data.GroupId {
			keep_profile_id = status.Profile.Id
		}
	}

	profiles, err := cmd.DB.UpdateGroupAndProfiles(data.GroupId, db_profiles, keep_profile_id)
	if err != nil {
		logger.Warn("update-subscription: failed to update DB:", err)
		cmd.warn("update-subscription-failed", "Failed to add new subscription content to database")
		return false
	}

	// The timestamp means "last successful profile merge", so it moves only
	// after the profiles are durable.
	now := time.Now().Unix()
	group, err = cmd.DB.MergeSubscriptionFields(data.GroupId, db.SubscriptionFields{LastUpdated: &now})
	if err != nil {
		logger.Warn("update-subscription: failed to save last-updated timestamp:", err)
		cmd.warn("update-subscription-failed", "Failed to save subscription metadata")
		return false
	}

	// Upsert extracted Happ routing profiles
	var profilesNeedingGeo []structs.RoutingProfile
	happRoutingMutex.Lock()
	for _, link := range happLinks {
		rp, isOff, err := happ.UpsertFromLink(cmd.DB, data.GroupId, link, now)
		if err != nil {
			logger.Warnf("update-subscription: failed to import happ routing link: %v", err)
			continue
		}
		if !isOff && rp.State == "updating" {
			profilesNeedingGeo = append(profilesNeedingGeo, rp)
		}
	}
	happRoutingMutex.Unlock()

	// Refresh group configuration after Happ link updates
	if refreshedGroup, err := cmd.DB.LoadGroupConfig(data.GroupId); err == nil {
		group = refreshedGroup
	}

	routingProfiles, _ := cmd.DB.ListRoutingProfileInfos(data.GroupId, group.ActiveRoutingProfileId)
	if background {
		logger.Debugf("auto-update: subscription %s: %d profiles, %d routing profiles", group.Name, len(profiles), len(routingProfiles))
	} else {
		logger.Infof("subscription %s: %d profiles, %d routing profiles", group.Name, len(profiles), len(routingProfiles))
	}
	cmd.send("subscription-updated", structs.SubscriptionUpdated{
		GroupId:         data.GroupId,
		Group:           group,
		Profiles:        profiles,
		RoutingProfiles: routingProfiles,
		Auto:            background,
	})

	if !background && proxy_manager != nil {
		st := proxy_manager.GetStatus()
		if st.Connection == "connected" && st.Profile.GroupId == data.GroupId {
			cmd.refreshDnsHijackFor(st.Profile)
		}
	}

	if len(profilesNeedingGeo) > 0 {
		go func(gID int, profs []structs.RoutingProfile, bg bool) {
			for _, p := range profs {
				ctx, cancel := context.WithTimeout(context.Background(), 7*time.Minute)
				err := happ.EnsureProfileGeo(ctx, cmd.DB, &p)
				cancel()
				if err != nil {
					logger.Warnf("async geo download failed for profile %s: %v", p.Name, err)
				}
				happRoutingMutex.Lock()
				grp, err := cmd.DB.LoadGroupConfig(gID)
				if err == nil {
					if grp.PendingActiveRoutingProfileId == p.Id && p.State == "ready" {
						_ = cmd.DB.SetActiveRoutingProfile(gID, p.Id)
						grp.ActiveRoutingProfileId = p.Id
					}
					// A background refresh must not reconfigure a live session;
					// the new routing profile applies on the next connect.
					if !bg && proxy_manager != nil {
						st := proxy_manager.GetStatus()
						if st.Connection == "connected" && st.Profile.GroupId == gID && grp.ActiveRoutingProfileId == p.Id {
							_ = proxy_manager.ReloadRouting(appconfig.GetConfig().TunName)
							cmd.refreshDnsHijackFor(st.Profile)
						}
					}
					rProfiles, _ := cmd.DB.ListRoutingProfileInfos(gID, grp.ActiveRoutingProfileId)
					groupWithProfiles, _ := cmd.DB.GetGroupDataAndProfiles(gID)
					cmd.send("subscription-updated", structs.SubscriptionUpdated{
						GroupId:         gID,
						Group:           grp,
						Profiles:        groupWithProfiles.Profiles,
						RoutingProfiles: rProfiles,
						Auto:            bg,
					})
				}
				happRoutingMutex.Unlock()
			}
		}(data.GroupId, profilesNeedingGeo, background)
	}

	// Auto-test the freshly-imported profiles so users see live latencies
	// immediately after `u`. Skip for background refreshes so the network is
	// not put under test load, and skip large subscriptions to avoid
	// port-pool exhaustion on slow machines.
	ac := appconfig.GetConfig()
	if !background && proxy_manager != nil && ac.TestConfig.AutoTestOnSub && len(profiles) > 0 && len(profiles) <= 50 {
		method := "http-get"
		total := proxy_manager.TestGroup(profiles, method, 0)
		proxy_manager.SeedTestProgress(data.GroupId, total)
	}
	return true
}

// maxSubBytes caps how much we'll read from a subscription endpoint. A
// legit subscription is a few hundred KB of base64 URIs; 16 MB is way
// beyond any reasonable subscription and protects the core from OOM via
// a hostile/misconfigured server.
const maxSubBytes = 16 * 1024 * 1024

// subClient is a dedicated HTTP client with a sane timeout so a hung or
// slow subscription server can't leak a goroutine forever.
var subClient = &http.Client{Timeout: 30 * time.Second, Transport: netmark.Transport()}

// fetchSubscription prefers the provider's JSON form (<url>/json) so each
// server keeps its own provider-owned routing. When that endpoint is missing or
// does not return Xray configurations, the regular share-link list is used.
func fetchSubscription(group structs.Group) (string, *http.Response, bool, error) {
	if jsonURL, ok := subscriptionJSONURL(group.SubscriptionUrl); ok {
		if body, resp, err := fetchSubscriptionURL(jsonURL); err == nil && xrayjson.IsConfigBody([]byte(body)) {
			logger.Debugf("update-subscription: using JSON format %s", redactSecret(jsonURL))
			return body, resp, true, nil
		}
	}

	body, resp, err := fetchSubscriptionURL(group.SubscriptionUrl)
	if err != nil {
		return "", nil, false, err
	}
	return body, resp, xrayjson.IsConfigBody([]byte(body)), nil
}

// subscriptionJSONURL appends /json to a subscription URL unless it already
// points at the JSON endpoint.
func subscriptionJSONURL(raw string) (string, bool) {
	u, err := url.Parse(raw)
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") {
		return "", false
	}
	if strings.HasSuffix(strings.TrimRight(u.Path, "/"), "/json") {
		return "", false
	}
	u.Path = strings.TrimRight(u.Path, "/") + "/json"
	return u.String(), true
}

func fetchSubscriptionURL(subscriptionURL string) (string, *http.Response, error) {
	// Reject non-http(s) schemes to avoid protocol-smuggling surprises
	// (e.g. file:// reads from local disk).
	if !strings.HasPrefix(subscriptionURL, "http://") &&
		!strings.HasPrefix(subscriptionURL, "https://") {
		return "", nil, fmt.Errorf("invalid subscription URL scheme")
	}

	req, err := http.NewRequest("GET", subscriptionURL, nil)
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

	resp, err := subClient.Do(req)
	if err != nil {
		return "", nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != 200 {
		return "", nil, fmt.Errorf("HTTP %d", resp.StatusCode)
	}

	// LimitReader caps the body so a hostile server can't OOM the core
	// with a multi-GB response.
	bodyBytes, err := io.ReadAll(io.LimitReader(resp.Body, maxSubBytes))
	if err != nil {
		return "", nil, err
	}

	return string(bodyBytes), resp, nil
}

const (
	minUpdateIntervalHours   = 1
	maxUpdateIntervalHours   = 8760
	maxUpdateIntervalSeconds = maxUpdateIntervalHours * 3600
)

type updateIntervalAction int

const (
	updateIntervalKeep updateIntervalAction = iota
	updateIntervalSet
	updateIntervalClear
)

// parseUpdateIntervalValue interprets the provider value. The standard unit is
// hours; 0 explicitly disables the schedule. Invalid values are ignored so a
// single bad header cannot erase a working interval.
func parseUpdateIntervalValue(raw string) (int64, updateIntervalAction) {
	raw = strings.Trim(strings.TrimSpace(raw), `"'`)
	if raw == "" {
		return 0, updateIntervalKeep
	}
	hours, err := strconv.Atoi(raw)
	if err != nil {
		return 0, updateIntervalKeep
	}
	switch {
	case hours == 0:
		return 0, updateIntervalClear
	case hours < minUpdateIntervalHours || hours > maxUpdateIntervalHours:
		return 0, updateIntervalKeep
	default:
		return int64(hours) * 3600, updateIntervalSet
	}
}

// parseProfileUpdateInterval prefers the standard HTTP header and falls back to
// a body directive (plain or base64), which some providers use instead.
func parseProfileUpdateInterval(resp *http.Response, body string) (int64, updateIntervalAction) {
	sawValue := false
	if resp != nil {
		if raw := strings.TrimSpace(resp.Header.Get("profile-update-interval")); raw != "" {
			sawValue = true
			if seconds, action := parseUpdateIntervalValue(raw); action != updateIntervalKeep {
				return seconds, action
			}
		}
	}
	if raw, ok := lib.SubscriptionHeader(body, "profile-update-interval"); ok {
		sawValue = true
		if seconds, action := parseUpdateIntervalValue(raw); action != updateIntervalKeep {
			return seconds, action
		}
	}
	if sawValue {
		return 0, updateIntervalKeep
	}
	return 0, updateIntervalClear
}

func subscriptionFieldsFromResponse(resp *http.Response, body, subscriptionURL, format string) db.SubscriptionFields {
	fields := db.SubscriptionFields{Format: &format}
	if title := subscriptionTitle(resp, subscriptionURL); title != "" {
		fields.Title = &title
	}
	if resp != nil {
		for _, part := range strings.Split(resp.Header.Get("subscription-userinfo"), ";") {
			kv := strings.SplitN(strings.TrimSpace(part), "=", 2)
			if len(kv) != 2 {
				continue
			}
			val, err := strconv.ParseInt(strings.TrimSpace(kv[1]), 10, 64)
			if err != nil {
				continue
			}
			switch strings.TrimSpace(kv[0]) {
			case "upload":
				fields.Upload = &val
			case "download":
				fields.Download = &val
			case "total":
				fields.Total = &val
			case "expire":
				fields.Expires = &val
			}
		}
	}
	if seconds, action := parseProfileUpdateInterval(resp, body); action != updateIntervalKeep {
		fields.ProviderInterval = &seconds
	}
	return fields
}

// subscriptionTitle returns the display name a provider advertises in the
// profile-title response header (Happ/Remnawave send it, optionally base64
// encoded), falling back to the subscription URL host.
func subscriptionTitle(resp *http.Response, subscriptionURL string) string {
	if resp != nil {
		if title := decodeProfileTitle(resp.Header.Get("profile-title")); title != "" {
			return title
		}
	}
	return utils.SubscriptionHost(subscriptionURL)
}

func decodeProfileTitle(raw string) string {
	title := strings.Trim(strings.TrimSpace(raw), `"'`)
	if encoded, ok := strings.CutPrefix(title, "base64:"); ok {
		encoded = strings.TrimSpace(encoded)
		decoded, err := base64.StdEncoding.DecodeString(encoded)
		if err != nil {
			decoded, err = base64.RawStdEncoding.DecodeString(encoded)
		}
		if err != nil {
			return ""
		}
		title = string(decoded)
	}
	title = strings.Map(func(r rune) rune {
		if unicode.IsControl(r) {
			return -1
		}
		return r
	}, title)
	title = strings.Join(strings.Fields(title), " ")
	title = strings.Trim(title, `"'`)
	// Keep the tree readable even if a provider sends a paragraph.
	if runes := []rune(title); len(runes) > 80 {
		title = string(runes[:80])
	}
	return title
}
