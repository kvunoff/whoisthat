package tunmode

import (
	"strings"
	"testing"
)

// The reconcile script must cover every nft table the core can create, so a
// crashed session never leaves a user with dangling rules.
func TestReconcileScriptCoversAllTables(t *testing.T) {
	script := buildReconcileScript()

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
	script := buildReconcileScript()
	for _, w := range []string{"WHOISTHAT_KS", "-F WHOISTHAT_KS", "-X WHOISTHAT_KS"} {
		if !strings.Contains(script, w) {
			t.Errorf("reconcile script missing kill-switch cleanup: %q", w)
		}
	}
}

// Policy routing (table 100 + its fwmark/uidrange rules) must be flushed.
func TestReconcileScriptFlushesTable100(t *testing.T) {
	script := buildReconcileScript()
	for _, w := range []string{
		"ip rule del table 100",
		"ip -6 rule del table 100",
		"ip route flush table 100",
		"ip -6 route flush table 100",
	} {
		if !strings.Contains(script, w) {
			t.Errorf("reconcile script missing policy-routing cleanup: %q", w)
		}
	}
}

// Every command must be individually guarded so the script is a clean no-op
// when nothing exists. No bare nft/iptables delete may run without a `|| true`,
// a `|| break`, or being inside a guarded loop.
func TestReconcileScriptIsIdempotent(t *testing.T) {
	script := buildReconcileScript()
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
