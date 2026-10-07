package lib

import (
	"whoisthat-core/lib/parser"
)

// ParseUri parses a proxy URI for an xray-supported protocol (vless, vmess,
// trojan, shadowsocks, socks, hysteria2) and returns xray JSON config bytes.
func ParseUri(uri string, socksport int, httpport int) ([]byte, error) {
	return parser.ParseUri(uri, socksport, httpport)
}

// ParseUriWithTun parses a proxy URI and optionally includes a native TUN inbound.
func ParseUriWithTun(uri string, socksport int, httpport int, tunName string) ([]byte, error) {
	return parser.ParseUriWithTun(uri, socksport, httpport, tunName)
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
