package lib

import (
	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/parser"
	"whoisthat-core/lib/xrayjson"
)

// ParseUri parses a proxy URI for an xray-supported protocol (vless, vmess,
// trojan, shadowsocks, socks, hysteria2) and returns xray JSON config bytes.
func ParseUri(uri string, socksport int, httpport int) ([]byte, error) {
	cfg, err := parser.ParseUri(uri, socksport, httpport)
	if err != nil {
		return nil, err
	}
	return xrayjson.ApplyAntiDPI(cfg, appconfig.GetConfig().AntiDPI)
}

// ParseUriWithTun parses a proxy URI and optionally includes a native TUN inbound.
func ParseUriWithTun(uri string, socksport int, httpport int, tunName string) ([]byte, error) {
	cfg, err := parser.ParseUriWithTun(uri, socksport, httpport, tunName)
	if err != nil {
		return nil, err
	}
	return xrayjson.ApplyAntiDPI(cfg, appconfig.GetConfig().AntiDPI)
}

// ParseUriHysteria returns a YAML config that the official hysteria2 client
// (github.com/apernet/hysteria2) understands. Only valid for hysteria2:// / hy2:// URIs.
func ParseUriHysteria(uri string, socksport int, httpport int) ([]byte, error) {
	return parser.CreateHysteria2ClientYAML(uri, socksport, httpport)
}

// GetUriProtocol returns the protocol string (e.g. "vless", "trojan", "hysteria2")
// of a URI. Empty string on error.
func GetUriProtocol(uri string) (string, error) {
	return parser.GetUriProtocol(uri)
}
