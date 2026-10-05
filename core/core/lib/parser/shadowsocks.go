package parser

import (
	"encoding/base64"
	"fmt"
	"strings"
)

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
	decodedBytes, err := base64.StdEncoding.DecodeString(userinfo)
	if err != nil {
		decodedBytes, err = base64.URLEncoding.DecodeString(userinfo)
		if err != nil {
			decodedBytes, err = base64.RawStdEncoding.DecodeString(userinfo)
			if err != nil {
				decodedBytes, err = base64.RawURLEncoding.DecodeString(userinfo)
				if err != nil {
					return RawData{}, ShadowSocksOutboundSettings{}, fmt.Errorf("shadowsocks user info is not valid base64: %w", err)
				}
			}
		}
	}

	decodedStr := string(decodedBytes)
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
