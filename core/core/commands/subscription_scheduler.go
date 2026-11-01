package cmd

import (
	"time"

	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	"whoisthat-core/structs"
)

const (
	autoUpdateTick         = 10 * time.Minute
	autoUpdateStartupDelay = 30 * time.Second
	autoUpdateRetryBase    = 30 * time.Minute
	autoUpdateRetryMax     = 6 * time.Hour
)

type retryState struct {
	failures int
	next     time.Time
}

type updateTarget struct {
	groupID int
	probe   bool
}

func effectiveUpdateInterval(g structs.Group) time.Duration {
	if g.SubUpdateIntervalOverride > 0 {
		return time.Duration(g.SubUpdateIntervalOverride) * time.Second
	}
	return time.Duration(g.SubUpdateInterval) * time.Second
}

func subscriptionDue(g structs.Group, now time.Time) bool {
	if g.SubscriptionUrl == "" || g.SubAutoUpdateDisabled {
		return false
	}
	interval := effectiveUpdateInterval(g)
	if interval <= 0 {
		return false
	}
	if g.SubLastUpdated <= 0 {
		return true
	}
	return now.Sub(time.Unix(g.SubLastUpdated, 0)) >= interval
}

func backoffDuration(failures int) time.Duration {
	d := autoUpdateRetryBase
	for i := 1; i < failures; i++ {
		d *= 2
		if d >= autoUpdateRetryMax {
			return autoUpdateRetryMax
		}
	}
	return d
}

func retryPending(r retryState, now time.Time) bool {
	return !r.next.IsZero() && now.Before(r.next)
}

// planAutoUpdate decides which groups need work this tick. It is pure so the
// due/probe/backoff rules can be tested without sleeping or hitting the network.
func planAutoUpdate(groups []structs.Group, now time.Time, probes map[int]bool, retry map[int]retryState) []updateTarget {
	seen := make(map[int]bool, len(groups))
	var targets []updateTarget
	for _, g := range groups {
		seen[g.Id] = true
		if g.SubscriptionUrl == "" || g.SubAutoUpdateDisabled {
			delete(probes, g.Id)
			delete(retry, g.Id)
			continue
		}
		if effectiveUpdateInterval(g) > 0 {
			delete(probes, g.Id)
			if subscriptionDue(g, now) && !retryPending(retry[g.Id], now) {
				targets = append(targets, updateTarget{groupID: g.Id})
			}
			continue
		}
		// Unknown interval: probe once per core start. A group that has never
		// refreshed successfully is retried until it produces a response.
		if probes[g.Id] || g.SubLastUpdated == 0 {
			probes[g.Id] = true
			if !retryPending(retry[g.Id], now) {
				targets = append(targets, updateTarget{groupID: g.Id, probe: true})
			}
		}
	}
	for id := range probes {
		if !seen[id] {
			delete(probes, id)
		}
	}
	for id := range retry {
		if !seen[id] {
			delete(retry, id)
		}
	}
	return targets
}

// AutoUpdateLoop runs in the daemon, so subscriptions refresh even when the TUI
// is closed. It waits for bootAutoconnect to finish before the first pass.
func (cmd *Cmd) AutoUpdateLoop(proxy_manager *proxy.ProxyManager) {
	time.Sleep(autoUpdateStartupDelay)

	probes := map[int]bool{}
	if groups, err := cmd.DB.GetAllGroupConfigs(); err == nil {
		for _, g := range groups {
			if g.SubscriptionUrl != "" && !g.SubAutoUpdateDisabled && effectiveUpdateInterval(g) == 0 {
				probes[g.Id] = true
			}
		}
	} else {
		logger.Warnf("auto-update: cannot list groups: %v", err)
	}

	retry := map[int]retryState{}
	for {
		cmd.autoUpdateTick(proxy_manager, probes, retry)
		time.Sleep(autoUpdateTick)
	}
}

func (cmd *Cmd) autoUpdateTick(proxy_manager *proxy.ProxyManager, probes map[int]bool, retry map[int]retryState) {
	defer func() {
		if r := recover(); r != nil {
			logger.Errorf("auto-update tick: panic: %v", r)
		}
	}()
	groups, err := cmd.DB.GetAllGroupConfigs()
	if err != nil {
		logger.Warnf("auto-update: cannot list groups: %v", err)
		return
	}
	now := time.Now()
	for _, target := range planAutoUpdate(groups, now, probes, retry) {
		ok, attempted := cmd.safeAutoUpdate(target, proxy_manager)
		if !attempted {
			continue
		}
		if ok {
			delete(retry, target.groupID)
			if target.probe {
				delete(probes, target.groupID)
			}
			continue
		}
		r := retry[target.groupID]
		r.failures++
		r.next = now.Add(backoffDuration(r.failures))
		retry[target.groupID] = r
	}
}

func (cmd *Cmd) safeAutoUpdate(target updateTarget, proxy_manager *proxy.ProxyManager) (ok bool, attempted bool) {
	defer func() {
		if r := recover(); r != nil {
			logger.Errorf("auto-update: panic for group %d: %v", target.groupID, r)
			ok = false
			attempted = true
		}
	}()
	return cmd.AutoUpdateSubscription(
		structs.UpdateSubscriptionData{GroupId: target.groupID},
		proxy_manager,
		target.probe,
	)
}
