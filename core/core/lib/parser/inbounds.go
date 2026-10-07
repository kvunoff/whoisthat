package parser

// GenerateInboundConfig generates the inbounds list based on provided ports.
func GenerateInboundConfig(socksPort, httpPort int) []Inbound {
	inbounds := make([]Inbound, 0, 2)
	if socksPort > 0 {
		inbounds = append(inbounds, generateSocksInbound(uint16(socksPort)))
	}
	if httpPort > 0 {
		inbounds = append(inbounds, generateHTTPInbound(uint16(httpPort)))
	}
	return inbounds
}

// GenerateInboundConfigWithTun generates inbounds with an optional native TUN inbound.
func GenerateInboundConfigWithTun(socksPort, httpPort int, tunName string) []Inbound {
	inbounds := GenerateInboundConfig(socksPort, httpPort)
	if tunName != "" {
		inbounds = append(inbounds, GenerateTunInbound(tunName, 1500))
	}
	return inbounds
}

// GenerateTunInbound generates an Xray native TUN inbound.
func GenerateTunInbound(tunName string, mtu int) Inbound {
	enabled := true
	routeOnly := false
	metadataOnly := false
	if mtu <= 0 {
		mtu = 1500
	}
	return Inbound{
		Protocol: "tun",
		Tag:      "tun-in",
		Settings: &TunSettings{
			Name: tunName,
			MTU:  mtu,
		},
		Sniffing: &SniffingSettings{
			Enabled:      &enabled,
			RouteOnly:    &routeOnly,
			MetadataOnly: &metadataOnly,
			DestOverride: []string{"http", "tls", "quic"},
		},
	}
}

func generateHTTPInbound(port uint16) Inbound {
	enabled := true
	routeOnly := true
	metadataOnly := false
	return Inbound{
		Protocol: "http",
		Port:     port,
		Tag:      "http-in",
		Listen:   "127.0.0.1",
		Sniffing: &SniffingSettings{
			Enabled:      &enabled,
			RouteOnly:    &routeOnly,
			MetadataOnly: &metadataOnly,
			DestOverride: []string{"http", "tls", "quic"},
		},
	}
}

func generateSocksInbound(port uint16) Inbound {
	enabled := true
	routeOnly := true
	metadataOnly := false
	return Inbound{
		Protocol: "socks",
		Port:     port,
		Tag:      "socks-in",
		Listen:   "127.0.0.1",
		Settings: &InboundSettings{UDP: true},
		Sniffing: &SniffingSettings{
			Enabled:      &enabled,
			RouteOnly:    &routeOnly,
			MetadataOnly: &metadataOnly,
			DestOverride: []string{"http", "tls", "quic"},
		},
	}
}
