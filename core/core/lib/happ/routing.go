package happ

import (
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"net/url"
	"strconv"
	"strings"
	"whoisthat-core/lib/happdns"
	"whoisthat-core/structs"
)

// Action represents the Happ routing action.
type Action string

const (
	ActionAdd   Action = "add"
	ActionOnAdd Action = "onadd"
	ActionOff   Action = "off"
)

const (
	LANRuleIP       = structs.LANRuleIP
	MaxPayloadBytes = 1024 * 1024 // 1 MiB limit
)

// FlexibleBool is kept as an alias for backwards compatibility. The
// implementation lives in lib/happdns so the DB layer can reuse it without
// importing this package (which itself imports db).
type FlexibleBool = happdns.FlexibleBool

// FlexibleUnix parses Unix timestamps from numbers or numeric strings without failing on garbage.
type FlexibleUnix struct {
	val     int64
	invalid bool
	raw     string
}

func (u *FlexibleUnix) UnmarshalJSON(data []byte) error {
	s := strings.TrimSpace(string(data))
	u.raw = s
	if s == "null" || s == "" || s == `""` {
		u.val = 0
		return nil
	}
	if strings.HasPrefix(s, `"`) && strings.HasSuffix(s, `"`) && len(s) >= 2 {
		s = s[1 : len(s)-1]
	}
	s = strings.TrimSpace(s)
	if s == "" {
		u.val = 0
		return nil
	}
	n, err := strconv.ParseInt(s, 10, 64)
	if err == nil {
		u.val = n
		return nil
	}
	f, err := strconv.ParseFloat(s, 64)
	if err == nil {
		u.val = int64(f)
		return nil
	}
	// On invalid format, do not abort JSON unmarshaling; mark as invalid and use 0.
	u.invalid = true
	u.val = 0
	return nil
}

func (u FlexibleUnix) MarshalJSON() ([]byte, error) {
	return []byte(strconv.FormatInt(u.val, 10)), nil
}

func (u FlexibleUnix) Value() int64 {
	return u.val
}

func (u FlexibleUnix) IsInvalid() bool {
	return u.invalid
}

func (u FlexibleUnix) Raw() string {
	return u.raw
}

// HappProfile is the deserialized Happ routing profile JSON structure.
type HappProfile struct {
	Name           string        `json:"Name"`
	GlobalProxy    *FlexibleBool `json:"GlobalProxy"`
	DirectSites    []string      `json:"DirectSites"`
	DirectIp       []string      `json:"DirectIp"`
	ProxySites     []string      `json:"ProxySites"`
	ProxyIp        []string      `json:"ProxyIp"`
	BlockSites     []string      `json:"BlockSites"`
	BlockIp        []string      `json:"BlockIp"`
	DomainStrategy string        `json:"DomainStrategy"`
	Geoipurl       string        `json:"Geoipurl"`
	Geositeurl     string        `json:"Geositeurl"`
	LastUpdated    FlexibleUnix  `json:"LastUpdated"`

	// DNS section. Kept in raw form here and normalized by lib/happdns so a
	// malformed value never fails the whole profile import.
	RemoteDNSType     string          `json:"RemoteDNSType,omitempty"`
	RemoteDNSDomain   string          `json:"RemoteDNSDomain,omitempty"`
	RemoteDNSIP       string          `json:"RemoteDNSIP,omitempty"`
	DomesticDNSType   string          `json:"DomesticDNSType,omitempty"`
	DomesticDNSDomain string          `json:"DomesticDNSDomain,omitempty"`
	DomesticDNSIP     string          `json:"DomesticDNSIP,omitempty"`
	DnsHosts          json.RawMessage `json:"DnsHosts,omitempty"`
	FakeDNS           json.RawMessage `json:"FakeDNS,omitempty"`
}

// ParsedLink holds the result of parsing a happ://routing/* URI.
type ParsedLink struct {
	Action  Action
	Raw     string
	Profile *HappProfile // nil for ActionOff
	RawJSON json.RawMessage
}

// ParseLink parses a happ://routing link.
func ParseLink(raw string) (ParsedLink, error) {
	trimmed := strings.TrimSpace(raw)
	if trimmed == "" {
		return ParsedLink{}, errors.New("empty happ link")
	}

	// If percent-encoded scheme/host e.g. happ%3A%2F%2F
	if strings.Contains(strings.ToLower(trimmed), "%3a%2f%2f") {
		if unescaped, err := url.QueryUnescape(trimmed); err == nil {
			trimmed = unescaped
		}
	}

	const prefix = "happ://routing/"
	if !strings.HasPrefix(strings.ToLower(trimmed), prefix) {
		return ParsedLink{}, fmt.Errorf("invalid happ routing link scheme: %s", raw)
	}

	rest := trimmed[len(prefix):]
	restLower := strings.ToLower(rest)

	if restLower == "off" || strings.HasPrefix(restLower, "off/") || strings.HasPrefix(restLower, "off?") {
		return ParsedLink{
			Action:  ActionOff,
			Raw:     raw,
			Profile: nil,
			RawJSON: nil,
		}, nil
	}

	var action Action
	var payload string

	if strings.HasPrefix(restLower, "add/") {
		action = ActionAdd
		payload = rest[4:]
	} else if strings.HasPrefix(restLower, "onadd/") {
		action = ActionOnAdd
		payload = rest[6:]
	} else {
		return ParsedLink{}, fmt.Errorf("unknown happ routing action in link: %s", raw)
	}

	decodedBytes, err := decodeBase64Tolerant(payload)
	if err != nil {
		return ParsedLink{}, fmt.Errorf("failed to decode happ routing payload: %w", err)
	}

	if len(decodedBytes) > MaxPayloadBytes {
		return ParsedLink{}, fmt.Errorf("happ routing payload exceeds 1 MiB limit (%d bytes)", len(decodedBytes))
	}

	if !json.Valid(decodedBytes) {
		return ParsedLink{}, errors.New("happ routing payload is not valid JSON")
	}

	var profile HappProfile
	if err := json.Unmarshal(decodedBytes, &profile); err != nil {
		return ParsedLink{}, fmt.Errorf("failed to unmarshal happ profile: %w", err)
	}

	profile.Name = strings.TrimSpace(profile.Name)
	if profile.Name == "" {
		return ParsedLink{}, errors.New("happ routing profile Name is required")
	}

	return ParsedLink{
		Action:  action,
		Raw:     raw,
		Profile: &profile,
		RawJSON: json.RawMessage(decodedBytes),
	}, nil
}

// decodeBase64Tolerant decodes standard, raw, URL-safe and raw URL-safe Base64 strings.
// Handles whitespace, percent-encoded characters, and space-to-plus conversion.
func decodeBase64Tolerant(s string) ([]byte, error) {
	// Strip carriage returns, newlines, and tabs
	s = strings.ReplaceAll(s, "\r", "")
	s = strings.ReplaceAll(s, "\n", "")
	s = strings.ReplaceAll(s, "\t", "")
	s = strings.TrimSpace(s)

	if s == "" {
		return nil, errors.New("empty payload")
	}

	// If payload contains percent-encoding like %2B or %2F, unescape using PathUnescape
	// (PathUnescape preserves '+' while unescaping %XX)
	if strings.Contains(s, "%") {
		if unescaped, err := url.PathUnescape(s); err == nil {
			s = unescaped
		}
	}

	tryEncodings := func(input string) ([]byte, error) {
		encodings := []*base64.Encoding{
			base64.StdEncoding,
			base64.RawStdEncoding,
			base64.URLEncoding,
			base64.RawURLEncoding,
		}
		for _, enc := range encodings {
			if b, err := enc.DecodeString(input); err == nil {
				return b, nil
			}
		}
		return nil, errors.New("base64 decoding failed")
	}

	if res, err := tryEncodings(s); err == nil {
		return res, nil
	}

	// Try replacing spaces with '+' in case they were altered in transmission
	if strings.Contains(s, " ") {
		withPlus := strings.ReplaceAll(s, " ", "+")
		if res, err := tryEncodings(withPlus); err == nil {
			return res, nil
		}
	}

	return nil, errors.New("unable to decode base64 payload with any supported encoding")
}

func cleanSlice(items []string) []string {
	var res []string
	for _, item := range items {
		t := strings.TrimSpace(item)
		if t != "" {
			res = append(res, t)
		}
	}
	return res
}

// ToRoutingConfig converts a HappProfile into a structs.RoutingConfig according to the strict priority rules.
// Rules priority:
// 1. LAN -> direct (built-in, always first)
// 2. BlockSites -> block (domain rule)
// 3. BlockIp -> block (ip rule)
// 4. DirectSites -> direct (domain rule)
// 5. DirectIp -> direct (ip rule)
// 6. ProxySites -> proxy (domain rule)
// 7. ProxyIp -> proxy (ip rule)
// 8. If GlobalProxy == false: catch-all rule (network "tcp,udp" -> direct)
func ToRoutingConfig(p HappProfile) (structs.RoutingConfig, []string, error) {
	var warnings []string

	if strings.TrimSpace(p.Name) == "" {
		return structs.RoutingConfig{}, nil, errors.New("profile Name is required")
	}

	// DNS is additive: invalid values are reported as warnings and never fail
	// the whole profile.
	dnsCfg, dnsWarnings := happdns.Parse(happdns.Raw{
		RemoteDNSType:     p.RemoteDNSType,
		RemoteDNSDomain:   p.RemoteDNSDomain,
		RemoteDNSIP:       p.RemoteDNSIP,
		DomesticDNSType:   p.DomesticDNSType,
		DomesticDNSDomain: p.DomesticDNSDomain,
		DomesticDNSIP:     p.DomesticDNSIP,
		DnsHosts:          p.DnsHosts,
		FakeDNS:           p.FakeDNS,
	})
	warnings = append(warnings, dnsWarnings...)

	// 1. Normalize DomainStrategy
	strategy := strings.TrimSpace(p.DomainStrategy)
	canonicalStrategy := "IPIfNonMatch"
	switch strings.ToLower(strategy) {
	case "":
		canonicalStrategy = "IPIfNonMatch"
	case "asis":
		canonicalStrategy = "AsIs"
	case "ipifnonmatch":
		canonicalStrategy = "IPIfNonMatch"
	case "ipondemand":
		canonicalStrategy = "IPOnDemand"
	default:
		warnings = append(warnings, fmt.Sprintf("unknown domain strategy %q, fallback to IPIfNonMatch", strategy))
		canonicalStrategy = "IPIfNonMatch"
	}

	// 2. Determine GlobalProxy
	globalProxy := true
	if p.GlobalProxy != nil {
		globalProxy = p.GlobalProxy.Value()
	}

	// 3. Validate Geo URLs
	if p.Geoipurl != "" {
		u, err := url.Parse(p.Geoipurl)
		if err != nil || u.Scheme != "https" {
			warnings = append(warnings, fmt.Sprintf("Geoipurl must use https scheme: %s", p.Geoipurl))
		}
	}
	if p.Geositeurl != "" {
		u, err := url.Parse(p.Geositeurl)
		if err != nil || u.Scheme != "https" {
			warnings = append(warnings, fmt.Sprintf("Geositeurl must use https scheme: %s", p.Geositeurl))
		}
	}

	// 4. Check LastUpdated
	if p.LastUpdated.IsInvalid() {
		warnings = append(warnings, fmt.Sprintf("invalid LastUpdated value %q, fallback to 0", p.LastUpdated.Raw()))
	}

	var rules []structs.RoutingRule

	// 5. Built-in LAN rule (always first)
	rules = append(rules, structs.RoutingRule{
		Type:        "field",
		IP:          LANRuleIP,
		OutboundTag: "direct",
		Enabled:     true,
	})

	// 6. BlockSites (domain)
	if sites := cleanSlice(p.BlockSites); len(sites) > 0 {
		rules = append(rules, structs.RoutingRule{
			Type:        "field",
			Domain:      strings.Join(sites, ","),
			OutboundTag: "block",
			Enabled:     true,
		})
	}

	// 7. BlockIp (ip)
	if ips := cleanSlice(p.BlockIp); len(ips) > 0 {
		rules = append(rules, structs.RoutingRule{
			Type:        "field",
			IP:          strings.Join(ips, ","),
			OutboundTag: "block",
			Enabled:     true,
		})
	}

	// 8. DirectSites (domain)
	if sites := cleanSlice(p.DirectSites); len(sites) > 0 {
		rules = append(rules, structs.RoutingRule{
			Type:        "field",
			Domain:      strings.Join(sites, ","),
			OutboundTag: "direct",
			Enabled:     true,
		})
	}

	// 9. DirectIp (ip) - avoid repeating the exact LAN rule if present
	if ips := cleanSlice(p.DirectIp); len(ips) > 0 {
		ipStr := strings.Join(ips, ",")
		if ipStr != LANRuleIP {
			rules = append(rules, structs.RoutingRule{
				Type:        "field",
				IP:          ipStr,
				OutboundTag: "direct",
				Enabled:     true,
			})
		}
	}

	// 10. ProxySites (domain)
	if sites := cleanSlice(p.ProxySites); len(sites) > 0 {
		rules = append(rules, structs.RoutingRule{
			Type:        "field",
			Domain:      strings.Join(sites, ","),
			OutboundTag: "proxy",
			Enabled:     true,
		})
	}

	// 11. ProxyIp (ip)
	if ips := cleanSlice(p.ProxyIp); len(ips) > 0 {
		rules = append(rules, structs.RoutingRule{
			Type:        "field",
			IP:          strings.Join(ips, ","),
			OutboundTag: "proxy",
			Enabled:     true,
		})
	}

	// 12. Catch-all when GlobalProxy == false
	if !globalProxy {
		rules = append(rules, structs.RoutingRule{
			Type:        "field",
			Network:     "tcp,udp",
			OutboundTag: "direct",
			Enabled:     true,
		})
	}

	return structs.RoutingConfig{
		DomainStrategy: canonicalStrategy,
		Rules:          rules,
		DNS:            dnsCfg,
	}, warnings, nil
}
