package parser

import (
	"fmt"
	"strings"
)

// GetUriProtocol identifies the protocol of the given URI string.
func GetUriProtocol(uri string) (string, error) {
	switch {
	case strings.HasPrefix(uri, "vmess://"):
		return "vmess", nil
	case strings.HasPrefix(uri, "vless://"):
		return "vless", nil
	case strings.HasPrefix(uri, "ss://"):
		return "shadowsocks", nil
	case strings.HasPrefix(uri, "socks5://"), strings.HasPrefix(uri, "socks4://"), strings.HasPrefix(uri, "socks://"):
		return "socks", nil
	case strings.HasPrefix(uri, "http://"):
		return "http", nil
	case strings.HasPrefix(uri, "trojan://"):
		return "trojan", nil
	case strings.HasPrefix(uri, "hysteria2://"), strings.HasPrefix(uri, "hy2://"):
		return "hysteria2", nil
	default:
		return "", fmt.Errorf("unsupported or invalid protocol in URI: %s", uri)
	}
}
