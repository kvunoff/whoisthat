package mainproxy

import (
	"fmt"
	"net"
	"strings"

	appconfig "whoisthat-core/lib/AppConfig"
	"whoisthat-core/lib/xrayjson"
	"whoisthat-core/structs"
)

// prepareManagedConfig builds the runtime config for a JSON-subscription
// profile: the provider routing stays intact while manual rules are prepended.
func (p *ProxyManager) prepareManagedConfig(
	profile structs.Profile,
	socksPort, httpPort int,
	tunName string,
	apiPort int,
	manual structs.RoutingConfig,
	bootstrap map[string][]string,
) ([]byte, []string, error) {
	if p.DB == nil {
		return nil, nil, fmt.Errorf("managed profile requires a database")
	}
	raw, err := p.DB.LoadProfileXrayConfig(profile.GroupId, profile.Id)
	if err != nil {
		return nil, nil, fmt.Errorf("load provider config: %w", err)
	}

	prepared, err := xrayjson.Prepare(raw, xrayjson.Options{
		SocksPort:          socksPort,
		HttpPort:           httpPort,
		TunName:            tunName,
		APIPort:            apiPort,
		DNSServers:         appconfig.GetConfig().DnsServers,
		BootstrapHosts:     bootstrap,
		UserRules:          manual.Rules,
		UserDomainStrategy: manual.DomainStrategy,
	})
	if err != nil {
		return nil, nil, err
	}

	cfg, err := injectStatsConfig(prepared.Config, apiPort)
	if err != nil {
		return nil, nil, err
	}
	return cfg, prepared.ProxyTags, nil
}

// managedSource returns everything Connect/reload needs before building the
// config: resolved server IPs and bootstrap DNS entries for all outbounds.
func (p *ProxyManager) managedSource(profile structs.Profile) ([]string, map[string][]string, error) {
	if p.DB == nil {
		return nil, nil, fmt.Errorf("managed profile requires a database")
	}
	raw, err := p.DB.LoadProfileXrayConfig(profile.GroupId, profile.Id)
	if err != nil {
		return nil, nil, fmt.Errorf("load provider config: %w", err)
	}
	ips, hosts := resolveServerRefs(xrayjson.ServerAddresses(raw))
	return ips, hosts, nil
}

// resolveServerRefs resolves every outbound server address once, before TUN
// takes over DNS. It returns the deduplicated IP list (killswitch, TUN routes)
// and a domain -> IPs map for the Xray dns.hosts bootstrap section.
func resolveServerRefs(refs []xrayjson.HostPort) ([]string, map[string][]string) {
	var ips []string
	hosts := map[string][]string{}
	seenIP := map[string]bool{}
	seenHost := map[string]bool{}

	addIP := func(ip string) {
		if ip == "" || seenIP[ip] {
			return
		}
		seenIP[ip] = true
		ips = append(ips, ip)
	}

	for _, ref := range refs {
		host := strings.TrimSpace(ref.Host)
		if host == "" {
			continue
		}
		if net.ParseIP(host) != nil {
			addIP(host)
			continue
		}
		if seenHost[host] {
			continue
		}
		seenHost[host] = true
		resolved, err := net.LookupIP(host)
		if err != nil {
			continue
		}
		for _, ip := range resolved {
			addIP(ip.String())
			hosts[host] = append(hosts[host], ip.String())
		}
	}
	return ips, hosts
}
