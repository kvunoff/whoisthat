package tunmode

import (
	"fmt"
	"os"
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
		"add table inet whoisthat_ks",
		"policy drop",
		"nft -f - <<'WHOISTHAT_KS_EOF'",
		"flush chain inet whoisthat_ks output",
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

	if !dropIsLastBatchRule(script) {
		t.Errorf("nft script must end the batch with the drop rule, got:\n%s", script)
	}
}

// The allowlist must be applied in one transaction: a partial batch would
// otherwise leave the chain flushes but incomplete.
func TestKillSwitchNftScriptIsAtomic(t *testing.T) {
	script := buildKillSwitchNftScript(
		[]string{"203.0.113.7"},
		"whoisthattun",
		[]string{"1.1.1.1"},
	)
	if got := strings.Count(script, "nft -f -"); got != 1 {
		t.Fatalf("expected exactly one nft -f batch, got %d:\n%s", got, script)
	}
	if got := strings.Count(script, "flush chain inet whoisthat_ks output"); got != 1 {
		t.Fatalf("expected exactly one flush, got %d:\n%s", got, script)
	}
	if strings.Contains(script, "nft add rule") {
		t.Fatalf("allowlist rules must be inside the batch, got:\n%s", script)
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
		"iptables -I OUTPUT 1 -j WHOISTHAT_KS",
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

	// Duplicate jumps from an older session must be removed before recreating
	// the chain, otherwise -X fails and -N aborts under set -e.
	for _, want := range []string{
		`while "$IPT" -C OUTPUT -j WHOISTHAT_KS`,
		"iptables -N WHOISTHAT_KS 2>/dev/null || iptables -F WHOISTHAT_KS",
	} {
		if !strings.Contains(script, want) {
			t.Errorf("iptables script missing %q:\n%s", want, script)
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
	if !dropIsLastBatchRule(script) {
		t.Errorf("nft script must still end the batch with the drop rule:\n%s", script)
	}
}

// fakeProbes swaps the inspection seams so exit-code handling can be tested
// without real capabilities or a live firewall.
func fakeProbes(t *testing.T, available map[string]bool, run func(string, ...string) (string, error)) {
	t.Helper()
	oldLook, oldRun, oldScript := killSwitchLookPath, killSwitchRun, killSwitchRunScript
	killSwitchLookPath = func(name string) (string, error) {
		if available[name] {
			return "/usr/bin/" + name, nil
		}
		return "", &exec.Error{Name: name, Err: exec.ErrNotFound}
	}
	killSwitchRun = run
	killSwitchRunScript = func(string) (string, error) { return "", nil }
	t.Cleanup(func() {
		killSwitchLookPath, killSwitchRun, killSwitchRunScript = oldLook, oldRun, oldScript
	})
}

func exitCodeErr(code int) error {
	return exec.Command("sh", "-c", fmt.Sprintf("exit %d", code)).Run()
}

func TestKillSwitchArmedSkipsAbsentBackends(t *testing.T) {
	fakeProbes(t, nil, func(string, ...string) (string, error) {
		t.Fatal("must not run a missing binary")
		return "", nil
	})
	armed, err := KillSwitchArmed()
	if armed || err != nil {
		t.Fatalf("KillSwitchArmed() = %v, %v; want false, nil", armed, err)
	}
}

func TestKillSwitchArmedDetectsNftChain(t *testing.T) {
	correct := readKSFixture(t, "nft_ks_correct.json")
	fakeProbes(t, map[string]bool{"nft": true}, func(string, ...string) (string, error) {
		return correct, nil
	})
	armed, err := KillSwitchArmed()
	if !armed || err != nil {
		t.Fatalf("KillSwitchArmed() = %v, %v; want true, nil", armed, err)
	}
}

func TestKillSwitchArmedDistinguishesMissingTableFromFailure(t *testing.T) {
	cases := []struct {
		name    string
		out     string
		err     error
		want    bool
		wantErr bool
	}{
		{"missing table", "Error: Could not process rule: No such file or directory", exitCodeErr(1), false, false},
		{"permission denied", "Error: Could not process rule: Operation not permitted", exitCodeErr(1), false, true},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			fakeProbes(t, map[string]bool{"nft": true}, func(string, ...string) (string, error) {
				return tc.out, tc.err
			})
			armed, err := KillSwitchArmed()
			if armed != tc.want || (err != nil) != tc.wantErr {
				t.Fatalf("KillSwitchArmed() = %v, %v; want %v, err=%v", armed, err, tc.want, tc.wantErr)
			}
		})
	}
}

func TestKillSwitchArmedIptablesExitCodes(t *testing.T) {
	cases := []struct {
		code    int
		want    bool
		wantErr bool
	}{
		{0, true, false},  // rule present
		{1, false, false}, // rule missing
		{2, false, false}, // chain missing
		{4, false, true},  // permission denied: must not look disarmed
	}
	for _, tc := range cases {
		t.Run(fmt.Sprintf("exit_%d", tc.code), func(t *testing.T) {
			fakeProbes(t, map[string]bool{"iptables": true, "ip6tables": true}, func(name string, args ...string) (string, error) {
				if name == "nft" {
					t.Fatalf("nft must be skipped when absent")
				}
				return "", exitCodeErr(tc.code)
			})
			armed, err := KillSwitchArmed()
			if armed != tc.want || (err != nil) != tc.wantErr {
				t.Fatalf("KillSwitchArmed() = %v, %v; want %v, err=%v", armed, err, tc.want, tc.wantErr)
			}
		})
	}
}

func TestKillSwitchArmedDetectsLeftoverIPv6Chain(t *testing.T) {
	fakeProbes(t, map[string]bool{"iptables": true, "ip6tables": true}, func(name string, args ...string) (string, error) {
		if name == "ip6tables" {
			return "", nil
		}
		return "", exitCodeErr(1)
	})
	armed, err := KillSwitchArmed()
	if !armed || err != nil {
		t.Fatalf("KillSwitchArmed() = %v, %v; want true, nil", armed, err)
	}
}

func TestRemoveKillSwitchBlockRefusesWhileArmed(t *testing.T) {
	fakeProbes(t, map[string]bool{"iptables": true, "ip6tables": true}, func(string, ...string) (string, error) {
		return "", nil // exit 0: still armed
	})
	if err := RemoveKillSwitchBlock(); err == nil {
		t.Fatal("RemoveKillSwitchBlock() = nil; want an error while rules remain")
	}
}

func TestRemoveKillSwitchBlockReportsProbeFailure(t *testing.T) {
	fakeProbes(t, map[string]bool{"iptables": true, "ip6tables": true}, func(string, ...string) (string, error) {
		return "", exitCodeErr(4) // permission denied
	})
	if err := RemoveKillSwitchBlock(); err == nil {
		t.Fatal("RemoveKillSwitchBlock() = nil; want an error when the state cannot be verified")
	}
}

func TestRemoveKillSwitchBlockSucceedsWhenGone(t *testing.T) {
	fakeProbes(t, map[string]bool{"iptables": true, "ip6tables": true}, func(string, ...string) (string, error) {
		return "", exitCodeErr(1)
	})
	if err := RemoveKillSwitchBlock(); err != nil {
		t.Fatalf("RemoveKillSwitchBlock() = %v; want nil", err)
	}
}

func TestKillSwitchIptablesTeardownRemovesDuplicateJumps(t *testing.T) {
	script := buildKillSwitchIptablesTeardownScript()
	for _, want := range []string{
		"for IPT in iptables ip6tables",
		`while "$IPT" -C OUTPUT -j WHOISTHAT_KS`,
		`"$IPT" -D OUTPUT -j WHOISTHAT_KS`,
		`"$IPT" -F WHOISTHAT_KS`,
		`"$IPT" -X WHOISTHAT_KS`,
	} {
		if !strings.Contains(script, want) {
			t.Errorf("teardown script missing %q:\n%s", want, script)
		}
	}
}

// K-1: a DHCP client broadcasts DISCOVER/REQUEST to 255.255.255.255:67 before
// it has any address, so none of the LAN ranges above match. With the switch
// armed a lease renewal would be dropped and the host strands itself offline.
func TestKillSwitchAllowsDHCPBroadcast(t *testing.T) {
	nft := buildKillSwitchNftScript(nil, "", nil)
	if !strings.Contains(nft, "ip daddr 255.255.255.255 udp dport 67 accept") {
		t.Errorf("nft script must accept udp 255.255.255.255:67 before the drop:\n%s", nft)
	}
	ipt := buildKillSwitchIptablesScript(nil, "", nil)
	if !strings.Contains(ipt, "iptables -A WHOISTHAT_KS -d 255.255.255.255 -p udp --dport 67 -j ACCEPT") {
		t.Errorf("iptables script must accept udp 255.255.255.255:67 before the drop:\n%s", ipt)
	}
	// The allow must not be v6 and must sit before the final drop.
	if strings.Index(nft, "255.255.255.255") > strings.LastIndex(nft, "output drop") {
		t.Errorf("DHCP allow must precede the drop rule")
	}
}

// Applies the nft ruleset in a throwaway netns and checks the kernel kept the
// DHCP rule ahead of the drop, i.e. the packet would traverse before dropping.
func TestKillSwitchDHCPRuleOrderedBeforeDropInNetns(t *testing.T) {
	if _, err := exec.LookPath("unshare"); err != nil {
		t.Skip("unshare not available")
	}
	if _, err := exec.LookPath("nft"); err != nil {
		t.Skip("nft not available")
	}
	if err := exec.Command("unshare", "-rn", "true").Run(); err != nil {
		t.Skipf("user+net namespaces not available: %v", err)
	}
	body := buildKillSwitchNftScript(nil, "", nil) + "nft list chain inet whoisthat_ks output\n"
	cmd := exec.Command("unshare", "-rn", "sh", "-s")
	cmd.Stdin = strings.NewReader(body)
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("apply failed: %v\n%s", err, out)
	}
	s := string(out)
	dhcp := strings.Index(s, "255.255.255.255")
	drop := strings.LastIndex(s, "drop")
	if dhcp < 0 || !strings.Contains(s, "udp dport 67 accept") {
		t.Fatalf("live ruleset has no DHCP accept:\n%s", s)
	}
	if dhcp > drop {
		t.Fatalf("DHCP accept is after the drop:\n%s", s)
	}
}

func readKSFixture(t *testing.T, name string) string {
	t.Helper()
	b, err := os.ReadFile("testdata/" + name)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

// K-3: "armed" must mean the live ruleset really blocks, not that a chain of
// that name exists. Fixtures are real `nft -j list table inet whoisthat_ks`
// output (nft 1.0.9) captured in a throwaway netns.
func TestKillSwitchArmedVerifiesLiveRuleset(t *testing.T) {
	correct := readKSFixture(t, "nft_ks_correct.json")
	cases := []struct {
		name string
		out  string
		want bool
	}{
		{"empty chain (policy drop, no rules)", readKSFixture(t, "nft_ks_empty_chain.json"), false},
		{"v4-only half install", readKSFixture(t, "nft_ks_v4_only.json"), false},
		{"policy accept with full rules", strings.Replace(correct, `"policy": "drop"`, `"policy": "accept"`, 1), false},
		{"correct ruleset", correct, true},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			fakeProbes(t, map[string]bool{"nft": true}, func(string, ...string) (string, error) {
				return tc.out, nil
			})
			armed, err := KillSwitchArmed()
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if armed != tc.want {
				t.Fatalf("KillSwitchArmed() = %v, want %v", armed, tc.want)
			}
		})
	}
}

func TestKillSwitchArmedRejectsUnparseableNftOutput(t *testing.T) {
	fakeProbes(t, map[string]bool{"nft": true}, func(string, ...string) (string, error) {
		return "not json", nil
	})
	armed, err := KillSwitchArmed()
	if armed || err == nil {
		t.Fatalf("KillSwitchArmed() = %v, %v; want false + error", armed, err)
	}
}

// A half-built table still drops traffic, so removal must not be declared
// complete while it is there even though it is not "armed".
func TestRemoveKillSwitchBlockRefusesWhileHalfInstalled(t *testing.T) {
	half := readKSFixture(t, "nft_ks_empty_chain.json")
	fakeProbes(t, map[string]bool{"nft": true}, func(string, ...string) (string, error) {
		return half, nil
	})
	if err := RemoveKillSwitchBlock(); err == nil {
		t.Fatal("RemoveKillSwitchBlock must fail while a drop-policy chain remains")
	}
}

// dropIsLastBatchRule reports whether the final line before the heredoc
// terminator is the catch-all drop.
func dropIsLastBatchRule(script string) bool {
	return strings.Contains(script, "add rule inet whoisthat_ks output drop\nWHOISTHAT_KS_EOF\n")
}
