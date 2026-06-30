package utils

import (
	"fmt"
	"net"
	"sort"
	"time"
	"whoisthat-core/lib/logger"

	"github.com/miekg/dns"
)

type ResolvedIPs struct {
	IPv4 []string
	IPv6 []string
}

func ResolveDomain(domain string, dnsServers []string) (*ResolvedIPs, error) {
	result := &ResolvedIPs{}
	ipSet4 := make(map[string]bool)
	ipSet6 := make(map[string]bool)

	for _, server := range dnsServers {
		v4, err := queryDNSServer(domain, server+":53", dns.TypeA)
		if err != nil {
			logger.Warnf("A query to %s failed: %v", server, err)
		} else {
			for _, ip := range v4 {
				ipSet4[ip] = true
			}
		}

		v6, err := queryDNSServer(domain, server+":53", dns.TypeAAAA)
		if err != nil {
			logger.Warnf("AAAA query to %s failed: %v", server, err)
		} else {
			for _, ip := range v6 {
				ipSet6[ip] = true
			}
		}
	}

	for ip := range ipSet4 {
		result.IPv4 = append(result.IPv4, ip)
	}
	for ip := range ipSet6 {
		result.IPv6 = append(result.IPv6, ip)
	}
	sort.Strings(result.IPv4)
	sort.Strings(result.IPv6)

	if len(result.IPv4) == 0 && len(result.IPv6) == 0 {
		logger.Warn("No IPs found via DNS servers, falling back to system resolver")
		systemIPs, err := ResolveDomainSystem(domain)
		if err != nil {
			return nil, fmt.Errorf("DNS servers and system resolver both failed: %w", err)
		}
		return systemIPs, nil
	}

	return result, nil
}

func ResolveDomainIpv4(domain string, dnsServers []string) ([]string, error) {
	r, err := ResolveDomain(domain, dnsServers)
	if err != nil {
		return nil, err
	}
	if len(r.IPv4) == 0 {
		return nil, fmt.Errorf("no A records found for %s", domain)
	}
	return r.IPv4, nil
}

func queryDNSServer(domain, server string, recordType uint16) ([]string, error) {
	var ips []string

	udpClient := &dns.Client{
		Net:     "udp",
		Timeout: 5 * time.Second,
	}

	msg := new(dns.Msg)
	msg.SetQuestion(dns.Fqdn(domain), recordType)
	msg.RecursionDesired = true
	msg.SetEdns0(4096, false) // Disable DNS cookies

	resp, _, err := udpClient.Exchange(msg, server)
	if err != nil {
		return nil, fmt.Errorf("UDP query failed: %w", err)
	}

	if resp.Truncated {
		tcpClient := &dns.Client{
			Net:     "tcp",
			Timeout: 10 * time.Second,
		}
		resp, _, err = tcpClient.Exchange(msg, server)
		if err != nil {
			return nil, fmt.Errorf("TCP retry failed: %w", err)
		}
	}

	for _, answer := range resp.Answer {
		switch recordType {
		case dns.TypeA:
			if a, ok := answer.(*dns.A); ok {
				ips = append(ips, a.A.String())
			}
		case dns.TypeAAAA:
			if a, ok := answer.(*dns.AAAA); ok {
				ips = append(ips, a.AAAA.String())
			}
		}
	}

	return ips, nil
}

func ResolveDomainSystem(domain string) (*ResolvedIPs, error) {
	ips, err := net.LookupIP(domain)
	if err != nil {
		return nil, err
	}

	result := &ResolvedIPs{}
	for _, ip := range ips {
		if ip.To4() != nil {
			result.IPv4 = append(result.IPv4, ip.String())
		} else if ip.To16() != nil {
			result.IPv6 = append(result.IPv6, ip.String())
		}
	}

	if len(result.IPv4) == 0 && len(result.IPv6) == 0 {
		return result, fmt.Errorf("no IPs found for domain: %s", domain)
	}

	return result, nil
}
