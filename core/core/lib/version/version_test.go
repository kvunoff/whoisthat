package version

import (
	"strings"
	"testing"
)

func TestVersionConstants(t *testing.T) {
	if CoreVersion == "" {
		t.Error("CoreVersion must not be empty")
	}
	if strings.HasPrefix(CoreVersion, "v") {
		t.Errorf("CoreVersion must not include leading v, got %q", CoreVersion)
	}
	if ProtocolVersion < 1 {
		t.Errorf("ProtocolVersion must be >= 1, got %d", ProtocolVersion)
	}
}
