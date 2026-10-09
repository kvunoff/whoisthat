package version

// CoreVersion is the single source of truth for the whoisthat-core daemon
// version. Bumped independently from the Rust TUI (Cargo.toml).
// Release tags: core-vX.Y.Z
const CoreVersion = "0.1.0"

// ProtocolVersion is the IPC contract version between whoisthat (TUI/CLI)
// and whoisthat-core. The TUI reuses a running core while the protocol
// matches and restarts it on mismatch. Bump only on breaking IPC changes.
const ProtocolVersion = 1
