package TCPServer

import (
	"encoding/json"
	"strings"

	cmd "whoisthat-core/commands"
	"whoisthat-core/lib/logger"
	"whoisthat-core/structs"
)

func (s *Server) dispatch(handler cmd.Cmd, raw_tcp_message structs.TCPMessage) bool {
	switch raw_tcp_message.Msg {
	case "die":
		s.stop_sig <- true
	case "set-log-level":
		// Fire-and-forget: replying "ok" would be broadcast to every
		// client and the TUI dispatcher treats unknown frames as errors.
		var data struct {
			Level string `json:"level"`
		}
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		switch strings.ToLower(strings.TrimSpace(data.Level)) {
		case "error", "warn", "info", "debug", "trace":
			logger.SetLevel(data.Level)
			logger.Infof("log level set to %s", data.Level)
		default:
			handler.Warn("set-log-level-failed", "unknown log level: "+data.Level)
		}
	case "add-profiles":
		var data structs.AddProfilesData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.AddProfiles(data)

	case "delete-profiles":
		var data structs.DeleteProfilesData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.DeleteProfiles(data)

	case "reorder-profiles":
		var data structs.ReorderProfilesData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.ReorderProfiles(data)

	case "reorder-groups":
		var data structs.ReorderGroupsData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.ReorderGroups(data)

	case "move-profile":
		var data structs.MoveProfileData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.MoveProfile(data, s.proxy_manager)

	case "add-group":
		var data structs.AddGroupData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.AddGroup(data, s.proxy_manager)

	case "delete-group":
		var data structs.DeleteGroupData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.DeleteGroup(data)

	case "connect":
		var data structs.ConnectData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.Connect(data, s.proxy_manager, s.tun_manager)

	case "disconnect":
		var data structs.DisconnectData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.Disconnect(data, s.proxy_manager, s.tun_manager)

	case "test-profile":
		var data structs.TestProfileData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		go handler.TestProfile(data, s.proxy_manager)

	case "test-group":
		var data structs.TestGroupData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		go handler.TestGroup(data, s.proxy_manager)

	case "cancel-tests":
		var data structs.CancelTestsData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.CancelTests(data, s.proxy_manager)

	case "set-test-config":
		var data structs.SetTestConfigData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.SetTestConfig(data, s.proxy_manager)

	case "get-application-state":
		var data structs.GetApplicationStateData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.GetApplicationState(data, s.proxy_manager, s.tun_manager)

	case "get-profile-details":
		var data structs.ProfileID
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.GetProfileDetails(data)

	case "repair-config":
		handler.RepairConfig(s.proxy_manager, s.tun_manager)

	case "restore-db":
		handler.RestoreDB(s.proxy_manager, s.tun_manager)

	case "update-subscription":
		var data structs.UpdateSubscriptionData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		go handler.UpdateSubscription(data, s.proxy_manager)

	case "enable-tun":
		var data structs.EnableTunData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.EnableTun(data, s.proxy_manager, s.tun_manager)

	case "disable-tun":
		var data structs.DisableTunData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.DisableTun(data, s.proxy_manager, s.tun_manager)

	case "is-root":
		var data structs.IsRootData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.IsRoot(data)

	case "set-tun-name":
		var data structs.SetTunNameData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.SetTunName(data)

	case "set-hwid":
		var data structs.SetHwidData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.SetHwid(data)

	case "update-profile":
		var data structs.UpdateProfileData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.UpdateProfile(data)

	case "update-group":
		var data structs.UpdateGroupData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.UpdateGroup(data, s.proxy_manager)

	case "set-kill-switch":
		var data structs.SetKillSwitchData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.SetKillSwitch(data, s.proxy_manager)

	case "set-split-tunnel":
		var data structs.SetSplitTunnelData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.SetSplitTunnel(data, s.proxy_manager, s.tun_manager)

	case "set-autoconnect":
		var data structs.SetAutoconnectData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.SetAutoconnect(data)

	case "get-routing":
		var data structs.GetRoutingData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.GetRouting(data)

	case "update-routing":
		var data structs.UpdateRoutingData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.UpdateRouting(data, s.proxy_manager)

	case "set-manual-routing-source":
		var data structs.SetManualRoutingSourceData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.SetManualRoutingSource(data, s.proxy_manager)

	case "import-happ-routing":
		var data structs.ImportHappRoutingData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.ImportHappRouting(data, s.proxy_manager)

	case "set-group-routing-mode":
		var data structs.SetGroupRoutingModeData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.SetGroupRoutingMode(data, s.proxy_manager)

	case "set-active-routing-profile":
		var data structs.SetActiveRoutingProfileData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.SetActiveRoutingProfile(data, s.proxy_manager)

	case "delete-routing-profile":
		var data structs.DeleteRoutingProfileData
		if err := json.Unmarshal(raw_tcp_message.Data, &data); err != nil {
			logger.Warnf("Invalid body for %s: %v", raw_tcp_message.Msg, err)
			return false
		}
		handler.DeleteRoutingProfile(data, s.proxy_manager)

	default:
		logger.Warn("Unknown message:", raw_tcp_message.Msg)
	}
	return true
}
