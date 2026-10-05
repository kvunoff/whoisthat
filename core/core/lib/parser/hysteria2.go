package parser

import (
	"encoding/json"
	"fmt"
	"strconv"
	"strings"

	"gopkg.in/yaml.v3"
)

func parseHysteria2(uri string) (RawData, HysteriaOutboundSettings, error) {
	var data string
	if strings.HasPrefix(uri, "hysteria2://") {
		data = strings.TrimPrefix(uri, "hysteria2://")
	} else if strings.HasPrefix(uri, "hy2://") {
		data = strings.TrimPrefix(uri, "hy2://")
	} else {
		return RawData{}, HysteriaOutboundSettings{}, fmt.Errorf("invalid hysteria2 URI: missing 'hysteria2://' or 'hy2://'")
	}

	rest, name, _ := splitOnce(data, "#")
	rawAddr, rawQuery, _ := splitOnce(rest, "?")

	passwordRaw, rawHostPort, ok := splitOnce(rawAddr, "@")
	if !ok {
		return RawData{}, HysteriaOutboundSettings{}, fmt.Errorf("wrong hysteria2 format, no '@' found in the address")
	}

	password := URLDecode(passwordRaw)
	rawHostPort = strings.TrimSuffix(rawHostPort, "/")
	if rawHostPort == "" {
		return RawData{}, HysteriaOutboundSettings{}, fmt.Errorf("missing host in hysteria2 address")
	}

	var host string
	var port uint16 = 443

	if strings.HasPrefix(rawHostPort, "[") {
		bracketEnd := strings.Index(rawHostPort, "]")
		if bracketEnd == -1 {
			return RawData{}, HysteriaOutboundSettings{}, fmt.Errorf("invalid IPv6 address: missing closing bracket")
		}
		host = rawHostPort[1:bracketEnd]
		after := rawHostPort[bracketEnd+1:]
		if strings.HasPrefix(after, ":") {
			if p, err := strconv.ParseUint(after[1:], 10, 16); err == nil {
				port = uint16(p)
			} else {
				return RawData{}, HysteriaOutboundSettings{}, fmt.Errorf("invalid hysteria2 port: %w", err)
			}
		}
	} else if lastColon := strings.LastIndex(rawHostPort, ":"); lastColon != -1 {
		if p, err := strconv.ParseUint(rawHostPort[lastColon+1:], 10, 16); err == nil {
			host = rawHostPort[:lastColon]
			port = uint16(p)
		} else {
			host = rawHostPort
		}
	} else {
		host = rawHostPort
	}

	if host == "" {
		return RawData{}, HysteriaOutboundSettings{}, fmt.Errorf("missing host in hysteria2 address")
	}

	query := parseQuery(rawQuery)

	obfs := urlDecodePtr(getQueryParam(query, "obfs", "obfs-type", "obfs_type"))
	obfsPassword := urlDecodePtr(getQueryParam(query, "obfs-password", "obfs_password", "obfs-param", "obfs_param"))
	rawFM := getQueryParam(query, "fm")
	decodedFM := urlDecodePtr(rawFM)

	if (obfs == nil || obfsPassword == nil) && decodedFM != nil {
		var val map[string]any
		if err := json.Unmarshal([]byte(*decodedFM), &val); err == nil {
			var udpArr []any
			if fmObj, ok := val["finalmask"].(map[string]any); ok {
				if u, ok := fmObj["udp"].([]any); ok {
					udpArr = u
				}
			} else if u, ok := val["udp"].([]any); ok {
				udpArr = u
			}
			for _, item := range udpArr {
				if m, ok := item.(map[string]any); ok {
					if t, ok := m["type"].(string); ok && obfs == nil {
						obfs = &t
					}
					if s, ok := m["settings"].(map[string]any); ok {
						if pw, ok := s["password"].(string); ok && obfsPassword == nil {
							obfsPassword = &pw
						}
					}
				}
			}
		}
	}

	remarks := URLDecode(name)

	rd := RawData{
		Remarks:       remarks,
		UUID:          &password,
		Address:       &host,
		Port:          &port,
		ALPN:          urlDecodePtr(getQueryParam(query, "alpn")),
		Path:          urlDecodePtr(getQueryParam(query, "path")),
		Authority:     urlDecodePtr(getQueryParam(query, "authority")),
		PBK:           urlDecodePtr(getQueryParam(query, "pbk")),
		Security:      getQueryParam(query, "security"),
		SID:           urlDecodePtr(getQueryParam(query, "sid")),
		Flow:          getQueryParam(query, "flow"),
		SNI:           getQueryParam(query, "sni"),
		FP:            urlDecodePtr(getQueryParam(query, "fp")),
		Type:          getQueryParam(query, "type"),
		Encryption:    getQueryParam(query, "encryption"),
		HeaderType:    getQueryParam(query, "headerType"),
		Host:          urlDecodePtr(getQueryParam(query, "host")),
		Seed:          urlDecodePtr(getQueryParam(query, "seed")),
		QuicSecurity:  getQueryParam(query, "quicSecurity"),
		Key:           getQueryParam(query, "key"),
		Mode:          urlDecodePtr(getQueryParam(query, "mode")),
		ServiceName:   urlDecodePtr(getQueryParam(query, "serviceName")),
		SLPN:          getQueryParam(query, "slpn"),
		SPX:           urlDecodePtr(getQueryParam(query, "spx")),
		Extra:         urlDecodePtr(getQueryParam(query, "extra")),
		AllowInsecure: getQueryParam(query, "allowInsecure", "insecure"),
		Obfs:          obfs,
		ObfsPassword:  obfsPassword,
		Up:            urlDecodePtr(getQueryParam(query, "up")),
		Down:          urlDecodePtr(getQueryParam(query, "down")),
		Ports:         urlDecodePtr(getQueryParam(query, "ports", "mport")),
		FM:            decodedFM,
	}

	outboundSettings := HysteriaOutboundSettings{
		Version: 2,
		Address: rd.Address,
		Port:    rd.Port,
	}

	return rd, outboundSettings, nil
}

func createHysteria2ClientYAML(data *RawData, socksPort int, httpPort int) ([]byte, error) {
	if data.Address == nil || data.Port == nil {
		return nil, fmt.Errorf("missing address or port for hysteria2 YAML")
	}

	server := fmt.Sprintf("%s:%d", *data.Address, *data.Port)
	auth := ""
	if data.UUID != nil {
		auth = *data.UUID
	}

	var tls *Hysteria2ClientTLS
	insecure := false
	if data.AllowInsecure != nil && (*data.AllowInsecure == "true" || *data.AllowInsecure == "1") {
		insecure = true
	}
	alpn := splitALPN(data.ALPN)
	if alpn == nil {
		alpn = []string{"h3"}
	}
	if data.SNI != nil || insecure || len(alpn) > 0 {
		tls = &Hysteria2ClientTLS{
			SNI:      data.SNI,
			Insecure: &insecure,
			ALPN:     alpn,
		}
	}

	var obfs *Hysteria2ClientObfs
	if data.ObfsPassword != nil {
		obfsType := "salamander"
		if data.Obfs != nil && *data.Obfs != "" {
			obfsType = *data.Obfs
		}
		obfs = &Hysteria2ClientObfs{
			Type: &obfsType,
			Salamander: &Hysteria2ClientSalamander{
				Password: data.ObfsPassword,
			},
		}
	}

	var bandwidth *Hysteria2ClientBandwidth
	if data.Up != nil || data.Down != nil {
		bandwidth = &Hysteria2ClientBandwidth{
			Up:   data.Up,
			Down: data.Down,
		}
	}

	cfg := Hysteria2ClientConfig{
		Server:    server,
		Auth:      auth,
		TLS:       tls,
		Obfs:      obfs,
		Bandwidth: bandwidth,
		Ports:     data.Ports,
		Socks5: Hysteria2ClientListen{
			Listen: fmt.Sprintf("127.0.0.1:%d", socksPort),
		},
	}
	if httpPort > 0 {
		cfg.HTTP = &Hysteria2ClientListen{
			Listen: fmt.Sprintf("127.0.0.1:%d", httpPort),
		}
	}

	return yaml.Marshal(&cfg)
}
