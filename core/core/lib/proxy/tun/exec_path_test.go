package tunmode

import (
	"strings"
	"testing"
)

func TestScriptsRunWithPinnedPATH(t *testing.T) {
	t.Setenv("PATH", "/tmp/attacker-bin:/home/u/.local/bin")
	out, err := runScriptWithSh("echo \"$PATH\"")
	if err != nil {
		t.Fatal(err)
	}
	if got := strings.TrimSpace(out); strings.Contains(got, "attacker") || strings.Contains(got, ".local") {
		t.Fatalf("shell inherited caller PATH: %q", got)
	}
}
