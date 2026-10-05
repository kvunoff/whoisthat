package parser

import (
	"encoding/base64"
	"fmt"
	"strings"
)

func parseSocks(uri string) (RawData, SocksOutboundSettings, error) {
	idx := strings.Index(uri, "://")
	if idx == -1 {
		return RawData{}, SocksOutboundSettings{}, fmt.Errorf("invalid socks URI")
	}
	data := uri[idx+3:]

	rawPart, name, _ := splitOnce(data, "#")
	rawURI, _, _ := splitOnce(rawPart, "?")

	var userinfoOpt *string
	var rawHostPort string
	if before, after, ok := splitOnce(rawURI, "@"); ok {
		userinfoOpt = &before
		rawHostPort = after
	} else {
		rawHostPort = rawURI
	}

	host, port, err := parseHostPort(rawHostPort)
	if err != nil {
		return RawData{}, SocksOutboundSettings{}, fmt.Errorf("invalid socks address URI: %w", err)
	}

	var username, password *string
	if userinfoOpt != nil {
		userinfo := URLDecode(*userinfoOpt)
		decodedBytes, err := base64.StdEncoding.DecodeString(userinfo)
		var userPassStr string
		if err == nil {
			userPassStr = string(decodedBytes)
		} else {
			userPassStr = userinfo
		}

		u, p, ok := splitOnce(userPassStr, ":")
		if ok {
			uDec := URLDecode(u)
			username = &uDec
			if p != "" {
				pDec := URLDecode(p)
				password = &pDec
			}
		} else {
			uDec := URLDecode(userPassStr)
			username = &uDec
		}
	}

	remarks := URLDecode(name)
	tcpType := "tcp"

	rd := RawData{
		Remarks:  remarks,
		Username: username,
		UUID:     password, // password stored in UUID field
		Address:  &host,
		Port:     &port,
		Type:     &tcpType,
	}

	level := uint8(0)
	var users []SocksUser
	if rd.Username != nil && rd.UUID != nil {
		users = []SocksUser{
			{
				User: rd.Username,
				Pass: rd.UUID,
			},
		}
	}

	outboundSettings := SocksOutboundSettings{
		Servers: []SocksServerObject{
			{
				Address: rd.Address,
				Port:    rd.Port,
				Level:   &level,
				Users:   users,
			},
		},
	}

	return rd, outboundSettings, nil
}
