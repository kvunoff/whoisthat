package appconfig

import "encoding/json"

// applyField decodes one top-level key so a single broken field cannot poison
// the rest of the file.
func applyField(cfg *AppConfig, key string, raw json.RawMessage) error {
	switch key {
	case "socks-port":
		return json.Unmarshal(raw, &cfg.SocksPort)
	case "http-port":
		return json.Unmarshal(raw, &cfg.HttpPort)
	case "core-tcp-port":
		return json.Unmarshal(raw, &cfg.CoreTCPPort)
	case "test-port-range":
		return json.Unmarshal(raw, &cfg.TestPortRange)
	case "dns-servers":
		return json.Unmarshal(raw, &cfg.DnsServers)
	case "tun-name":
		return json.Unmarshal(raw, &cfg.TunName)
	case "hwid-enabled":
		return json.Unmarshal(raw, &cfg.HwidEnabled)
	case "hwid":
		return json.Unmarshal(raw, &cfg.Hwid)
	case "user-agent":
		return json.Unmarshal(raw, &cfg.UserAgent)
	case "kill-switch-enabled":
		return json.Unmarshal(raw, &cfg.KillSwitchEnabled)
	case "autoconnect-enabled":
		return json.Unmarshal(raw, &cfg.AutoconnectEnabled)
	case "autoconnect-group-id":
		return json.Unmarshal(raw, &cfg.AutoconnectGroupId)
	case "autoconnect-profile-id":
		return json.Unmarshal(raw, &cfg.AutoconnectProfileId)
	case "autoconnect-mode":
		return json.Unmarshal(raw, &cfg.AutoconnectMode)
	case "test-config":
		return json.Unmarshal(raw, &cfg.TestConfig)
	case "ipc-socket-path":
		return json.Unmarshal(raw, &cfg.IPCSocketPath)
	case "tcp-enabled":
		return json.Unmarshal(raw, &cfg.TCPEnabled)
	case "split-tunnel":
		return json.Unmarshal(raw, &cfg.SplitTunnel)
	}
	return nil // unknown keys are ignored, not fatal
}
