package version

import (
	_ "embed"
	"strconv"
	"strings"
)

// CoreVersion is the single source of truth for the whoisthat-core daemon
// version. Bumped independently from the Rust TUI (Cargo.toml).
// Release tags: core-vX.Y.Z
const CoreVersion = "0.3.6"

// protocol.txt is the shared wire-contract version. The Rust client embeds the
// same file, so the two sides can no longer drift silently.
//
//go:embed protocol.txt
var protocolVersionRaw string

// ProtocolVersion is the IPC contract version between whoisthat (TUI/CLI)
// and whoisthat-core. The TUI reuses a running core while the protocol
// matches and restarts it on mismatch. Bump only on breaking IPC changes.
var ProtocolVersion = func() int {
	n, err := strconv.Atoi(strings.TrimSpace(protocolVersionRaw))
	if err != nil {
		panic("invalid core/core/lib/version/protocol.txt: " + err.Error())
	}
	return n
}()
