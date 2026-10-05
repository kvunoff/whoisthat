package parser

import (
	"fmt"
	"strings"
)

func parseVless(uri string) (RawData, VlessOutboundSettings, error) {
	if !strings.HasPrefix(uri, "vless://") {
		return RawData{}, VlessOutboundSettings{}, fmt.Errorf("invalid vless URI: missing 'vless://'")
	}
	data := strings.TrimPrefix(uri, "vless://")

	rest, name, _ := splitOnce(data, "#")
	rawAddr, rawQuery, _ := splitOnce(rest, "?")

	uuidRaw, rawHostPort, ok := splitOnce(rawAddr, "@")
	if !ok {
		return RawData{}, VlessOutboundSettings{}, fmt.Errorf("wrong vless format, no '@' found in the address")
	}

	host, port, err := parseHostPort(rawHostPort)
	if err != nil {
		return RawData{}, VlessOutboundSettings{}, fmt.Errorf("invalid vless address URI: %w", err)
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
		AllowInsecure: getQueryParam(query, "allowInsecure"),
	}

	enc := "none"
	if rd.Encryption != nil && *rd.Encryption != "" {
		enc = *rd.Encryption
	}
	level := uint8(0)

	outboundSettings := VlessOutboundSettings{
		Vnext: []VnextServerObject{
			{
				Address: rd.Address,
				Port:    rd.Port,
				Users: []VnextUser{
					{
						ID:         rd.UUID,
						Encryption: &enc,
						Flow:       rd.Flow,
						Level:      &level,
					},
				},
			},
		},
	}

	return rd, outboundSettings, nil
}
