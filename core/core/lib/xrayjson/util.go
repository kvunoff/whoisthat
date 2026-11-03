package xrayjson

import "strings"

func asMap(value interface{}) map[string]interface{} {
	if m, ok := value.(map[string]interface{}); ok {
		return m
	}
	return map[string]interface{}{}
}

func asSlice(value interface{}) []interface{} {
	if s, ok := value.([]interface{}); ok {
		return s
	}
	return nil
}

func asStringSlice(value interface{}) []string {
	switch v := value.(type) {
	case []string:
		return v
	case []interface{}:
		out := make([]string, 0, len(v))
		for _, raw := range v {
			part, ok := raw.(string)
			if !ok {
				return nil
			}
			out = append(out, part)
		}
		return out
	default:
		return nil
	}
}

func ensureServiceOutbounds(outbounds []interface{}) []interface{} {
	outbounds = ensureOutbound(outbounds, "direct", "freedom", map[string]interface{}{
		"domainStrategy": "UseIP",
	})
	outbounds = ensureOutbound(outbounds, "block", "blackhole", map[string]interface{}{})
	outbounds = ensureOutbound(outbounds, "dns-out", "dns", map[string]interface{}{})
	return outbounds
}

func ensureOutbound(outbounds []interface{}, tag, protocol string, settings map[string]interface{}) []interface{} {
	for _, raw := range outbounds {
		if m, ok := raw.(map[string]interface{}); ok && m["tag"] == tag {
			return outbounds
		}
	}
	return append(outbounds, map[string]interface{}{
		"tag":      tag,
		"protocol": protocol,
		"settings": settings,
	})
}

// rewriteGeoList points manual geo rules at the global copies that
// geo.EnsureGlobalCopies places next to the provider's own assets. Provider
// rules are left untouched and keep using geoip.dat/geosite.dat.
func rewriteGeoList(values []string, enabled, ip bool) []string {
	if !enabled {
		return values
	}
	out := make([]string, 0, len(values))
	for _, value := range values {
		out = append(out, rewriteGeo(value, ip))
	}
	return out
}

func rewriteGeo(value string, ip bool) string {
	if ip {
		switch {
		case strings.HasPrefix(value, "geoip:ext:"):
			return value
		case strings.HasPrefix(value, "geoip:"):
			return "ext-ip:geoip-global.dat:" + strings.TrimPrefix(value, "geoip:")
		}
		return value
	}
	switch {
	case strings.HasPrefix(value, "geosite:ext:"):
		return value
	case strings.HasPrefix(value, "geosite:"):
		return "ext-domain:geosite-global.dat:" + strings.TrimPrefix(value, "geosite:")
	}
	return value
}

func splitAndTrim(value string) []string {
	parts := strings.Split(value, ",")
	out := make([]string, 0, len(parts))
	for _, part := range parts {
		if part = strings.TrimSpace(part); part != "" {
			out = append(out, part)
		}
	}
	return out
}

func contains(list []string, value string) bool {
	for _, item := range list {
		if item == value {
			return true
		}
	}
	return false
}
