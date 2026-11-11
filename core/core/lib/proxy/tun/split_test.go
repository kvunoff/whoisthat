package tunmode

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// The cgroup paths and their nftables "level" must stay in sync: level is the depth
// of the slice dir below the cgroup root, so it equals the number of path
// components. If the layout changes, level must track it automatically.
func TestSplitCgroupLevelMatchesPathDepth(t *testing.T) {
	paths := splitCgroupRelPaths(1000)
	if len(paths) != 3 {
		t.Fatalf("expected 3 split cgroup candidate paths, got %d", len(paths))
	}

	// 1. whoisthat_split.slice (level 4, no hyphens, top-level user slice)
	relUnderscore := paths[0]
	wantComponentsUnderscore := strings.Count(relUnderscore, "/") + 1
	if got := splitCgroupLevel(relUnderscore); got != wantComponentsUnderscore || got != 4 {
		t.Errorf("splitCgroupLevel(%q) = %d, want %d (level 4)", relUnderscore, got, wantComponentsUnderscore)
	}

	// 2. whoisthat.slice/whoisthat-split.slice (level 5, subslice)
	relHyphen := paths[1]
	wantComponentsHyphen := strings.Count(relHyphen, "/") + 1
	if got := splitCgroupLevel(relHyphen); got != wantComponentsHyphen || got != 5 {
		t.Errorf("splitCgroupLevel(%q) = %d, want %d (level 5)", relHyphen, got, wantComponentsHyphen)
	}

	// 3. whoisthat-<uid> (level 1, native backend without systemd)
	relNative := paths[2]
	if relNative != "whoisthat-1000" {
		t.Errorf("native cgroup path = %q, want whoisthat-1000", relNative)
	}
	if got := splitCgroupLevel(relNative); got != 1 {
		t.Errorf("splitCgroupLevel(%q) = %d, want 1", relNative, got)
	}
}

// Exclude mode marks split-app sockets with mark 1 and must make sure the
// physical table-100 route for that mark exists: TUN start installs it too, but
// a reconcile/network reset can drop it while the TUN stays up.
func TestSplitExcludeModeReusesTable100(t *testing.T) {
	script := buildSplitSetupScript("exclude", 1000, "whoisthattun", "wlan0", "192.168.3.1", true, true)

	if !strings.Contains(script, "nft add table inet whoisthat_split") {
		t.Error("exclude script must create the whoisthat_split nft table")
	}
	// The marking rule must target the cgroup and set the exclude mark.
	if !strings.Contains(script, "socket cgroupv2 level") {
		t.Error("exclude script must match sockets by cgroupv2 membership")
	}
	if !strings.Contains(script, "meta mark set 1") {
		t.Errorf("exclude script must set mark %d, script: %s", splitExcludeMark, script)
	}
	if !strings.Contains(script, "nft 'add rule inet whoisthat_split output socket cgroupv2 level 4 \"user.slice/user-1000.slice/user@1000.service/whoisthat_split.slice\" meta mark set 1'") {
		t.Errorf("exclude script missing level 4 rule: %s", script)
	}
	if !strings.Contains(script, "nft 'add rule inet whoisthat_split output socket cgroupv2 level 5 \"user.slice/user-1000.slice/user@1000.service/whoisthat.slice/whoisthat-split.slice\" meta mark set 1'") {
		t.Errorf("exclude script missing level 5 rule: %s", script)
	}
	if !strings.Contains(script, "nft 'add rule inet whoisthat_split postrouting meta mark 1 oifname != \"whoisthattun\" oifname != \"lo\" masquerade'") {
		t.Errorf("exclude script missing masquerade rule: %s", script)
	}
	// Exclude mode must not touch the include-mode table 200.
	if strings.Contains(script, "table 200") {
		t.Error("exclude script must not reference the include-mode table 200")
	}
	// The mark-1 policy route must be (re)installed idempotently.
	if !strings.Contains(script, "ip rule add fwmark 1 table 100") {
		t.Error("exclude script must re-assert the mark-1 fwmark rule")
	}
	if !strings.Contains(script, "ip route replace default via 192.168.3.1 table 100") {
		t.Errorf("exclude script must re-assert the table-100 default route: %s", script)
	}
}

// Without a parsed physical gateway there is no valid table-100 route to
// install; the script must skip it instead of emitting broken commands.
func TestSplitExcludeModeSkipsRouteWithoutPhysicalGateway(t *testing.T) {
	script := buildSplitSetupScript("exclude", 1000, "whoisthattun", "", "", true, false)
	if strings.Contains(script, "ip rule add fwmark 1 table 100") {
		t.Errorf("exclude script must not route mark 1 without a physical gateway: %s", script)
	}
	if strings.Contains(script, "ip route replace default via  table 100") {
		t.Errorf("exclude script emitted an empty gateway: %s", script)
	}
}

// When the physical link lacks IPv6, exclude mode must blackhole mark 1 IPv6 so
// dual-stack apps fall back to physical IPv4 instead of leaking into the TUN.
func TestSplitExcludeModeFallbackV6WhenPhysicalAbsent(t *testing.T) {
	script := buildSplitSetupScript("exclude", 1000, "whoisthattun", "wlan0", "192.168.3.1", true, false)
	if !strings.Contains(script, "ip -6 rule add fwmark 1 table 100") {
		t.Errorf("exclude script must add fwmark 1 v6 rule when physV6=false, script: %s", script)
	}
	if !strings.Contains(script, "ip -6 route replace unreachable default table 100") {
		t.Errorf("exclude script must add unreachable v6 route when physV6=false, script: %s", script)
	}
}

// With a physical IPv6 gateway the fallback must not exist: table 100 already
// carries Xray's direct route there.
func TestSplitExcludeModeSkipsFallbackWhenPhysicalV6Present(t *testing.T) {
	script := buildSplitSetupScript("exclude", 1000, "whoisthattun", "wlan0", "192.168.3.1", true, true)
	if strings.Contains(script, "unreachable") {
		t.Errorf("exclude script must not blackhole v6 when physV6=true, script: %s", script)
	}
}

// Include mode is the inverted model: only split apps use the tunnel. It marks
// with mark 2 and installs a dedicated table whose default route is the TUN dev.
func TestSplitIncludeModeInstallsTunTable(t *testing.T) {
	script := buildSplitSetupScript("include", 1000, "whoisthattun", "wlan0", "192.168.3.1", true, true)

	if !strings.Contains(script, "meta mark set 2") {
		t.Errorf("include script must set mark %d", splitIncludeMark)
	}
	if !strings.Contains(script, "nft 'add rule inet whoisthat_split postrouting meta mark 2 oifname \"whoisthattun\" masquerade'") {
		t.Errorf("include script missing include masquerade rule: %s", script)
	}
	if !strings.Contains(script, "ip rule add fwmark 2 table 200") {
		t.Error("include script must route mark 2 via table 200")
	}
	if !strings.Contains(script, "ip route replace default dev whoisthattun table 200") {
		t.Errorf("include script must point table 200 default at the tun dev, script: %s", script)
	}
	// IPv6 branch must be present when the TUN carries v6.
	if !strings.Contains(script, "ip -6 rule add fwmark 2 table 200") {
		t.Error("include script must install the IPv6 fwmark rule when the tun has v6")
	}
}

// Without an IPv6 TUN address, include mode must not emit v6 routing commands
// (they would fail on a v4-only tunnel/host).
func TestSplitIncludeModeSkipsV6WhenTunAbsent(t *testing.T) {
	script := buildSplitSetupScript("include", 1000, "whoisthattun", "wlan0", "192.168.3.1", false, false)
	if strings.Contains(script, "ip -6 ") {
		t.Errorf("include script must omit IPv6 commands when tunV6=false, script: %s", script)
	}
}

// Unknown / off modes must produce no script at all (no rules installed).
func TestSplitOffModeProducesNoScript(t *testing.T) {
	for _, mode := range []string{"off", "", "bogus"} {
		if s := buildSplitSetupScript(mode, 1000, "whoisthattun", "wlan0", "192.168.3.1", true, true); s != "" {
			t.Errorf("mode %q must yield an empty script, got: %s", mode, s)
		}
	}
}

// Teardown must remove everything setup can create and be a clean no-op when
// nothing exists (every command guarded).
func TestSplitTeardownIsIdempotentAndComplete(t *testing.T) {
	script := buildSplitTeardownScript(false)

	if !strings.Contains(script, "nft delete table inet whoisthat_split") {
		t.Error("teardown must drop the whoisthat_split nft table")
	}
	if !strings.Contains(script, "ip rule del fwmark 2 table 200") {
		t.Error("teardown must remove the include-mode fwmark rule")
	}
	if !strings.Contains(script, "ip route flush table 200") {
		t.Error("teardown must flush the include-mode routing table")
	}
	if !strings.Contains(script, "ip -6 route flush table 100") {
		t.Error("teardown must clear the v4-only v6 fallback from table 100")
	}
	for _, line := range strings.Split(script, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" {
			continue
		}
		if !strings.Contains(trimmed, "|| true") {
			t.Errorf("unguarded command in split teardown (not idempotent): %q", trimmed)
		}
	}
}

// With a physical IPv6 gateway, table 100 belongs to Xray's fwmark route and
// must survive a split reapply.
func TestSplitTeardownKeepsXrayV6Route(t *testing.T) {
	script := buildSplitTeardownScript(true)
	if strings.Contains(script, "table 100") {
		t.Errorf("teardown must not touch table 100 when physV6=true, script: %s", script)
	}
}

// A cgroup v1 host or a missing root-created cgroup must stop the script with
// an actionable message instead of silently installing no rules.
func TestSplitScriptFailsWithActionableMessage(t *testing.T) {
	script := buildSplitSetupScript("exclude", 1000, "whoisthattun", "wlan0", "192.168.3.1", true, true)
	for _, want := range []string{
		"cgroup.controllers",
		"split cgroup not available; run: sudo /usr/lib/whoisthat/cgroup-setup $USER",
		`[ -d "$root/whoisthat-1000" ] && nft`,
	} {
		if !strings.Contains(script, want) {
			t.Errorf("split script missing %q: %s", want, script)
		}
	}
}

// The native cgroup override is test-only: an environment variable must not
// redirect normal operation outside the real cgroup tree.
func TestCgroupRootOverrideOnlyInTestMode(t *testing.T) {
	t.Setenv("WHOISTHAT_TEST_MODE", "")
	t.Setenv("WHOISTHAT_CGROUP_ROOT", "/tmp/whoisthat-evil")
	if got := cgroupRoot(); got != "/sys/fs/cgroup" {
		t.Errorf("cgroupRoot() = %q without test mode, want /sys/fs/cgroup", got)
	}

	t.Setenv("WHOISTHAT_TEST_MODE", "1")
	if got := cgroupRoot(); got != "/tmp/whoisthat-evil" {
		t.Errorf("cgroupRoot() = %q in test mode, want the override", got)
	}
}

func TestValidateSplitUid(t *testing.T) {
	for _, mode := range []string{"include", "exclude"} {
		if err := ValidateSplitUid(mode, 0); err == nil {
			t.Errorf("mode %q with uid 0 must fail", mode)
		}
		if err := ValidateSplitUid(mode, 1000); err != nil {
			t.Errorf("mode %q with uid 1000 must pass: %v", mode, err)
		}
	}
	for _, mode := range []string{"", "off"} {
		if err := ValidateSplitUid(mode, 0); err != nil {
			t.Errorf("mode %q must be a no-op: %v", mode, err)
		}
	}
}

func TestReapplySplitNoopWhenTunDown(t *testing.T) {
	manager := &TunModeManager{}
	if err := manager.ReapplySplit("off"); err != nil {
		t.Errorf("ReapplySplit on a stopped TUN must be a no-op: %v", err)
	}
}

func writeStub(t *testing.T, dir, name, logPath string) {
	t.Helper()
	body := "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"" + logPath + "\"\nexit 0\n"
	if err := os.WriteFile(filepath.Join(dir, name), []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
}

// The script must only reference cgroups that exist: nft resolves the path at
// rule-add time and aborts the whole script otherwise.
func TestSplitScriptRunsAgainstFakeCgroupRoot(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "cgroup.controllers"), nil, 0o644); err != nil {
		t.Fatal(err)
	}
	bin := t.TempDir()
	logPath := filepath.Join(t.TempDir(), "commands.log")
	writeStub(t, bin, "nft", logPath)
	writeStub(t, bin, "ip", logPath)

	t.Setenv("WHOISTHAT_TEST_MODE", "1")
	t.Setenv("WHOISTHAT_CGROUP_ROOT", root)
	t.Setenv("PATH", bin+string(os.PathListSeparator)+os.Getenv("PATH"))

	script := buildSplitSetupScript("exclude", 1000, "whoisthattun", "wlan0", "192.168.3.1", true, true)
	if out, err := exec.Command("sh", "-c", script).CombinedOutput(); err != nil {
		t.Fatalf("script failed: %v\n%s", err, out)
	}

	logged, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatal(err)
	}
	rules := 0
	for _, line := range strings.Split(string(logged), "\n") {
		if strings.HasPrefix(line, "add rule inet whoisthat_split output") {
			rules++
		}
	}
	if rules != 3 {
		t.Errorf("expected one cgroup rule per path (3), got %d:\n%s", rules, logged)
	}
}

func TestSplitScriptFailsWithoutCgroupDirs(t *testing.T) {
	if os.Geteuid() == 0 {
		t.Skip("root ignores the read-only temp dir this test relies on")
	}
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "cgroup.controllers"), nil, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(root, 0o500); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chmod(root, 0o700) })

	t.Setenv("WHOISTHAT_TEST_MODE", "1")
	t.Setenv("WHOISTHAT_CGROUP_ROOT", root)

	script := buildSplitSetupScript("exclude", 1000, "whoisthattun", "wlan0", "192.168.3.1", true, true)
	out, err := exec.Command("sh", "-c", script).CombinedOutput()
	if err == nil {
		t.Fatalf("script must fail when no cgroup dir can be created: %s", out)
	}
	if !strings.Contains(string(out), "split cgroup not available") {
		t.Errorf("unexpected error output: %s", out)
	}
}
