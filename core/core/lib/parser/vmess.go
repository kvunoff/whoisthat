package parser

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"strconv"
	"strings"
)

func parseVmess(uri string) (RawData, VmessOutboundSettings, error) {
	if !strings.HasPrefix(uri, "vmess://") {
		return RawData{}, VmessOutboundSettings{}, fmt.Errorf("invalid vmess URI: missing 'vmess://'")
	}
	data := strings.TrimPrefix(uri, "vmess://")

	// Try base64 json format first
	if rd, err := parseVmessBase64(data); err == nil {
		return rd, createVmessOutboundSettings(rd), nil
	}

	// Fallback to raw URI format
	rd, err := parseVmessRaw(data)
	if err != nil {
		return RawData{}, VmessOutboundSettings{}, err
	}
	return rd, createVmessOutboundSettings(rd), nil
}

func parseVmessBase64(data string) (RawData, error) {
	decodedStr := URLDecode(data)
	decodedBytes, err := base64.StdEncoding.DecodeString(decodedStr)
	if err != nil {
		decodedBytes, err = base64.URLEncoding.DecodeString(decodedStr)
		if err != nil {
			decodedBytes, err = base64.RawStdEncoding.DecodeString(decodedStr)
			if err != nil {
				decodedBytes, err = base64.RawURLEncoding.DecodeString(decodedStr)
				if err != nil {
					return RawData{}, err
				}
			}
		}
	}

	var jsonMap map[string]any
	if err := json.Unmarshal(decodedBytes, &jsonMap); err != nil {
		return RawData{}, err
	}

	getString := func(key string) *string {
		if v, ok := jsonMap[key]; ok {
			if s, ok := v.(string); ok && s != "" {
				return &s
			}
		}
		return nil
	}

	var port *uint16
	if v, ok := jsonMap["port"]; ok {
		switch val := v.(type) {
		case float64:
			p := uint16(val)
			port = &p
		case string:
			if p64, err := strconv.ParseUint(val, 10, 16); err == nil {
				p := uint16(p64)
				port = &p
			}
		}
	}

	remarks := ""
	if ps := getString("ps"); ps != nil {
		remarks = URLDecode(*ps)
	}

	rd := RawData{
		Remarks:       remarks,
		UUID:          getString("id"),
		Port:          port,
		Address:       getString("add"),
		ALPN:          urlDecodePtr(getString("alpn")),
		Path:          urlDecodePtr(getString("path")),
		Authority:     urlDecodePtr(getString("host")),
		PBK:           urlDecodePtr(getString("pbk")),
		Security:      getString("tls"),
		VnextSecurity: getString("scy"),
		SID:           urlDecodePtr(getString("sid")),
		Flow:          urlDecodePtr(getString("flow")),
		SNI:           getString("sni"),
		FP:            urlDecodePtr(getString("fp")),
		Type:          urlDecodePtr(getString("net")),
		HeaderType:    urlDecodePtr(getString("type")),
		Host:          urlDecodePtr(getString("host")),
		Seed:          urlDecodePtr(getString("seed")),
		Mode:          urlDecodePtr(getString("mode")),
		ServiceName:   urlDecodePtr(getString("path")),
		SLPN:          urlDecodePtr(getString("slpn")),
		SPX:           urlDecodePtr(getString("spx")),
		Extra:         urlDecodePtr(getString("extra")),
	}

	return rd, nil
}

func parseVmessRaw(data string) (RawData, error) {
	rest, name, _ := splitOnce(data, "#")
	rawAddr, rawQuery, _ := splitOnce(rest, "?")

	uuidRaw, rawHostPort, ok := splitOnce(rawAddr, "@")
	if !ok {
		return RawData{}, fmt.Errorf("wrong vmess format, no '@' found in the address")
	}

	host, port, err := parseHostPort(rawHostPort)
	if err != nil {
		return RawData{}, fmt.Errorf("invalid vmess address URI: %w", err)
	}

	uuid := URLDecode(uuidRaw)
	query := parseQuery(rawQuery)

	remarks := URLDecode(name)

	rd := RawData{
		Remarks:       remarks,
		UUID:          &uuid,
		Address:       &host,
		Port:          &port,
		ALPN:          urlDecodePtr(getQueryParam(query, "alpn")),
		Path:          urlDecodePtr(getQueryParam(query, "path")),
		Authority:     urlDecodePtr(getQueryParam(query, "authority")),
		PBK:           urlDecodePtr(getQueryParam(query, "pbk")),
		Security:      getQueryParam(query, "security"),
		VnextSecurity: getQueryParam(query, "scy"),
		SID:           urlDecodePtr(getQueryParam(query, "sid")),
		Flow:          getQueryParam(query, "flow"),
		SNI:           getQueryParam(query, "sni"),
		FP:            urlDecodePtr(getQueryParam(query, "fp")),
		Type:          getQueryParam(query, "type"),
		HeaderType:    getQueryParam(query, "headerType"),
		Host:          urlDecodePtr(getQueryParam(query, "host")),
		Seed:          urlDecodePtr(getQueryParam(query, "seed")),
		Mode:          urlDecodePtr(getQueryParam(query, "mode")),
		ServiceName:   urlDecodePtr(getQueryParam(query, "serviceName")),
		SLPN:          getQueryParam(query, "slpn"),
		SPX:           urlDecodePtr(getQueryParam(query, "spx")),
		Extra:         urlDecodePtr(getQueryParam(query, "extra")),
		AllowInsecure: getQueryParam(query, "allowInsecure"),
	}

	return rd, nil
}

func createVmessOutboundSettings(rd RawData) VmessOutboundSettings {
	sec := "auto"
	if rd.VnextSecurity != nil && *rd.VnextSecurity != "" {
		sec = *rd.VnextSecurity
	}
	level := uint8(0)

	return VmessOutboundSettings{
		Vnext: []VnextServerObject{
			{
				Address: rd.Address,
				Port:    rd.Port,
				Users: []VnextUser{
					{
						ID:       rd.UUID,
						Level:    &level,
						Security: &sec,
					},
				},
			},
		},
	}
}
