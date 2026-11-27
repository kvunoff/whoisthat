package structs

// NetworkSettings is the TUN/DNS block shared with clients. It is optional in
// application-state: an older core never sends it, and a newer client must
// degrade instead of substituting defaults.
type NetworkSettings struct {
	TunMTU     int      `json:"tun_mtu"`
	DnsServers []string `json:"dns_servers"`
}

type SetNetworkSettingsData struct {
	Config NetworkSettings `json:"config"`
}

type NetworkSettingsUpdated struct {
	Config NetworkSettings `json:"config"`
}
