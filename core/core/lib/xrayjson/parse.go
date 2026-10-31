// Package xrayjson handles subscriptions whose body is a JSON array of full
// Xray client configurations (one selectable server per element) instead of a
// list of share links. Happ calls these "JSON subscriptions".
package xrayjson

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
)

// Profile is one server entry from a JSON subscription.
type Profile struct {
	Raw         json.RawMessage
	Name        string
	Protocol    string
	Address     string
	Host        string
	Port        int
	ProviderKey string
}

// HostPort is an outbound server endpoint.
type HostPort struct {
	Host string
	Port int
}

type configDoc struct {
	Remarks   string     `json:"remarks"`
	Outbounds []outbound `json:"outbounds"`
}

type outbound struct {
	Tag            string          `json:"tag"`
	Protocol       string          `json:"protocol"`
	Settings       json.RawMessage `json:"settings"`
	StreamSettings json.RawMessage `json:"streamSettings"`
}

type endpoint struct {
	Address string `json:"address"`
	Port    int    `json:"port"`
}

type endpointSettings struct {
	VNext   []endpoint `json:"vnext"`
	Servers []endpoint `json:"servers"`
	Address string     `json:"address"`
	Port    int        `json:"port"`
}

// serviceProtocols are outbounds that do not carry a proxy connection.
var serviceProtocols = map[string]bool{
	"":          true,
	"freedom":   true,
	"blackhole": true,
	"dns":       true,
	"loopback":  true,
}

// IsConfigBody reports whether body is a JSON subscription we can import. It is
// deliberately cheap enough to call on every subscription refresh.
func IsConfigBody(body []byte) bool {
	profiles, err := Parse(body)
	return err == nil && len(profiles) > 0
}

// Parse extracts server entries from a JSON subscription body. Both a JSON
// array of configurations and a single configuration object are accepted;
// entries without a usable proxy outbound are skipped.
func Parse(body []byte) ([]Profile, error) {
	trimmed := bytes.TrimSpace(bytes.TrimPrefix(body, []byte{0xEF, 0xBB, 0xBF}))
	if len(trimmed) == 0 {
		return nil, errors.New("empty body")
	}

	var items []json.RawMessage
	switch trimmed[0] {
	case '[':
		if err := json.Unmarshal(trimmed, &items); err != nil {
			return nil, fmt.Errorf("invalid JSON subscription: %w", err)
		}
	case '{':
		items = []json.RawMessage{json.RawMessage(trimmed)}
	default:
		return nil, errors.New("body is not a JSON configuration")
	}

	profiles := make([]Profile, 0, len(items))
	seen := make(map[string]int, len(items))
	for i, raw := range items {
		p, err := parseOne(raw, i)
		if err != nil {
			continue
		}
		// Duplicate remarks are common for fallback entries. Keep the key
		// stable within one refresh by numbering occurrences in order.
		seen[p.ProviderKey]++
		if n := seen[p.ProviderKey]; n > 1 {
			p.ProviderKey = fmt.Sprintf("%s#%d", p.ProviderKey, n)
		}
		profiles = append(profiles, p)
	}
	if len(profiles) == 0 {
		return nil, errors.New("no usable xray configurations in JSON body")
	}
	return profiles, nil
}

func parseOne(raw json.RawMessage, index int) (Profile, error) {
	var doc configDoc
	if err := json.Unmarshal(raw, &doc); err != nil {
		return Profile{}, err
	}

	proxy, ok := firstProxyOutbound(doc.Outbounds)
	if !ok {
		return Profile{}, errors.New("no proxy outbound")
	}

	ep := outboundEndpoint(proxy)
	name := strings.TrimSpace(doc.Remarks)
	if name == "" {
		if ep.Address != "" {
			name = fmt.Sprintf("%s:%d", ep.Address, ep.Port)
		} else {
			name = fmt.Sprintf("Server %d", index+1)
		}
	}

	key := strings.TrimSpace(doc.Remarks)
	if key == "" {
		key = fmt.Sprintf("%s://%s:%d", proxy.Protocol, ep.Address, ep.Port)
	}

	return Profile{
		Raw:         append(json.RawMessage(nil), raw...),
		Name:        name,
		Protocol:    displayProtocol(proxy.Protocol),
		Address:     ep.Address,
		Host:        outboundHost(proxy, ep.Address),
		Port:        ep.Port,
		ProviderKey: key,
	}, nil
}

func firstProxyOutbound(outbounds []outbound) (outbound, bool) {
	for _, ob := range outbounds {
		if !serviceProtocols[strings.ToLower(strings.TrimSpace(ob.Protocol))] {
			return ob, true
		}
	}
	return outbound{}, false
}

func outboundEndpoint(ob outbound) endpoint {
	var settings endpointSettings
	_ = json.Unmarshal(ob.Settings, &settings)
	switch {
	case len(settings.VNext) > 0:
		return settings.VNext[0]
	case len(settings.Servers) > 0:
		return settings.Servers[0]
	default:
		return endpoint{Address: settings.Address, Port: settings.Port}
	}
}

func allEndpoints(ob outbound) []endpoint {
	var settings endpointSettings
	_ = json.Unmarshal(ob.Settings, &settings)
	var out []endpoint
	out = append(out, settings.VNext...)
	out = append(out, settings.Servers...)
	if len(out) == 0 && (settings.Address != "" || settings.Port != 0) {
		out = append(out, endpoint{Address: settings.Address, Port: settings.Port})
	}
	return out
}

func outboundHost(ob outbound, fallback string) string {
	var stream map[string]json.RawMessage
	if err := json.Unmarshal(ob.StreamSettings, &stream); err != nil {
		return fallback
	}
	checks := [][2]string{
		{"wsSettings", "host"},
		{"xhttpSettings", "host"},
		{"grpcSettings", "authority"},
		{"tlsSettings", "serverName"},
		{"realitySettings", "serverName"},
	}
	for _, c := range checks {
		var nested map[string]json.RawMessage
		if json.Unmarshal(stream[c[0]], &nested) != nil {
			continue
		}
		var value string
		if json.Unmarshal(nested[c[1]], &value) == nil && strings.TrimSpace(value) != "" {
			return value
		}
	}
	return fallback
}

func displayProtocol(protocol string) string {
	switch strings.ToLower(strings.TrimSpace(protocol)) {
	case "hysteria":
		return "hysteria2"
	case "shadowsocks":
		return "shadowsocks"
	default:
		return strings.ToLower(strings.TrimSpace(protocol))
	}
}

// ServerAddresses returns every proxy endpoint referenced by the config, so the
// caller can pre-resolve all of them before TUN takes over DNS.
func ServerAddresses(raw json.RawMessage) []HostPort {
	var doc configDoc
	if err := json.Unmarshal(raw, &doc); err != nil {
		return nil
	}
	seen := make(map[HostPort]bool)
	var refs []HostPort
	for _, ob := range doc.Outbounds {
		if serviceProtocols[strings.ToLower(strings.TrimSpace(ob.Protocol))] {
			continue
		}
		for _, ep := range allEndpoints(ob) {
			host := strings.TrimSpace(ep.Address)
			if host == "" {
				continue
			}
			ref := HostPort{Host: host, Port: ep.Port}
			if seen[ref] {
				continue
			}
			seen[ref] = true
			refs = append(refs, ref)
		}
	}
	return refs
}
