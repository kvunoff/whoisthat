package cmd

import (
	"fmt"
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/logger"
	proxy "whoisthat-core/lib/proxy/mainproxy"
	"whoisthat-core/structs"
)

// routingPayload builds the routing-updated notification for a group.
// groupID == 0 addresses the global rules page.
func (cmd *Cmd) routingPayload(groupID int) (structs.RoutingUpdated, error) {
	cfg, source, err := cmd.DB.LoadEffectiveManualRouting(groupID)
	if err != nil {
		return structs.RoutingUpdated{}, err
	}

	payload := structs.RoutingUpdated{
		GroupId: groupID,
		Config:  cfg,
		Source:  source,
	}

	if groupID != 0 {
		custom, err := cmd.DB.LoadGroupRoutingIfExists(groupID)
		if err != nil {
			logger.Warnf("failed to load custom routing for group %d: %v", groupID, err)
		} else if custom != nil {
			payload.Custom = custom
		}
	}

	return payload, nil
}

func (cmd *Cmd) GetRouting(data structs.GetRoutingData) {
	payload, err := cmd.routingPayload(data.GroupId)
	if err != nil {
		logger.Warnf("failed to load routing: %v", err)
		cmd.warn("get-routing-failed", "failed to load routing config")
		return
	}
	cmd.send("routing-updated", payload)
}

func (cmd *Cmd) UpdateRouting(data structs.UpdateRoutingData, proxy_manager *proxy.ProxyManager) {
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()

	groupID := data.GroupId
	if groupID < 0 {
		cmd.warn("update-routing-failed", "Invalid group id")
		return
	}

	if groupID == 0 {
		if err := cmd.DB.SaveRouting(data.Config); err != nil {
			logger.Warnf("failed to save routing: %v", err)
			cmd.warn("update-routing-failed", "failed to save routing config")
			return
		}
	} else {
		group, err := cmd.DB.LoadGroupConfig(groupID)
		if err != nil {
			cmd.warn("update-routing-failed", "Group not found")
			return
		}

		// Editing an inheriting group implicitly turns it into a custom one.
		if group.ManualRoutingMode != structs.ManualRoutingCustom {
			if err := cmd.DB.SetManualRoutingSource(groupID, structs.ManualRoutingCustom); err != nil {
				logger.Warnf("failed to enable custom routing for group %d: %v", groupID, err)
				cmd.warn("update-routing-failed", fmt.Sprintf("failed to enable custom routing: %v", err))
				return
			}
			if updated, err := cmd.DB.LoadGroupConfig(groupID); err == nil {
				cmd.send("group-updated", updated)
			}
		}

		if err := cmd.DB.SaveGroupRouting(groupID, data.Config); err != nil {
			logger.Warnf("failed to save group routing: %v", err)
			cmd.warn("update-routing-failed", "failed to save routing config")
			return
		}
	}

	if payload, err := cmd.routingPayload(groupID); err == nil {
		cmd.send("routing-updated", payload)
	}

	cmd.reloadRoutingIfNeeded(proxy_manager, groupID)
}

func (cmd *Cmd) SetManualRoutingSource(data structs.SetManualRoutingSourceData, proxy_manager *proxy.ProxyManager) {
	ConnectionMutex.Lock()
	defer ConnectionMutex.Unlock()

	if data.GroupId == 0 {
		cmd.warn("set-manual-routing-source-failed", "Global routing has no manual source")
		return
	}

	if err := cmd.DB.SetManualRoutingSource(data.GroupId, data.Source); err != nil {
		logger.Warnf("set-manual-routing-source: %v", err)
		cmd.warn("set-manual-routing-source-failed", fmt.Sprintf("Failed to change routing source: %v", err))
		return
	}

	if group, err := cmd.DB.LoadGroupConfig(data.GroupId); err == nil {
		cmd.send("group-updated", group)
	}

	if payload, err := cmd.routingPayload(data.GroupId); err == nil {
		cmd.send("routing-updated", payload)
	}

	cmd.reloadRoutingIfNeeded(proxy_manager, data.GroupId)
}

// reloadRoutingIfNeeded reloads Xray only when the changed config can affect
// the currently connected group: global changes matter only for groups that
// inherit them, and per-group changes matter only for that group.
func (cmd *Cmd) reloadRoutingIfNeeded(proxy_manager *proxy.ProxyManager, changedGroupID int) {
	if proxy_manager == nil {
		return
	}

	status := proxy_manager.GetStatus()
	if status.Connection != "connected" || status.Profile.GroupId == 0 {
		return
	}

	connectedGroupID := status.Profile.GroupId
	if changedGroupID != 0 && changedGroupID != connectedGroupID {
		return
	}

	if changedGroupID == 0 {
		group, err := cmd.DB.LoadGroupConfig(connectedGroupID)
		if err == nil && group.ManualRoutingMode == structs.ManualRoutingCustom {
			return
		}
	}

	if err := proxy_manager.ReloadRouting(appconfig.GetConfig().TunName); err != nil {
		logger.Warnf("failed to reload routing in running proxy: %v", err)
		cmd.warn("update-routing-failed", fmt.Sprintf("failed to reload routing: %v", err))
	}
}
