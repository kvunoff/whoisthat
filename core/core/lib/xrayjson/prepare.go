package xrayjson

import (
	"encoding/json"
	"errors"
	"fmt"

	"whoisthat-core/structs"
)

// Options controls the whoisthat-specific adaptations applied to a provider's
// full Xray configuration. Everything not mentioned here (routing rules,
// balancers, DNS servers, outbound transports) is preserved verbatim.
type Options struct {
	SocksPort int
	HttpPort  int
	TunName   string
	APIPort   int

	DNSServers         []string
	BootstrapHosts     map[string][]string
	UserRules          []structs.RoutingRule
	UserDomainStrategy string
	// GlobalGeoExt is set when the runtime asset directory also contains
	// geoip-global.dat/geosite-global.dat next to the provider's own geo files.
	// Manual rules with global geo codes are then compiled against those copies.
	GlobalGeoExt bool
}

// Prepared is the runtime configuration plus metadata the proxy manager needs.
type Prepared struct {
	Config    []byte
	ProxyTags []string
}

// Prepare merges the manual (user) rules into the provider configuration and
// applies the structural changes whoisthat needs: its own inbounds, stats API,
// TUN DNS capture, fwmark on freedom outbounds and bootstrap DNS hosts.
//
// Rule priority is manual rules first, provider rules after, so a user override
// always wins while the provider configuration remains the default policy.
func Prepare(raw json.RawMessage, opts Options) (Prepared, error) {
	var config map[string]interface{}
	if err := json.Unmarshal(raw, &config); err != nil {
		return Prepared{}, fmt.Errorf("parse xray config: %w", err)
	}

	outbounds := asSlice(config["outbounds"])
	if len(outbounds) == 0 {
		return Prepared{}, errors.New("xray config has no outbounds")
	}
	proxyTags := collectProxyTags(outbounds)
	if len(proxyTags) == 0 {
		return Prepared{}, errors.New("xray config has no proxy outbound")
	}

	outbounds = ensureServiceOutbounds(outbounds)
	markFreedomOutbounds(outbounds)
	fixDanglingDialerProxy(outbounds, proxyTags)
	config["outbounds"] = outbounds

	config["inbounds"] = rewriteInbounds(config["inbounds"], opts)

	config["dns"] = mergeDNS(config["dns"], opts)

	routing := asMap(config["routing"])
	config["routing"] = mergeRouting(routing, outbounds, proxyTags, opts)

	result, err := json.Marshal(config)
	if err != nil {
		return Prepared{}, fmt.Errorf("marshal xray config: %w", err)
	}

	return Prepared{
		Config:    result,
		ProxyTags: proxyTags,
	}, nil
}

func rewriteInbounds(value interface{}, opts Options) []interface{} {
	var out []interface{}
	for _, raw := range asSlice(value) {
		inbound, ok := raw.(map[string]interface{})
		if !ok {
			continue
		}
		protocol, _ := inbound["protocol"].(string)
		switch protocol {
		case "socks":
			if opts.SocksPort <= 0 {
				continue
			}
			inbound["port"] = opts.SocksPort
			inbound["listen"] = "127.0.0.1"
			inbound["sniffing"] = sniffing(inbound["sniffing"])
			out = append(out, inbound)
		case "http":
			if opts.HttpPort <= 0 {
				continue
			}
			inbound["port"] = opts.HttpPort
			inbound["listen"] = "127.0.0.1"
			inbound["sniffing"] = sniffing(inbound["sniffing"])
			out = append(out, inbound)
		}
	}

	if opts.SocksPort > 0 && !hasInboundProtocol(out, "socks") {
		out = append(out, map[string]interface{}{
			"tag":      "socks-in",
			"protocol": "socks",
			"port":     opts.SocksPort,
			"listen":   "127.0.0.1",
			"settings": map[string]interface{}{"udp": true, "auth": "noauth"},
			"sniffing": sniffing(nil),
		})
	}
	if opts.HttpPort > 0 && !hasInboundProtocol(out, "http") {
		out = append(out, map[string]interface{}{
			"tag":      "http-in",
			"protocol": "http",
			"port":     opts.HttpPort,
			"listen":   "127.0.0.1",
			"sniffing": sniffing(nil),
		})
	}
	if opts.TunName != "" {
		out = append(out, map[string]interface{}{
			"tag":      "tun-in",
			"protocol": "tun",
			"settings": map[string]interface{}{"name": opts.TunName, "MTU": 1400},
			"sniffing": map[string]interface{}{
				"enabled":      true,
				"routeOnly":    false,
				"metadataOnly": false,
				"destOverride": []string{"http", "tls"},
			},
		})
	}
	return out
}

func sniffing(value interface{}) map[string]interface{} {
	s := asMap(value)
	s["enabled"] = true
	s["routeOnly"] = false
	if _, ok := s["destOverride"]; !ok {
		s["destOverride"] = []string{"http", "tls"}
	}
	return s
}

func hasInboundProtocol(inbounds []interface{}, protocol string) bool {
	for _, raw := range inbounds {
		if inbound, ok := raw.(map[string]interface{}); ok && inbound["protocol"] == protocol {
			return true
		}
	}
	return false
}

func mergeDNS(value interface{}, opts Options) map[string]interface{} {
	dns := asMap(value)
	if servers, ok := dns["servers"].([]interface{}); !ok || len(servers) == 0 {
		configured := opts.DNSServers
		if len(configured) == 0 {
			configured = []string{"1.1.1.1", "8.8.8.8"}
		}
		list := make([]interface{}, 0, len(configured))
		for _, address := range configured {
			list = append(list, map[string]interface{}{"address": address})
		}
		dns["servers"] = list
	}

	hosts := asMap(dns["hosts"])
	for domain, ips := range opts.BootstrapHosts {
		if _, exists := hosts[domain]; exists {
			continue
		}
		if len(ips) == 1 {
			hosts[domain] = ips[0]
		} else if len(ips) > 1 {
			hosts[domain] = ips
		}
	}
	if len(hosts) > 0 {
		dns["hosts"] = hosts
	}
	return dns
}
