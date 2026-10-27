// Package happdns parses and normalizes the DNS section of Happ routing
// profiles.
//
// It is a leaf package (stdlib + structs only) so both the Happ importer
// (lib/happ) and the database layer (db) can use it without creating an import
// cycle: lib/happ already imports db.
package happdns

import (
	"encoding/json"
	"fmt"
	"net"
	"net/url"
	"strings"

	"whoisthat-core/structs"
)

// FlexibleBool parses boolean values from bool, quoted bool, numeric 1/0 or strings.
type FlexibleBool bool

func (b *FlexibleBool) UnmarshalJSON(data []byte) error {
	v, err := parseBool(data)
	if err != nil {
		return err
	}
	*b = FlexibleBool(v)
	return nil
}

func (b FlexibleBool) MarshalJSON() ([]byte, error) {
	if b {
		return []byte("true"), nil
	}
	return []byte("false"), nil
}

func (b *FlexibleBool) Value() bool {
	if b == nil {
		return false
	}
	return bool(*b)
}

// Raw is the DNS subset of a Happ routing profile. It is used both by the
// profile parser and for lazy backfill of profiles imported before DNS support.
type Raw struct {
	RemoteDNSType     string          `json:"RemoteDNSType,omitempty"`
	RemoteDNSDomain   string          `json:"RemoteDNSDomain,omitempty"`
	RemoteDNSIP       string          `json:"RemoteDNSIP,omitempty"`
	DomesticDNSType   string          `json:"DomesticDNSType,omitempty"`
	DomesticDNSDomain string          `json:"DomesticDNSDomain,omitempty"`
	DomesticDNSIP     string          `json:"DomesticDNSIP,omitempty"`
	DnsHosts          json.RawMessage `json:"DnsHosts,omitempty"`
	FakeDNS           json.RawMessage `json:"FakeDNS,omitempty"`
}

// Parse normalizes a Happ DNS section. Invalid values never fail the profile:
// they are reported as warnings and replaced by a safe fallback (or dropped).
// Returns nil when the profile carries no usable DNS configuration.
func Parse(raw Raw) (*structs.HappDNSConfig, []string) {
	var warnings []string

	cfg := &structs.HappDNSConfig{}

	fake, err := parseBool(raw.FakeDNS)
	if err != nil {
		warnings = append(warnings, fmt.Sprintf("FakeDNS: %v", err))
	}
	cfg.FakeDNSEnabled = fake

	cfg.RemoteDNSType, cfg.RemoteDNSDomain, cfg.RemoteDNSIP =
		normalizeServer("RemoteDNS", raw.RemoteDNSType, raw.RemoteDNSDomain, raw.RemoteDNSIP, &warnings)
	cfg.DomesticDNSType, cfg.DomesticDNSDomain, cfg.DomesticDNSIP =
		normalizeServer("DomesticDNS", raw.DomesticDNSType, raw.DomesticDNSDomain, raw.DomesticDNSIP, &warnings)
	cfg.Hosts = parseHosts(raw.DnsHosts, &warnings)

	if !cfg.FakeDNSEnabled &&
		cfg.RemoteDNSType == "" &&
		cfg.DomesticDNSType == "" &&
		len(cfg.Hosts) == 0 {
		return nil, warnings
	}

	return cfg, warnings
}

// ParseRawJSON is a convenience wrapper for lazy backfill of RoutingProfile.RawJSON.
func ParseRawJSON(raw json.RawMessage) (*structs.HappDNSConfig, []string) {
	if len(raw) == 0 {
		return nil, nil
	}
	var r Raw
	if err := json.Unmarshal(raw, &r); err != nil {
		return nil, []string{fmt.Sprintf("DNS: failed to decode profile JSON: %v", err)}
	}
	return Parse(r)
}

// RemoteLabel returns a short human-readable description for the remote DNS
// server, e.g. "DoH cloudflare-dns.com" or "DoU 1.1.1.1".
func RemoteLabel(cfg *structs.HappDNSConfig) string {
	if cfg == nil {
		return ""
	}
	return serverLabel(cfg.RemoteDNSType, cfg.RemoteDNSDomain, cfg.RemoteDNSIP)
}

// DomesticLabel returns a short human-readable description for the domestic
// DNS server.
func DomesticLabel(cfg *structs.HappDNSConfig) string {
	if cfg == nil {
		return ""
	}
	return serverLabel(cfg.DomesticDNSType, cfg.DomesticDNSDomain, cfg.DomesticDNSIP)
}

func serverLabel(kind, domain, ip string) string {
	if kind == "" {
		return ""
	}
	addr := domain
	if kind == "DoU" || addr == "" {
		addr = ip
	}
	addr = displayHost(addr)
	if addr == "" {
		return kind
	}
	return kind + " " + addr
}

func displayHost(addr string) string {
	if addr == "" {
		return ""
	}
	if u, err := url.Parse(addr); err == nil && u.Host != "" {
		return u.Host
	}
	return addr
}

// normalizeServer canonicalizes one server pair. It returns ("", "", "") when
// the pair cannot be used.
func normalizeServer(label, kind, domain, ip string, warnings *[]string) (string, string, string) {
	kind = strings.TrimSpace(kind)
	domain = strings.TrimSpace(domain)
	ip = strings.TrimSpace(ip)

	validIP := ""
	if ip != "" {
		if net.ParseIP(ip) == nil {
			*warnings = append(*warnings, fmt.Sprintf("%s: %q is not a valid IP address", label, ip))
		} else {
			validIP = ip
		}
	}

	canonical := canonicalType(label, kind, warnings)
	if canonical == "" {
		switch {
		case domain != "":
			canonical = "DoH"
		case validIP != "":
			canonical = "DoU"
		default:
			return "", "", ""
		}
	}

	switch canonical {
	case "DoH":
		if domain == "" {
			*warnings = append(*warnings, fmt.Sprintf("%s: DoH selected but no URL/domain provided", label))
			if validIP != "" {
				*warnings = append(*warnings, fmt.Sprintf("%s: falling back to DoU", label))
				return "DoU", "", validIP
			}
			return "", "", ""
		}
		normalized, ok := normalizeDoHURL(label, domain, warnings)
		if !ok {
			if validIP != "" {
				*warnings = append(*warnings, fmt.Sprintf("%s: falling back to DoU", label))
				return "DoU", "", validIP
			}
			return "", "", ""
		}
		return "DoH", normalized, validIP
	case "DoU":
		if validIP == "" {
			*warnings = append(*warnings, fmt.Sprintf("%s: DoU requires a valid IP address", label))
			return "", "", ""
		}
		return "DoU", "", validIP
	default:
		return "", "", ""
	}
}

func canonicalType(label, kind string, warnings *[]string) string {
	switch strings.ToLower(strings.TrimSpace(kind)) {
	case "":
		return ""
	case "doh", "https":
		return "DoH"
	case "dou", "udp", "tcp", "plain":
		return "DoU"
	default:
		*warnings = append(*warnings, fmt.Sprintf("%s: unknown DNS type %q", label, kind))
		return ""
	}
}

func normalizeDoHURL(label, domain string, warnings *[]string) (string, bool) {
	if !strings.Contains(domain, "://") {
		domain = "https://" + domain
	}
	u, err := url.Parse(domain)
	if err != nil || !strings.EqualFold(u.Scheme, "https") || u.Host == "" {
		*warnings = append(*warnings, fmt.Sprintf("%s: invalid DoH URL %q (must be https://host[/path])", label, domain))
		return "", false
	}
	return u.String(), true
}

func parseHosts(raw json.RawMessage, warnings *[]string) map[string]string {
	if len(raw) == 0 || strings.TrimSpace(string(raw)) == "null" {
		return nil
	}

	var obj map[string]json.RawMessage
	if err := json.Unmarshal(raw, &obj); err != nil {
		*warnings = append(*warnings, fmt.Sprintf("DnsHosts: expected an object of domain -> IP: %v", err))
		return nil
	}

	hosts := make(map[string]string, len(obj))
	for domain, value := range obj {
		d := strings.TrimSpace(domain)
		if d == "" {
			*warnings = append(*warnings, "DnsHosts: ignoring empty domain")
			continue
		}

		var single string
		if err := json.Unmarshal(value, &single); err == nil {
			single = strings.TrimSpace(single)
			if net.ParseIP(single) == nil {
				*warnings = append(*warnings, fmt.Sprintf("DnsHosts: ignoring %q: %q is not an IP address", d, single))
				continue
			}
			hosts[d] = single
			continue
		}

		// Tolerate array values: keep the first literal IP.
		var list []string
		if err := json.Unmarshal(value, &list); err == nil {
			picked := ""
			for _, item := range list {
				item = strings.TrimSpace(item)
				if net.ParseIP(item) != nil {
					picked = item
					break
				}
			}
			if picked == "" {
				*warnings = append(*warnings, fmt.Sprintf("DnsHosts: ignoring %q: no literal IP in value", d))
				continue
			}
			hosts[d] = picked
			continue
		}

		*warnings = append(*warnings, fmt.Sprintf("DnsHosts: ignoring %q: value must be an IP string", d))
	}

	if len(hosts) == 0 {
		return nil
	}
	return hosts
}

func parseBool(raw json.RawMessage) (bool, error) {
	s := strings.TrimSpace(string(raw))
	if s == "" || s == "null" {
		return false, nil
	}
	if strings.HasPrefix(s, `"`) && strings.HasSuffix(s, `"`) && len(s) >= 2 {
		s = s[1 : len(s)-1]
	}
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "true", "1", "t", "yes":
		return true, nil
	case "false", "0", "f", "no":
		return false, nil
	default:
		return false, fmt.Errorf("invalid boolean value: %s", string(raw))
	}
}
