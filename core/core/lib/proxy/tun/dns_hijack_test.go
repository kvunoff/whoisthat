package tunmode

import (
	"os/exec"
	"strings"
	"testing"
)

func TestBuildDnsHijackNftHapp(t *testing.T) {
	d := sanitizeDnsHijack(DnsHijack{
		Enabled:   true,
		Happ:      true,
		ScopeMark: 1,
		Exclude:   []string{"203.0.113.7", "2001:db8::7", "not-an-ip"},
	})

	v4 := buildDnsHijackNftScript("ip", "eth0", d)
	for _, want := range []string{
		"nft delete table ip whoisthat",
		"nft add rule ip whoisthat output meta mark 1 return",
		"nft add rule ip whoisthat output ip daddr 198.18.0.53 return",
		"nft add rule ip whoisthat output ip daddr 203.0.113.7 return",
		"nft add rule ip whoisthat output udp dport 53 dnat to 198.18.0.53:53",
		"nft add rule ip whoisthat output tcp dport 53 dnat to 198.18.0.53:53",
		"nft add rule ip whoisthat prerouting udp dport 53 dnat to 198.18.0.53:53",
	} {
		if !strings.Contains(v4, want) {
			t.Errorf("nft v4 script missing %q:\n%s", want, v4)
		}
	}
	for _, unwanted := range []string{"masquerade", "postrouting", "2001:db8::7"} {
		if strings.Contains(v4, unwanted) {
			t.Errorf("nft v4 script must not contain %q:\n%s", unwanted, v4)
		}
	}

	v6 := buildDnsHijackNftScript("ip6", "eth0", d)
	for _, want := range []string{
		"nft add rule ip6 whoisthat output ip6 daddr fd00::53 return",
		"nft add rule ip6 whoisthat output ip6 daddr 2001:db8::7 return",
		"dnat to [fd00::53]:53",
	} {
		if !strings.Contains(v6, want) {
			t.Errorf("nft v6 script missing %q:\n%s", want, v6)
		}
	}
	if strings.Contains(v6, "203.0.113.7") {
		t.Errorf("nft v6 script leaked a v4 exclude:\n%s", v6)
	}
}

func TestBuildDnsHijackNftLegacy(t *testing.T) {
	d := sanitizeDnsHijack(DnsHijack{
		Enabled: true,
		Legacy:  true,
		Sink4:   "1.1.1.1",
		Sink6:   "2606:4700:4700::1111",
		Exclude: []string{"203.0.113.7"},
	})

	v4 := buildDnsHijackNftScript("ip", "eth0", d)
	for _, want := range []string{
		"nft add chain ip whoisthat postrouting",
		"nft add rule ip whoisthat postrouting ip daddr 1.1.1.1 udp dport 53 oif $IFACE masquerade",
		"nft add rule ip whoisthat output udp dport 53 dnat to 1.1.1.1:53",
	} {
		if !strings.Contains(v4, want) {
			t.Errorf("legacy nft script missing %q:\n%s", want, v4)
		}
	}
	for _, unwanted := range []string{"meta mark", "daddr 1.1.1.1 return", "203.0.113.7 return"} {
		if strings.Contains(v4, unwanted) {
			t.Errorf("legacy nft script must not contain %q:\n%s", unwanted, v4)
		}
	}
}

func TestBuildDnsHijackIptablesHapp(t *testing.T) {
	d := sanitizeDnsHijack(DnsHijack{
		Enabled:   true,
		Happ:      true,
		ScopeMark: 1,
		Exclude:   []string{"203.0.113.7", "2001:db8::7"},
	})

	v4 := buildDnsHijackIptablesScript("iptables", "eth0", d)
	for _, want := range []string{
		"iptables -t nat -N WHOISTHAT_DNS_OUT",
		"iptables -t nat -I OUTPUT 1 -j WHOISTHAT_DNS_OUT",
		"iptables -t nat -A WHOISTHAT_DNS_OUT -m mark --mark 1 -j RETURN",
		"iptables -t nat -A WHOISTHAT_DNS_OUT -d 198.18.0.53 -j RETURN",
		"iptables -t nat -A WHOISTHAT_DNS_OUT -d 203.0.113.7 -j RETURN",
		"iptables -t nat -A WHOISTHAT_DNS_OUT -p udp --dport 53 -j DNAT --to-destination 198.18.0.53:53",
	} {
		if !strings.Contains(v4, want) {
			t.Errorf("iptables v4 script missing %q:\n%s", want, v4)
		}
	}
	for _, unwanted := range []string{"MASQUERADE", "WHOISTHAT_DNS_POST", "2001:db8::7"} {
		if strings.Contains(v4, unwanted) {
			t.Errorf("iptables v4 script must not contain %q:\n%s", unwanted, v4)
		}
	}

	v6 := buildDnsHijackIptablesScript("ip6tables", "eth0", d)
	for _, want := range []string{
		"ip6tables -t nat -A WHOISTHAT_DNS_OUT -d fd00::53 -j RETURN",
		"ip6tables -t nat -A WHOISTHAT_DNS_OUT -d 2001:db8::7 -j RETURN",
		"--to-destination [fd00::53]:53",
	} {
		if !strings.Contains(v6, want) {
			t.Errorf("iptables v6 script missing %q:\n%s", want, v6)
		}
	}
}

func TestBuildDnsHijackIptablesLegacy(t *testing.T) {
	d := sanitizeDnsHijack(DnsHijack{
		Enabled: true,
		Legacy:  true,
		Sink4:   "1.1.1.1",
		Sink6:   "2606:4700:4700::1111",
	})

	v4 := buildDnsHijackIptablesScript("iptables", "eth0", d)
	for _, want := range []string{
		"iptables -t nat -N WHOISTHAT_DNS_POST",
		"iptables -t nat -I POSTROUTING 1 -j WHOISTHAT_DNS_POST",
		"iptables -t nat -A WHOISTHAT_DNS_POST -p udp -d 1.1.1.1 --dport 53 -o $IFACE -j MASQUERADE",
	} {
		if !strings.Contains(v4, want) {
			t.Errorf("legacy iptables script missing %q:\n%s", want, v4)
		}
	}
	for _, unwanted := range []string{"--mark", "daddr", "203.0.113.7"} {
		if strings.Contains(v4, unwanted) {
			t.Errorf("legacy iptables script must not contain %q:\n%s", unwanted, v4)
		}
	}
}

func TestBuildDnsHijackCleanup(t *testing.T) {
	for _, ipt := range []string{"iptables", "ip6tables"} {
		script := buildDnsHijackCleanupIptablesScript(ipt)
		for _, want := range []string{
			ipt + " -t nat -D OUTPUT -j WHOISTHAT_DNS_OUT",
			ipt + " -t nat -D PREROUTING -j WHOISTHAT_DNS_PRE",
			ipt + " -t nat -D POSTROUTING -j WHOISTHAT_DNS_POST",
			ipt + " -t nat -X WHOISTHAT_DNS_OUT",
			ipt + " -t nat -X WHOISTHAT_DNS_PRE",
			ipt + " -t nat -X WHOISTHAT_DNS_POST",
		} {
			if !strings.Contains(script, want) {
				t.Errorf("cleanup script for %s missing %q:\n%s", ipt, want, script)
			}
		}
	}
}

func TestSanitizeDnsHijack(t *testing.T) {
	d := sanitizeDnsHijack(DnsHijack{
		Enabled: true,
		Happ:    true,
		Sink4:   "not-an-ip",
		Sink6:   "2001:db8::53",
		Exclude: []string{"203.0.113.7", "203.0.113.7", "bad", "2001:db8::7"},
	})
	if d.Sink4 != DNSSink4 {
		t.Errorf("invalid Happ sink must fall back to %s, got %q", DNSSink4, d.Sink4)
	}
	if d.Sink6 != "2001:db8::53" {
		t.Errorf("unexpected sink6: %q", d.Sink6)
	}
	if len(d.Exclude) != 2 || d.Exclude[0] != "203.0.113.7" || d.Exclude[1] != "2001:db8::7" {
		t.Errorf("unexpected excludes: %#v", d.Exclude)
	}

	legacy := sanitizeDnsHijack(DnsHijack{Enabled: true, Legacy: true, Sink4: "bad"})
	if legacy.Sink4 != "" {
		t.Errorf("legacy must not fall back to the Happ sink, got %q", legacy.Sink4)
	}
}

func TestDnsHijackEmptySinkProducesNoScript(t *testing.T) {
	d := sanitizeDnsHijack(DnsHijack{Enabled: true, Legacy: true})
	if script := buildDnsHijackNftScript("ip", "eth0", d); script != "" {
		t.Errorf("expected empty nft script, got %q", script)
	}
	if script := buildDnsHijackIptablesScript("iptables", "eth0", d); script != "" {
		t.Errorf("expected empty iptables script, got %q", script)
	}
}

// Applies the generated scripts for real inside a throwaway user+network
// namespace, so nft/iptables validate the syntax without touching the host.
func TestDnsHijackScriptsApplyInNetworkNamespace(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping namespace integration test in -short mode")
	}
	if _, err := exec.LookPath("unshare"); err != nil {
		t.Skip("unshare not available")
	}
	if err := exec.Command("unshare", "-rn", "true").Run(); err != nil {
		t.Skipf("user+net namespaces not available: %v", err)
	}

	happ := sanitizeDnsHijack(DnsHijack{
		Enabled:   true,
		Happ:      true,
		ScopeMark: 1,
		Exclude:   []string{"203.0.113.7", "2001:db8::7"},
	})
	legacy := sanitizeDnsHijack(DnsHijack{
		Enabled: true,
		Legacy:  true,
		Sink4:   "1.1.1.1",
		Sink6:   "2606:4700:4700::1111",
	})

	cases := []struct {
		name string
		bin  string
		body string
	}{
		{
			name: "nftables-happ",
			bin:  "nft",
			body: buildDnsHijackNftScript("ip", "lo", happ) +
				buildDnsHijackNftScript("ip6", "lo", happ),
		},
		{
			name: "nftables-legacy",
			bin:  "nft",
			body: buildDnsHijackNftScript("ip", "lo", legacy) +
				buildDnsHijackNftScript("ip6", "lo", legacy),
		},
		{
			name: "iptables-happ",
			bin:  "iptables",
			body: buildDnsHijackIptablesScript("iptables", "lo", happ) +
				buildDnsHijackIptablesScript("ip6tables", "lo", happ),
		},
		{
			name: "iptables-legacy",
			bin:  "iptables",
			body: buildDnsHijackIptablesScript("iptables", "lo", legacy) +
				buildDnsHijackIptablesScript("ip6tables", "lo", legacy),
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if _, err := exec.LookPath(tc.bin); err != nil {
				t.Skipf("%s not available", tc.bin)
			}
			if tc.bin == "iptables" {
				if _, err := exec.LookPath("ip6tables"); err != nil {
					t.Skip("ip6tables not available")
				}
			}
			cmd := exec.Command("unshare", "-rn", "sh", "-s")
			cmd.Stdin = strings.NewReader(tc.body)
			if out, err := cmd.CombinedOutput(); err != nil {
				t.Fatalf("%s script rejected: %v\n%s\n--- script ---\n%s", tc.name, err, out, tc.body)
			}
		})
	}
}
