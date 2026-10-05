package parser

import (
	"fmt"
	"strings"
)

func parseTrojan(uri string) (RawData, TrojanOutboundSettings, error) {
	if !strings.HasPrefix(uri, "trojan://") {
		return RawData{}, TrojanOutboundSettings{}, fmt.Errorf("invalid trojan URI: missing 'trojan://'")
	}
	data := strings.TrimPrefix(uri, "trojan://")

	rest, name, _ := splitOnce(data, "#")
	rawAddr, rawQuery, _ := splitOnce(rest, "?")

	passwordRaw, rawHostPort, ok := splitOnce(rawAddr, "@")
	if !ok {
		return RawData{}, TrojanOutboundSettings{}, fmt.Errorf("wrong trojan format, no '@' found in the address")
	}

	host, port, err := parseHostPort(rawHostPort)
	if err != nil {
		return RawData{}, TrojanOutboundSettings{}, fmt.Errorf("invalid trojan address URI: %w", err)
	}

	password := URLDecode(passwordRaw)
	query := parseQuery(rawQuery)

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
		AllowInsecure: getQueryParam(query, "allowInsecure"),
	}

	level := uint8(0)
	outboundSettings := TrojanOutboundSettings{
		Servers: []TrojanServerObject{
			{
				Address:  rd.Address,
				Port:     rd.Port,
				Password: rd.UUID,
				Level:    &level,
			},
		},
	}

	return rd, outboundSettings, nil
}
