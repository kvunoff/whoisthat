# Development & Testing Guide

This guide explains how to build, test, debug, and contribute to the WhoisThat codebase.

---

## Local Development Loop

No installation step is required during development. The Rust TUI automatically searches for the core binary in `./core/core/whoisthat-core`, falling back to `PATH` and `/usr/local/bin/`.

```bash
# 1. Build the Go core daemon (must be built first)
cd core/core
go build -o whoisthat-core
cd ../..

# 2. Grant capabilities to the newly built binary
sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep ./core/core/whoisthat-core

# 3. Launch the Rust TUI
cargo run
```

::: warning Inode Invalidation Gotcha
Every time you run `go build`, Linux assigns a new filesystem **inode** to the output binary, which automatically wipes any previously attached file capabilities (`setcap`). Remember to reapply capabilities after recompiling `whoisthat-core`. The TUI automatically detects this on startup and offers a one-time `pkexec` prompt if capabilities are missing.
:::

---

## Annotated File Structure

```text
whoisthat/
├── Cargo.toml          ← Rust project manifest (TUI & CLI dependencies)
├── README.md           ← Quick overview and reference guide
├── install.sh          ← Universal installer / updater script
├── pkg/
│   ├── whoisthat.desktop ← Desktop launcher entry & x-scheme-handler/wit MIME association
│   └── aur/              ← AUR packaging (PKGBUILD + .SRCINFO + whoisthat.install)
├── src/                ← Rust TUI & CLI source code
│   ├── main.rs         ← Entry point: CLI routing, core daemon spawn/re-attach, terminal lifecycle
│   ├── cli.rs          ← CLI subcommands & shorthand flags, status monitoring, JSON/NDJSON streams
│   ├── doctor/         ← System & capability diagnostics (`whoisthat doctor`)
│   │   ├── mod.rs      ← Module entry point
│   │   ├── types.rs    ← Diagnostic models, pass/warn/fail statuses, reports
│   │   └── checks.rs   ← Diagnostic checks: binaries, capabilities, polkit, IPC, config/storage
│   ├── event_loop.rs   ← AppEvent enum + run_loop (tokio::select! core_rx ↔ input_rx ↔ ticks)
│   ├── core_events.rs  ← handle_core_event — dispatches all CoreEvent broadcasts
│   ├── input.rs        ← handle_input + handle_normal_input — keyboard & mouse dispatch by tab/focus
│   ├── popups.rs       ← Modal dialogs & routing popup key handlers, two-field forms
│   ├── testing.rs      ← build_test_list, run_test_batch, persist_and_sync_test_config
│   ├── text_edit.rs    ← edit_text_field line editor + clipboard paste + UTF-8/Cyrillic support
│   ├── logger.rs       ← FileLogger (custom) + init/configure logger
│   ├── core_spawn.rs   ← spawn_core, find_core_binary, ensure_core_caps (pkexec fallback)
│   ├── systemd.rs      ← Unit-file generation, linger, enable/disable service
│   ├── net_info.rs     ← fetch_public_ip / fetch_public_ipv6 (raw TCP to ipify)
│   ├── deeplink.rs     ← Deep link routing (wit:// scheme parser, subscription & profile import)
│   ├── launcher.rs     ← `whoisthat run <app>` — launch into split-tunnel slice (foreground/background)
│   ├── config.rs       ← Config loader (~/.config/whoisthat/config.toml)
│   ├── core_client/    ← IPC client for the Go core (Unix domain socket / TCP)
│   │   ├── protocol.rs ← All serde types mirroring Go structs
│   │   ├── connection.rs ← UDS/TCP transport + 4-byte length framing
│   │   ├── dispatch.rs ← Read loop → typed event channel
│   │   └── commands.rs ← High-level async send functions
│   └── ui/             ← Ratatui UI components
│       ├── app/        ← Main app state & rendering (state, tree, details, popups, helpers)
│       ├── layout.rs   ← Responsive layout geometry engine (width/height tiers, adaptive splits)
│       ├── theme.rs    ← Curated color palettes (4 themes: Graphite Dark, Graphite Light, Tokyo Night, Dracula)
│       ├── settings.rs ← Settings screen
│       ├── routing.rs  ← Routing rules tab + presets catalog + popups
│       ├── traffic.rs  ← Real-time traffic monitoring tab (Braille charts + live transfer stats)
│       ├── logs.rs     ← Log viewer (live tail + auto-scroll + level filter)
│       └── uri.rs      ← URI detail parser (VLESS/VMess/Trojan/SS/SOCKS/Hysteria2)
├── core/               ← WhoisThat Core (Go VPN engine)
│   └── core/
│       ├── main.go     ← Daemon entry point; RaiseAmbientCaps() before everything
│       ├── go.mod
│       ├── commands/   ← IPC command handlers (connect, test, groups, subscription, split, ...)
│       ├── structs/    ← Shared data types (mirror of src/core_client/protocol.rs)
│       ├── db/         ← Encrypted JSON profile DB (AES-256-GCM) + auto-migration
│       ├── utils/      ← caps.go (RaiseAmbientCaps, CanTun), user.go (UIDs), resolve.go
│       └── lib/        ← Core libraries
│           ├── parser/      ← Native in-memory URI parser & Xray JSON config generator
│           ├── xraymgr/     ← Pinned Xray-core runtime manager (v26.9.9 download & checksum verification)
│           ├── logger/      ← Structured logger (lumberjack rotation, 20 MB)
│           ├── TCPServer/   ← IPC server + dispatcher (Unix socket / TCP, length-prefixed JSON)
│           ├── AppConfig/   ← Core configuration, defaults, HWID gen
│           ├── PortPool/    ← Dynamic port allocator for test/profile instances
│           ├── crypto/      ← AES-256-GCM encrypt/decrypt for DB files
│           ├── geo/         ← Auto-download + verify geoip.dat / geosite.dat from v2fly
│           └── proxy/       ← mainproxy (connect/stop/status), xray wrapper, TUN manager + scripts, routing
└── .github/workflows/
    ├── ci.yml          ← Automated CI: Rust (fmt/clippy/test) + Go (vet/build/test)
    ├── aur-publish.yml ← Tag-triggered CI: pushes updated PKGBUILD to AUR
    └── deploy-docs.yml ← Automated documentation build and deploy to GitHub Pages
```

---

## Log Files Reference

| Log File | Produced by | Lifecycle & Rotation | Default Severity |
| :--- | :--- | :--- | :---: |
| `/tmp/whoisthat-tui.log` | Rust TUI | Emergency fallback if the data directory is read-only | `warn` |
| `~/.config/whoisthat/core.log` | Go Core | Lumberjack rotation (20 MB max, 1 backup) | `warn` |
| `/tmp/whoisthat-core.log` | Go Core | Emergency fallback if config directory is read-only | `warn` |
| `~/.local/share/whoisthat/tui.log` | Rust TUI | Active when **TUI Log** setting is enabled | Configured level |

---

## Running the Test Suites

Both the Rust client and Go daemon feature extensive unit test suites that run entirely in memory in milliseconds without requiring network access.

### 1. Rust Test Suite (149 Tests)

```bash
cargo test
```

**Areas Covered:**
* **Protocol Framing & Dispatch** (`src/core_client/`): Tests handling of all core notification types, deserialization resilience, profile moving, and reordering.
* **Responsive Layout Tiers** (`src/ui/layout.rs`, `src/ui/app/helpers.rs`): Tests width tiers (Compact, Medium, Wide, UltraWide), height tiers (Tiny, Short, Normal, Tall), adaptive panel collapsing, and safety threshold (<45x8).
* **Routing Logic & Presets** (`src/ui/routing.rs`): Form-to-rule and rule-to-form round trips for all 6 match types, preset idempotency, live rule toggling, and Global / inherited / custom page rendering.
* **Global & Per-Group Routing State** (`src/ui/app/state.rs`): Page navigation including the Global page, inheritance fallback, custom-rule seeding, cached overrides, and page clamping after group deletion.
* **Text Editing & Cyrillic** (`src/text_edit.rs`): UTF-8 character insertion, boundary backspace, cursor positioning, Home/End boundaries.
* **Traffic History Ring Buffer** (`src/ui/traffic.rs`): 60-sample rolling buffer push, dynamic max-Y scale calculations.
* **Deep Links** (`src/deeplink.rs`): Scheme parser validation for `wit://sub`, `wit://import`, `wit://toggle`, and raw proxy URIs.

### 2. Go Core Test Suite

```bash
cd core/core
go test ./...
```

**Areas Covered:**
* **AES-256-GCM Crypto** (`lib/crypto`): Nonce uniqueness, key validation, tampering detection, base64 wrapping, short ciphertext handling.
* **Database Operations** (`db`): Encrypted read/write round-trips, key persistence across restarts, directory initialization, manual routing inheritance, custom-rule seeding, and cleanup on group deletion.
* **Proxy URI Parsers** (`lib/parser`): In-memory compilation of VLESS (Reality, Vision, xHTTP, gRPC), VMess, Trojan, Shadowsocks, SOCKS5, and Hysteria2 URIs into Xray JSON.
* **PortPool Allocator** (`lib/PortPool`): Concurrent ephemeral port lease and release without race conditions.
* **Startup Reconcile & Split** (`lib/proxy/tun`): Idempotency of nftables teardown scripts and routing table rule restoration.

### 3. Privileged network-namespace tests

`go test ./...` only checks the *text* of the ip/nftables scripts. The
`integration` build tag adds tests that actually execute them inside a
throwaway network namespace: TUN create/delete, startup reconcile after a
simulated crash, kill-switch install/remove, and `rp_filter` save/restore.

```bash
sudo -E go test -tags=integration ./lib/proxy/tun/ -run 'TestNetns' -v
# or, equivalently
sudo -E make integration
```

They require root (or `CAP_NET_ADMIN`), `nft` and `ip`, and skip themselves
when `/dev/net/tun` is unavailable. CI runs them on every push.

---

## Unified build commands

A root `Makefile` wraps the common workflows:

```bash
make build        # whoisthat-core + release TUI
make test         # cargo test + go test ./...
make lint         # rustfmt --check, clippy, go vet, gofmt
make integration  # privileged netns suite
make docs         # VitePress build
make install      # install both binaries and set capabilities
```

---

## Runtime Debugging & Inspection

### Verbose Logging

```bash
WHOISTHAT_LOG_LEVEL=debug cargo run
```

### Manual Socket and Daemon Inspection

```bash
# Check if the Unix domain socket is active (default)
ss -xlp | grep core.sock

# Check if legacy TCP listener is active (when tcp-enabled: true)
ss -tlnp | grep 4897

# Live tail of the core's rotating log
tail -f ~/.config/whoisthat/core.log

# Inspect active Linux policy routing rules
ip rule show
ip route show table 100
ip route show table 200

# Inspect active nftables firewall tables
nft list table inet whoisthat_split
nft list table inet whoisthat_ks
```

---

## CI/CD Workflows

Continuous integration is handled via GitHub Actions in `.github/workflows/`:

* **`ci.yml`**: Runs on push and PRs targeting `main`:
  * Rust: `cargo fmt --all --check`, `cargo clippy --all-targets --no-deps`, `cargo test`.
  * Go: `go vet ./...`, `go build ./...`, `go test ./...`, `go mod tidy -diff`, `govulncheck`.
  * Shell: `shellcheck install.sh`.
  * Integration: privileged netns suite (`-tags=integration`).
* **`aur-publish.yml`**: Triggers on every `v*` tag push:
  1. Parses version from tag.
  2. Clones the live AUR repo.
  3. Computes `pkgrel` (increments if `pkgver` matches latest AUR, otherwise resets to 1).
  4. Regenerates `PKGBUILD` and `.SRCINFO` in CI.
  5. Pushes to AUR using the `AUR_SSH_KEY` secret.
* **`deploy-docs.yml`**: Builds and deploys this VitePress documentation to GitHub Pages whenever changes are committed to `docs/**`.
