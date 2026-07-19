package lib

import (
	"encoding/json"
	"fmt"
	"os/exec"
	"strconv"
	"whoisthat-core/utils"
)

// ParseUri runs whoisthat-parser for an xray-supported protocol (vless, vmess,
// trojan, shadowsocks, socks) and returns xray JSON config bytes.
func ParseUri(uri string, socksport int, httpport int) ([]byte, error) {
	var parsed_config []byte
	parserbin, err := utils.GetParserBin()
	if err != nil {
		return parsed_config, fmt.Errorf("failed to parse: %w", err)
	}
	var parser_parse_cmd *exec.Cmd
	if httpport != -1 {
		parser_parse_cmd = exec.Command(parserbin, uri, "--socksport", strconv.Itoa(socksport), "--httpport", strconv.Itoa(httpport))
	} else {
		parser_parse_cmd = exec.Command(parserbin, uri, "--socksport", strconv.Itoa(socksport))
	}

	parsed_config, err = parser_parse_cmd.Output()
	if err != nil {
		return parsed_config, fmt.Errorf("parsing uri failed: %w", err)
	}

	return parsed_config, nil
}

// ParseUriHysteria runs whoisthat-parser with --get-hysteria-yaml and returns
// a YAML config that the official hysteria2 client (github.com/apernet/hysteria2)
// understands. Only valid for hysteria2:// / hy2:// URIs.
func ParseUriHysteria(uri string, socksport int, httpport int) ([]byte, error) {
	parserbin, err := utils.GetParserBin()
	if err != nil {
		return nil, fmt.Errorf("failed to find parser bin: %w", err)
	}
	var cmd *exec.Cmd
	if httpport != -1 {
		cmd = exec.Command(parserbin, uri, "--get-hysteria-yaml", "--socksport", strconv.Itoa(socksport), "--httpport", strconv.Itoa(httpport))
	} else {
		cmd = exec.Command(parserbin, uri, "--get-hysteria-yaml", "--socksport", strconv.Itoa(socksport))
	}
	out, err := cmd.Output()
	if err != nil {
		return out, fmt.Errorf("parsing hysteria2 uri failed: %w", err)
	}
	return out, nil
}

// parserMetadata mirrors lib.profileMetaData but is reused here for protocol
// detection without depending on db types.
type parserMetadata struct {
	Name     string `json:"name"`
	Protocol string `json:"protocol"`
	Address  string `json:"address,omitzero"`
	Host     string `json:"host,omitzero"`
}

// GetUriProtocol asks the parser what protocol a URI is. Returns the protocol
// string (e.g. "vless", "trojan", "hysteria2"). Empty string on error.
func GetUriProtocol(uri string) (string, error) {
	parserbin, err := utils.GetParserBin()
	if err != nil {
		return "", fmt.Errorf("failed to find parser bin: %w", err)
	}
	out, err := exec.Command(parserbin, uri, "--get-metadata").Output()
	if err != nil {
		return "", fmt.Errorf("getting metadata failed: %w", err)
	}
	var meta parserMetadata
	if err := json.Unmarshal(out, &meta); err != nil {
		return "", fmt.Errorf("unmarshaling metadata: %w", err)
	}
	return meta.Protocol, nil
}
