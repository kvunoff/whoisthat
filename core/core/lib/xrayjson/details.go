package xrayjson

import (
	"encoding/json"
	"strings"

	"whoisthat-core/structs"
)

// StreamDetails extracts display-only connection metadata from the first proxy
// outbound of a provider config. Credentials (UUID, password, auth) are never
// copied into the result.
func StreamDetails(raw json.RawMessage) structs.ProfileDetails {
	var doc configDoc
	if err := json.Unmarshal(raw, &doc); err != nil {
		return structs.ProfileDetails{}
	}
	proxy, ok := firstProxyOutbound(doc.Outbounds)
	if !ok {
		return structs.ProfileDetails{}
	}

	var stream map[string]json.RawMessage
	_ = json.Unmarshal(proxy.StreamSettings, &stream)

	tls := objectValue(stream, "tlsSettings")
	reality := objectValue(stream, "realitySettings")
	ws := objectValue(stream, "wsSettings")
	xhttp := objectValue(stream, "xhttpSettings")
	httpupgrade := objectValue(stream, "httpupgradeSettings")
	grpc := objectValue(stream, "grpcSettings")
	hysteria := objectValue(stream, "hysteriaSettings")

	details := structs.ProfileDetails{
		Transport: stringValue(stream, "network"),
		Security:  stringValue(stream, "security"),
		SNI: firstNonEmpty(
			stringValue(reality, "serverName", "sni"),
			stringValue(tls, "serverName", "sni"),
			stringValue(ws, "host", "Host"),
			stringValue(xhttp, "host", "Host"),
			stringValue(httpupgrade, "host", "Host"),
		),
		ALPN: firstNonEmpty(
			alpnValue(reality, "alpn"),
			alpnValue(tls, "alpn"),
		),
		Fingerprint: firstNonEmpty(
			stringValue(reality, "fingerprint"),
			stringValue(tls, "fingerprint"),
		),
		PublicKey: stringValue(reality, "publicKey"),
		ShortId:   stringValue(reality, "shortId"),
		SpiderX:   stringValue(reality, "spiderX"),
		AllowInsecure: boolValue(tls, "allowInsecure") ||
			boolValue(reality, "allowInsecure"),
		Path: firstNonEmpty(
			stringValue(ws, "path"),
			stringValue(xhttp, "path"),
			stringValue(httpupgrade, "path"),
		),
		Host: firstNonEmpty(
			stringValue(ws, "host", "Host"),
			stringValue(xhttp, "host", "Host"),
			stringValue(httpupgrade, "host", "Host"),
			stringValue(grpc, "authority"),
		),
		ServiceName: stringValue(grpc, "serviceName"),
		Mode:        stringValue(xhttp, "mode"),
		Obfs: firstNonEmpty(
			stringValue(hysteria, "obfs", "obfs-type", "obfs_type"),
			stringValue(stream, "obfs"),
		),
	}

	var settings proxyDetailsSettings
	_ = json.Unmarshal(proxy.Settings, &settings)
	details.Flow = settings.flow()
	details.Cipher = settings.cipher()

	return details
}

type proxyDetailsSettings struct {
	VNext []struct {
		Users []struct {
			Flow     string `json:"flow"`
			Security string `json:"security"`
		} `json:"users"`
	} `json:"vnext"`
	Servers []struct {
		Method string `json:"method"`
		Users  []struct {
			Flow     string `json:"flow"`
			Security string `json:"security"`
		} `json:"users"`
	} `json:"servers"`
}

func (s proxyDetailsSettings) flow() string {
	for _, server := range s.VNext {
		for _, user := range server.Users {
			if strings.TrimSpace(user.Flow) != "" {
				return user.Flow
			}
		}
	}
	for _, server := range s.Servers {
		for _, user := range server.Users {
			if strings.TrimSpace(user.Flow) != "" {
				return user.Flow
			}
		}
	}
	return ""
}

func (s proxyDetailsSettings) cipher() string {
	for _, server := range s.Servers {
		if strings.TrimSpace(server.Method) != "" {
			return server.Method
		}
	}
	for _, server := range s.VNext {
		for _, user := range server.Users {
			if strings.TrimSpace(user.Security) != "" {
				return user.Security
			}
		}
	}
	return ""
}

func objectValue(parent map[string]json.RawMessage, key string) map[string]json.RawMessage {
	raw, ok := parent[key]
	if !ok {
		return map[string]json.RawMessage{}
	}
	var child map[string]json.RawMessage
	if json.Unmarshal(raw, &child) != nil || child == nil {
		return map[string]json.RawMessage{}
	}
	return child
}

func stringValue(obj map[string]json.RawMessage, keys ...string) string {
	for _, key := range keys {
		raw, ok := obj[key]
		if !ok {
			continue
		}
		var value string
		if json.Unmarshal(raw, &value) == nil && strings.TrimSpace(value) != "" {
			return value
		}
	}
	return ""
}

func alpnValue(obj map[string]json.RawMessage, key string) string {
	raw, ok := obj[key]
	if !ok {
		return ""
	}
	var values []string
	if json.Unmarshal(raw, &values) == nil && len(values) > 0 {
		return strings.Join(values, ",")
	}
	var value string
	if json.Unmarshal(raw, &value) == nil {
		return value
	}
	return ""
}

func boolValue(obj map[string]json.RawMessage, keys ...string) bool {
	for _, key := range keys {
		raw, ok := obj[key]
		if !ok {
			continue
		}
		var value bool
		if json.Unmarshal(raw, &value) == nil {
			return value
		}
		var text string
		if json.Unmarshal(raw, &text) == nil {
			switch strings.ToLower(strings.TrimSpace(text)) {
			case "1", "true", "yes", "on":
				return true
			}
		}
	}
	return false
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		if strings.TrimSpace(value) != "" {
			return value
		}
	}
	return ""
}
