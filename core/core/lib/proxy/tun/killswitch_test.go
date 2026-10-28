package tunmode

import (
	"os/exec"
	"strings"
	"testing"
)

func TestKillSwitchNftWhitelistsEverythingDocumented(t *testing.T) {
	script := buildKillSwitchNftScript(
		[]string{"203.0.113.7", "2001:db8::7"},
		"whoisthattun",
		[]string{"1.1.1.1", "2606:4700:4700::1111"},
	)

	mustContain := []string{
		"nft delete table inet whoisthat_ks",
		"oif lo accept",
		"ip daddr 127.0.0.0/8 accept",
		"ip daddr 10.0.0.0/8 accept",
		"ip daddr 172.16.0.0/12 accept",
		"ip daddr 192.168.0.0/16 accept",
		"ip6 daddr fc00::/7 accept",
		"ip6 daddr fe80::/10 accept",
		// DNS resolution must survive the kill-switch.
		"ip daddr 1.1.1.1 accept",
		"ip6 daddr 2606:4700:4700::1111 accept",
		// Reserved TUN DNS sink.
		"ip daddr " + killSwitchDNSSink4 + " accept",
		"ip6 daddr " + killSwitchDNSSink6 + " accept",
		// Direct/split-exclude sockets.
		"meta mark 1 accept",
		// The tunnel itself.
		`oifname "whoisthattun" accept`,
		// VPN server.
		"ip daddr 203.0.113.7 accept",
		"ip6 daddr 2001:db8::7 accept",
	}
	for _, want := range mustContain {
		if !strings.Contains(script, want) {
			t.Errorf("nft script missing %q\n---\n%s", want, script)
		}
	}

	if !strings.HasSuffix(strings.TrimSpace(script), "nft add rule inet whoisthat_ks output drop") {
		t.Errorf("nft script must end with the drop rule, got:\n%s", script)
	}
}

func TestKillSwitchIptablesWhitelistsEverythingDocumented(t *testing.T) {
	script := buildKillSwitchIptablesScript(
		[]string{"203.0.113.7", "2001:db8::7"},
		"whoisthattun",
		[]string{"1.1.1.1", "2606:4700:4700::1111"},
	)

	mustContain := []string{
		"iptables -N WHOISTHAT_KS",
		"iptables -A WHOISTHAT_KS -o lo -j ACCEPT",
		"iptables -A WHOISTHAT_KS -d 10.0.0.0/8 -j ACCEPT",
		"iptables -A WHOISTHAT_KS -d 192.168.0.0/16 -j ACCEPT",
		"iptables -A WHOISTHAT_KS -d 1.1.1.1 -j ACCEPT",
		"iptables -A WHOISTHAT_KS -d " + killSwitchDNSSink4 + " -j ACCEPT",
		"iptables -A WHOISTHAT_KS -m mark --mark 1 -j ACCEPT",
		"iptables -A WHOISTHAT_KS -o whoisthattun -j ACCEPT",
		"iptables -A WHOISTHAT_KS -d 203.0.113.7 -j ACCEPT",
		"ip6tables -A WHOISTHAT_KS -d fc00::/7 -j ACCEPT",
		"ip6tables -A WHOISTHAT_KS -d 2606:4700:4700::1111 -j ACCEPT",
		"ip6tables -A WHOISTHAT_KS -d " + killSwitchDNSSink6 + " -j ACCEPT",
		"ip6tables -A WHOISTHAT_KS -d 2001:db8::7 -j ACCEPT",
	}
	for _, want := range mustContain {
		if !strings.Contains(script, want) {
			t.Errorf("iptables script missing %q\n---\n%s", want, script)
		}
	}

	// The drop must be the last rule in each chain.
	for _, cmd := range []string{"iptables", "ip6tables"} {
		dropped := false
		for _, line := range strings.Split(script, "\n") {
			if line == cmd+" -A WHOISTHAT_KS -j DROP" {
				dropped = true
				continue
			}
			if dropped && strings.HasPrefix(line, cmd+" -A WHOISTHAT_KS ") {
				t.Errorf("rule appended after the %s drop: %q", cmd, line)
			}
		}
		if !dropped {
			t.Errorf("missing %s drop rule", cmd)
		}
	}
}

// Applies the generated script for real inside a throwaway user+network
// namespace, so nft/iptables validate the syntax without touching the host
// firewall. Skips when namespaces are unavailable (containers, restricted CI).
func TestKillSwitchScriptsApplyInNetworkNamespace(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping namespace integration test in -short mode")
	}
	if _, err := exec.LookPath("unshare"); err != nil {
		t.Skip("unshare not available")
	}
	if err := exec.Command("unshare", "-rn", "true").Run(); err != nil {
		t.Skipf("user+net namespaces not available: %v", err)
	}

	cases := []struct {
		name string
		bin  string
		argv []string
		body string
	}{
		{
			name: "nftables",
			bin:  "nft",
			argv: []string{"unshare", "-rn", "sh", "-s"},
			body: buildKillSwitchNftScript(
				[]string{"203.0.113.7", "2001:db8::7"},
				"whoisthattun",
				[]string{"1.1.1.1", "2606:4700:4700::1111"},
			),
		},
		{
			name: "iptables",
			bin:  "iptables",
			argv: []string{"unshare", "-rn", "sh", "-s"},
			body: buildKillSwitchIptablesScript(
				[]string{"203.0.113.7", "2001:db8::7"},
				"whoisthattun",
				[]string{"1.1.1.1", "2606:4700:4700::1111"},
			),
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if _, err := exec.LookPath(tc.bin); err != nil {
				t.Skipf("%s not available", tc.bin)
			}
			cmd := exec.Command(tc.argv[0], tc.argv[1:]...)
			cmd.Stdin = strings.NewReader(tc.body)
			if out, err := cmd.CombinedOutput(); err != nil {
				t.Fatalf("%s script rejected: %v\n%s\n--- script ---\n%s", tc.name, err, out, tc.body)
			}
		})
	}
}

func TestKillSwitchOmitsTunRuleWhenDisabled(t *testing.T) {
	for name, script := range map[string]string{
		"nft":      buildKillSwitchNftScript(nil, "", nil),
		"iptables": buildKillSwitchIptablesScript(nil, "", nil),
	} {
		if strings.Contains(script, "oifname") || strings.Contains(script, "-o whoisthat") {
			t.Errorf("%s: tun rule must be absent without a tun name:\n%s", name, script)
		}
	}
}

func TestKillSwitchRejectsShellInjection(t *testing.T) {
	// Invalid IPs and names must never reach the generated script verbatim.
	script := buildKillSwitchNftScript(
		[]string{"1.1.1.1\nnft flush ruleset", "; rm -rf /", "not-an-ip"},
		"bad name; reboot",
		[]string{"$(curl evil)", "8.8.8.8"},
	)
	for _, bad := range []string{"flush ruleset", "rm -rf", "not-an-ip", "reboot", "curl evil"} {
		if strings.Contains(script, bad) {
			t.Errorf("script contains rejected value %q:\n%s", bad, script)
		}
	}
	// Valid entries still make it through.
	if !strings.Contains(script, "ip daddr 8.8.8.8 accept") {
		t.Errorf("valid DNS IP was dropped:\n%s", script)
	}
	// The TUN name is rejected as a whole, so no tun rule is emitted.
	if strings.Contains(script, "oifname") {
		t.Errorf("invalid tun name leaked into the script:\n%s", script)
	}
}

func TestKillSwitchDeduplicatesIPs(t *testing.T) {
	script := buildKillSwitchNftScript([]string{"203.0.113.7", " 203.0.113.7 "}, "", nil)
	if got := strings.Count(script, "ip daddr 203.0.113.7 accept"); got != 1 {
		t.Errorf("expected the proxy IP once, got %d:\n%s", got, script)
	}
}

// Fail closed with no server IPs: only LAN/DNS/TUN/direct-mark stay allowed.
func TestKillSwitchNftWithoutProxyIPsStillFailsClosed(t *testing.T) {
	script := buildKillSwitchNftScript(nil, "whoisthattun", []string{"1.1.1.1"})

	for _, want := range []string{
		"oif lo accept",
		"ip daddr 10.0.0.0/8 accept",
		"ip daddr 1.1.1.1 accept",
		`oifname "whoisthattun" accept`,
		"meta mark 1 accept",
	} {
		if !strings.Contains(script, want) {
			t.Errorf("nft script missing %q\n---\n%s", want, script)
		}
	}
	if !strings.HasSuffix(strings.TrimSpace(script), "nft add rule inet whoisthat_ks output drop") {
		t.Errorf("nft script must still end with the drop rule:\n%s", script)
	}
}
