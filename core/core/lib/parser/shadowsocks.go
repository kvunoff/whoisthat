package parser

import (
	"encoding/base64"
	"fmt"
	"strings"
)

// decodeShadowsocksUserinfo decodes the legacy base64 userinfo used by
// ss:// links. SIP002 plain userinfo is handled by the caller as a fallback.
func decodeShadowsocksUserinfo(userinfo string) (string, bool) {
	encodings := []*base64.Encoding{
		base64.StdEncoding,
		base64.URLEncoding,
		base64.RawStdEncoding,
		base64.RawURLEncoding,
	}
	for _, enc := range encodings {
		if decoded, err := enc.DecodeString(userinfo); err == nil {
			if strings.Contains(string(decoded), ":") {
				return string(decoded), true
			}
		}
	}
	return "", false
}

func parseShadowsocks(uri string) (RawData, ShadowSocksOutboundSettings, error) {
	if !strings.HasPrefix(uri, "ss://") {
		return RawData{}, ShadowSocksOutboundSettings{}, fmt.Errorf("invalid shadowsocks URI: missing 'ss://'")
	}
	data := strings.TrimPrefix(uri, "ss://")

	rawPart, name, _ := splitOnce(data, "#")
	rawURI, _, _ := splitOnce(rawPart, "?")

	userinfoRaw, rawHostPort, ok := splitOnce(rawURI, "@")
	if !ok {
		return RawData{}, ShadowSocksOutboundSettings{}, fmt.Errorf("wrong shadowsocks format, no '@' found in the address")
	}

	host, port, err := parseHostPort(rawHostPort)
	if err != nil {
		return RawData{}, ShadowSocksOutboundSettings{}, fmt.Errorf("invalid shadowsocks address URI: %w", err)
	}

	userinfo := URLDecode(userinfoRaw)
	decodedStr, ok := decodeShadowsocksUserinfo(userinfo)
	if !ok {
		// SIP002 permits a plain, percent-encoded "method:password" userinfo.
		// Accept it when it is not valid base64.
		decodedStr = userinfo
	}

	method, password, ok := splitOnce(decodedStr, ":")
	if !ok {
		return RawData{}, ShadowSocksOutboundSettings{}, fmt.Errorf("no ':' found in decoded shadowsocks data")
	}

	remarks := URLDecode(name)
	tcpType := "tcp"
	headerType := "none"

	rd := RawData{
		Remarks:      remarks,
		ServerMethod: &method,
		Address:      &host,
		Port:         &port,
		UUID:         &password,
		Type:         &tcpType,
		HeaderType:   &headerType,
	}

	level := uint8(0)
	outboundSettings := ShadowSocksOutboundSettings{
		Servers: []ShadowSocksServerObject{
			{
				Address:  rd.Address,
				Port:     rd.Port,
				Password: rd.UUID,
				Level:    &level,
				Method:   rd.ServerMethod,
			},
		},
	}

	return rd, outboundSettings, nil
}
