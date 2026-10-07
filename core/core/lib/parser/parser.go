package parser

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"strings"
)

// ParseUri parses a proxy URI and generates full Xray configuration JSON bytes.
func ParseUri(uri string, socksPort, httpPort int) ([]byte, error) {
	return ParseUriWithTun(uri, socksPort, httpPort, "")
}

// ParseUriWithTun parses a proxy URI and generates full Xray configuration JSON bytes,
// optionally appending a native TUN inbound if tunName is non-empty.
func ParseUriWithTun(uri string, socksPort, httpPort int, tunName string) ([]byte, error) {
	outbound, _, err := createOutboundObject(uri)
	if err != nil {
		return nil, err
	}

	inbounds := GenerateInboundConfigWithTun(socksPort, httpPort, tunName)
	cfg := Config{
		Outbounds: []Outbound{outbound},
		Inbounds:  inbounds,
	}

	return json.Marshal(cfg)
}

// GetMetadata extracts basic profile metadata from a URI.
func GetMetadata(uri string) (ProfileMetadata, error) {
	_, rd, err := createOutboundObject(uri)
	if err != nil {
		return ProfileMetadata{}, err
	}
	proto, _ := GetUriProtocol(uri)

	return ProfileMetadata{
		Name:     rd.Remarks,
		Protocol: proto,
		Address:  rd.Address,
		Host:     rd.Host,
		Port:     rd.Port,
	}, nil
}

// GetMetadataBatch reads newline-delimited URIs from a reader and returns metadata for each valid one.
func GetMetadataBatch(reader io.Reader) ([]BatchProfileMetadata, error) {
	var results []BatchProfileMetadata
	scanner := bufio.NewScanner(reader)
	// Support large subscription lines (up to 1MB per line if needed)
	buf := make([]byte, 64*1024)
	scanner.Buffer(buf, 1024*1024)

	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}
		_, rd, err := createOutboundObject(line)
		if err != nil {
			continue
		}
		proto, _ := GetUriProtocol(line)
		results = append(results, BatchProfileMetadata{
			URI:      line,
			Name:     rd.Remarks,
			Protocol: proto,
			Address:  rd.Address,
			Host:     rd.Host,
			Port:     rd.Port,
		})
	}

	if err := scanner.Err(); err != nil {
		return nil, err
	}
	return results, nil
}

// CreateHysteria2ClientYAML creates YAML for the official hysteria2 client if the URI is hysteria2/hy2.
func CreateHysteria2ClientYAML(uri string, socksPort, httpPort int) ([]byte, error) {
	proto, err := GetUriProtocol(uri)
	if err != nil || proto != "hysteria2" {
		return nil, fmt.Errorf("URI is not a hysteria2/hy2 link (this command only handles hysteria2)")
	}
	rd, _, err := parseHysteria2(uri)
	if err != nil {
		return nil, err
	}
	return createHysteria2ClientYAML(&rd, socksPort, httpPort)
}

func createOutboundObject(uri string) (Outbound, RawData, error) {
	proto, err := GetUriProtocol(uri)
	if err != nil {
		return Outbound{}, RawData{}, err
	}

	var rd RawData
	var settings any

	switch proto {
	case "vless":
		r, s, err := parseVless(uri)
		if err != nil {
			return Outbound{}, RawData{}, err
		}
		rd, settings = r, s
	case "vmess":
		r, s, err := parseVmess(uri)
		if err != nil {
			return Outbound{}, RawData{}, err
		}
		rd, settings = r, s
	case "trojan":
		r, s, err := parseTrojan(uri)
		if err != nil {
			return Outbound{}, RawData{}, err
		}
		rd, settings = r, s
	case "shadowsocks":
		r, s, err := parseShadowsocks(uri)
		if err != nil {
			return Outbound{}, RawData{}, err
		}
		rd, settings = r, s
	case "socks":
		r, s, err := parseSocks(uri)
		if err != nil {
			return Outbound{}, RawData{}, err
		}
		rd, settings = r, s
	case "hysteria2":
		r, s, err := parseHysteria2(uri)
		if err != nil {
			return Outbound{}, RawData{}, err
		}
		rd, settings = r, s
	default:
		return Outbound{}, RawData{}, fmt.Errorf("unsupported protocol: %s", proto)
	}

	isHy2 := proto == "hysteria2"

	networkType := "tcp"
	if isHy2 {
		networkType = "hysteria"
	} else if rd.Type != nil && *rd.Type != "" {
		networkType = *rd.Type
	}

	var effectiveSecurity *string
	if isHy2 {
		sec := "tls"
		effectiveSecurity = &sec
	} else if proto == "trojan" {
		if rd.Security != nil && *rd.Security != "" {
			effectiveSecurity = rd.Security
		} else {
			sec := "tls"
			effectiveSecurity = &sec
		}
	} else {
		effectiveSecurity = rd.Security
	}

	allowInsecure := false
	if rd.AllowInsecure != nil && (*rd.AllowInsecure == "true" || *rd.AllowInsecure == "1") {
		allowInsecure = true
	}

	outboundProtocol := proto
	if isHy2 {
		outboundProtocol = "hysteria"
	}

	streamSettings := StreamSettings{
		Network:  &networkType,
		Security: effectiveSecurity,
	}

	if effectiveSecurity != nil && *effectiveSecurity == "tls" {
		alpn := splitALPN(rd.ALPN)
		if isHy2 && len(alpn) == 0 {
			alpn = []string{"h3"}
		}
		streamSettings.TLSSettings = &TLSSettings{
			ALPN:          alpn,
			AllowInsecure: allowInsecure,
			Fingerprint:   rd.FP,
			ServerName:    rd.SNI,
		}
	} else if effectiveSecurity != nil && *effectiveSecurity == "reality" {
		spx := ""
		streamSettings.RealitySettings = &RealitySettings{
			PublicKey:   rd.PBK,
			ServerName:  rd.SNI,
			ShortId:     rd.SID,
			SpiderX:     &spx,
			Fingerprint: rd.FP,
		}
	}

	switch networkType {
	case "ws":
		streamSettings.WsSettings = &WsSettings{
			Host: rd.Host,
			Path: rd.Path,
		}
	case "tcp":
		ht := "none"
		if rd.HeaderType != nil && *rd.HeaderType != "" {
			ht = *rd.HeaderType
		}
		streamSettings.TCPSettings = &TCPSettings{
			Header: &TCPHeader{
				Type: &ht,
			},
		}
	case "grpc":
		multi := rd.Mode != nil && *rd.Mode == "multi"
		streamSettings.GRPCSettings = &GRPCSettings{
			Authority:   rd.Authority,
			MultiMode:   &multi,
			ServiceName: rd.ServiceName,
		}
	case "quic":
		none := "none"
		empty := ""
		streamSettings.QuicSettings = &QuicSettings{
			Header: &NonHeaderObject{
				Type: &none,
			},
			Security: &none,
			Key:      &empty,
		}
	case "kcp":
		streamSettings.KCPSettings = &KCPSettings{
			Seed: rd.Seed,
		}
	case "xhttp":
		var extra any
		if rd.Extra != nil {
			extra = parseRawJSON(*rd.Extra)
		}
		streamSettings.XHTTPSettings = &XHTTPSettings{
			Host:  rd.Host,
			Path:  rd.Path,
			Mode:  rd.Mode,
			Extra: extra,
		}
	case "httpupgrade":
		streamSettings.HttpUpgradeSettings = &HttpUpgradeSettings{
			Host: rd.Host,
			Path: rd.Path,
		}
	}

	if isHy2 {
		auth := ""
		if rd.UUID != nil {
			auth = *rd.UUID
		}
		timeout := uint32(60)
		streamSettings.HysteriaSettings = &HysteriaSettings{
			Version:        2,
			Auth:           auth,
			UDPIdleTimeout: &timeout,
		}

		var quicParams *QuicParamsSettings
		if rd.Up != nil || rd.Down != nil {
			quicParams = &QuicParamsSettings{
				BrutalUp:   rd.Up,
				BrutalDown: rd.Down,
			}
		}

		var udpItems []any
		if rd.Ports != nil {
			udpItems = append(udpItems, map[string]any{
				"type": "udphop",
				"settings": map[string]any{
					"mode":        "intervalLocal,intervalRemote",
					"remotePorts": *rd.Ports,
					"interval":    "5-30",
				},
			})
		}

		if rd.FM != nil {
			var val map[string]any
			if err := json.Unmarshal([]byte(*rd.FM), &val); err == nil {
				target := val
				if fmObj, ok := val["finalmask"].(map[string]any); ok {
					target = fmObj
				}
				if qpVal, ok := target["quicParams"].(map[string]any); ok && quicParams == nil {
					var bUp, bDown *string
					if u, ok := qpVal["brutalUp"].(string); ok {
						bUp = &u
					}
					if d, ok := qpVal["brutalDown"].(string); ok {
						bDown = &d
					}
					if bUp != nil || bDown != nil {
						quicParams = &QuicParamsSettings{BrutalUp: bUp, BrutalDown: bDown}
					}
				}
				if arr, ok := target["udp"].([]any); ok {
					udpItems = append(udpItems, arr...)
				}
			}
		} else if rd.ObfsPassword != nil {
			obfsType := "salamander"
			if rd.Obfs != nil && *rd.Obfs != "" {
				obfsType = *rd.Obfs
			}
			udpItems = append(udpItems, map[string]any{
				"type": obfsType,
				"settings": map[string]any{
					"password": *rd.ObfsPassword,
				},
			})
		}

		if quicParams != nil || len(udpItems) > 0 {
			streamSettings.Finalmask = &FinalmaskSettings{
				QuicParams: quicParams,
				UDP:        udpItems,
			}
		}
	}

	outbound := Outbound{
		Protocol:       outboundProtocol,
		Tag:            "proxy",
		Settings:       settings,
		StreamSettings: streamSettings,
	}

	return outbound, rd, nil
}
