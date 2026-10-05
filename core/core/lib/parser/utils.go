package parser

import (
	"encoding/json"
	"fmt"
	"net"
	"strconv"
	"strings"
)

// URLDecode decodes percent-encoded characters in a string.
// Invalid percent-sequences (like %2 or %ZZ) are left unencoded, matching Rust's urlencoding crate.
func URLDecode(s string) string {
	if !strings.Contains(s, "%") {
		return s
	}
	var b strings.Builder
	b.Grow(len(s))
	for i := 0; i < len(s); {
		if s[i] == '%' && i+2 < len(s) {
			h1 := fromHex(s[i+1])
			h2 := fromHex(s[i+2])
			if h1 >= 0 && h2 >= 0 {
				b.WriteByte(byte((h1 << 4) | h2))
				i += 3
				continue
			}
		}
		b.WriteByte(s[i])
		i++
	}
	return b.String()
}

func fromHex(c byte) int {
	if c >= '0' && c <= '9' {
		return int(c - '0')
	}
	if c >= 'a' && c <= 'f' {
		return int(c - 'a' + 10)
	}
	if c >= 'A' && c <= 'F' {
		return int(c - 'A' + 10)
	}
	return -1
}

func strPtr(s string) *string {
	return &s
}

func strPtrOrNil(s string) *string {
	if s == "" {
		return nil
	}
	return &s
}

func urlDecodePtr(s *string) *string {
	if s == nil {
		return nil
	}
	decoded := URLDecode(*s)
	return &decoded
}

// splitOnce splits s on the first occurrence of sep.
func splitOnce(s, sep string) (before, after string, ok bool) {
	idx := strings.Index(s, sep)
	if idx == -1 {
		return s, "", false
	}
	return s[:idx], s[idx+len(sep):], true
}

// parseQuery parses key=value query string into a map, ignoring empty values.
func parseQuery(raw string) map[string]string {
	res := make(map[string]string)
	if raw == "" {
		return res
	}
	pairs := strings.Split(raw, "&")
	for _, pair := range pairs {
		if pair == "" {
			continue
		}
		k, v, ok := splitOnce(pair, "=")
		if !ok {
			k = pair
			v = ""
		}
		if _, exists := res[k]; !exists && v != "" {
			res[k] = v
		}
	}
	return res
}

func getQueryParam(q map[string]string, keys ...string) *string {
	for _, k := range keys {
		if v, ok := q[k]; ok && v != "" {
			return &v
		}
	}
	return nil
}

// parseHostPort parses host:port, supporting IPv6 with brackets.
func parseHostPort(addr string) (string, uint16, error) {
	addr = strings.TrimSuffix(addr, "/")
	host, portStr, err := net.SplitHostPort(addr)
	if err != nil {
		// Try fallback if no brackets on raw IPv6 or standard port parsing
		lastColon := strings.LastIndex(addr, ":")
		if lastColon == -1 {
			return "", 0, fmt.Errorf("missing port in address: %s", addr)
		}
		host = addr[:lastColon]
		portStr = addr[lastColon+1:]
	}
	host = strings.TrimPrefix(host, "[")
	host = strings.TrimSuffix(host, "]")
	if host == "" {
		return "", 0, fmt.Errorf("empty host in address: %s", addr)
	}
	p, err := strconv.ParseUint(portStr, 10, 16)
	if err != nil {
		return "", 0, fmt.Errorf("invalid port in address: %s (%w)", addr, err)
	}
	return host, uint16(p), nil
}

// parseRawJSON attempts to parse a raw JSON string into a map if it is an object.
func parseRawJSON(s string) any {
	var val any
	if err := json.Unmarshal([]byte(s), &val); err == nil {
		if _, ok := val.(map[string]any); ok {
			return val
		}
	}
	return nil
}

// splitALPN splits comma-separated ALPN string into a slice of non-empty strings.
func splitALPN(alpn *string) []string {
	if alpn == nil || *alpn == "" {
		return nil
	}
	parts := strings.Split(*alpn, ",")
	res := make([]string, 0, len(parts))
	for _, p := range parts {
		trimmed := strings.TrimSpace(p)
		if trimmed != "" {
			res = append(res, trimmed)
		}
	}
	if len(res) == 0 {
		return nil
	}
	return res
}
