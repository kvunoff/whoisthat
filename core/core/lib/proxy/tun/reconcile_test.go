package tunmode

import (
	"strings"
	"testing"
)

// The reconcile script must cover every nft table the core can create, so a
// crashed session never leaves a user with dangling rules.
func TestReconcileScriptCoversAllTables(t *testing.T) {
	script := buildReconcileScript("whoisthattun")

	wantTables := []string{
		"nft delete table ip whoisthat ",     // DNS hijack v4
		"nft delete table ip6 whoisthat ",    // DNS hijack v6
		"nft delete table inet whoisthat_ks", // kill-switch
		"nft delete table ip whoisthat_mangle",
		"nft delete table ip6 whoisthat_mangle",
		"nft delete table inet whoisthat_mangle",
		"nft delete table inet whoisthat_split", // split tunnel
	}
	for _, w := range wantTables {
		if !strings.Contains(script, w) {
			t.Errorf("reconcile script missing table teardown: %q", w)
		}
	}
}

// The kill-switch iptables chain (the documented breakage in the troubleshooting
// table) must be reconciled on the legacy backend too.
func TestReconcileScriptClearsKillSwitchChain(t *testing.T) {
	script := buildReconcileScript("whoisthattun")
	for _, w := range []string{"WHOISTHAT_KS", "-F WHOISTHAT_KS", "-X WHOISTHAT_KS"} {
		if !strings.Contains(script, w) {
			t.Errorf("reconcile script missing kill-switch cleanup: %q", w)
		}
	}
}

// Policy routing for both the exclude (table 100) and include (table 200)
// split-tunnel modes must be flushed.
func TestReconcileScriptFlushesPolicyTables(t *testing.T) {
	script := buildReconcileScript("whoisthattun")
	if !strings.Contains(script, "for TABLE in 100 200; do") {
		t.Fatalf("reconcile script does not iterate over both policy tables: %s", script)
	}
	for _, w := range []string{
		"ip rule del table $TABLE",
		"ip -6 rule del table $TABLE",
		"ip route flush table $TABLE",
		"ip -6 route flush table $TABLE",
	} {
		if !strings.Contains(script, w) {
			t.Errorf("reconcile script missing policy-routing cleanup: %q", w)
		}
	}
}

// A custom TUN name must be used for the device teardown, otherwise a crash
// leaves the user's configured interface behind.
func TestReconcileScriptUsesConfiguredTunName(t *testing.T) {
	script := buildReconcileScript("mytun0")
	if !strings.Contains(script, `ip tuntap del mode tun dev "mytun0"`) {
		t.Errorf("reconcile script does not target configured tun name: %s", script)
	}
	if strings.Contains(script, `dev "whoisthattun"`) {
		t.Error("reconcile script still hardcodes whoisthattun")
	}
}

// Every command must be individually guarded so the script is a clean no-op
// when nothing exists. No bare nft/iptables delete may run without a `|| true`,
// a `|| break`, or being inside a guarded loop.
func TestReconcileScriptIsIdempotent(t *testing.T) {
	script := buildReconcileScript("whoisthattun")
	for _, line := range strings.Split(script, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" || strings.HasPrefix(trimmed, "#") {
			continue
		}
		guarded := strings.Contains(trimmed, "|| true") ||
			strings.Contains(trimmed, "|| break") ||
			// loop/control constructs and their bodies
			strings.HasPrefix(trimmed, "for ") ||
			strings.HasPrefix(trimmed, "while ") ||
			strings.HasPrefix(trimmed, "do") ||
			strings.HasPrefix(trimmed, "done") ||
			strings.HasPrefix(trimmed, "i=") ||
			trimmed == "done"
		if !guarded {
			t.Errorf("unguarded command in reconcile script (not idempotent): %q", trimmed)
		}
	}
}

// The iptables DNS hijack chains must be reconciled as well; otherwise a crash
// on the legacy backend leaves port 53 DNAT'd to a sink with no core running.
func TestReconcileScriptClearsDnsHijackChains(t *testing.T) {
	script := buildReconcileScript("whoisthattun")
	for _, w := range []string{
		"-t nat -D OUTPUT -j WHOISTHAT_DNS_OUT",
		"-t nat -D PREROUTING -j WHOISTHAT_DNS_PRE",
		"-t nat -D POSTROUTING -j WHOISTHAT_DNS_POST",
		"-t nat -F WHOISTHAT_DNS_OUT",
		"-t nat -F WHOISTHAT_DNS_PRE",
		"-t nat -F WHOISTHAT_DNS_POST",
		"-t nat -X WHOISTHAT_DNS_OUT",
		"-t nat -X WHOISTHAT_DNS_PRE",
		"-t nat -X WHOISTHAT_DNS_POST",
	} {
		if !strings.Contains(script, w) {
			t.Errorf("reconcile script missing DNS hijack cleanup: %q", w)
		}
	}
}
