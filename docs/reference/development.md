# Development & Testing Guide

This guide explains how to build, test, debug, and contribute to the WhoisThat codebase.

---

## Local Development Loop

No installation step is required during development. The Rust TUI resolves `whoisthat-core` in this order: `PATH`, `./whoisthat-core`, `./core/core/whoisthat-core`, `./bin/whoisthat-core`, `/usr/bin`, and the legacy `/usr/local/bin`.

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
Every time you run `go build`, Linux assigns a new filesystem **inode** to the output binary, which automatically wipes any previously attached file capabilities (`setcap`). Remember to reapply capabilities after recompiling `whoisthat-core`. The TUI detects this on startup and offers a one-time `pkexec` prompt if capabilities are missing, but only in an interactive terminal and only when TUN autostart is configured; otherwise it points at the `setcap` command.
:::

::: tip Standalone core repository
`core/core` is a self-contained Go module (`module whoisthat-core`). It is mirrored to
[kvunoff/whoisthat-core](https://github.com/kvunoff/whoisthat-core) on every change under `core/core/**` and on every
`core-v*` tag. That repository is generated and read-only: open issues and pull requests here.
:::

---

## Annotated File Structure

```text
whoisthat/
├── .cargo/config.toml  ← static musl linker and rustflags for release builds
├── Cargo.toml          ← Rust project manifest (TUI & CLI dependencies)
├── README.md           ← Quick overview and reference guide
├── install.sh          ← Release downloader / source installer / updater
├── pkg/
│   ├── whoisthat.desktop ← Desktop launcher entry (opens the TUI in a terminal)
│   ├── whoisthat-url.desktop ← Hidden x-scheme-handler/wit deep link handler
│   ├── nfpm.yaml         ← deb/rpm package definition (binaries, entries, icon)
│   ├── package-postinstall.sh ← deb/rpm post-install: setcap + desktop/icon caches
│   ├── aur/              ← source AUR package (PKGBUILD + .SRCINFO + whoisthat.install)
│   └── aur-bin/          ← prebuilt AUR package (PKGBUILD + whoisthat-bin.install)
├── src/                ← Rust TUI & CLI source code
│   ├── main.rs         ← Entry point: CLI routing, core daemon spawn/re-attach, terminal lifecycle
│   ├── cli/            ← CLI: clap grammar, IPC session, selectors, output, command modules
│   │   ├── args.rs     ← clap definitions and hidden legacy aliases
│   │   ├── client.rs   ← Session (request-id correlation, spawn policy, timeouts)
│   │   ├── resolve.rs  ← group/profile/routing-profile selectors
│   │   ├── output.rs   ← JSON/human output, masking, tables
│   │   └── commands/   ← status, connection, core, profiles, subs, routing, tests, settings, logs, hwid, systemd, links, doctor
│   ├── doctor/         ← System & capability diagnostics (`whoisthat doctor`)
│   │   ├── mod.rs      ← Module entry point
│   │   ├── types.rs    ← Diagnostic models, pass/warn/fail statuses, reports
│   │   └── checks/     ← Diagnostic checks: binaries, capabilities, polkit, IPC, DNS, config/storage, logs
│   ├── event_loop.rs   ← AppEvent enum + run_loop (tokio::select! core_rx ↔ input_rx ↔ ticks)
│   ├── core_events.rs  ← handle_core_event — dispatches all CoreEvent broadcasts
│   ├── input/          ← Keyboard & mouse dispatch (mod.rs + navigation, profiles, routing, settings, mouse)
│   ├── popups/         ← Modal dialogs & routing popup key handlers (general, editors, routing)
│   ├── testing.rs      ← build_test_list, run_test_batch, persist_and_sync_test_config
│   ├── text_edit.rs    ← edit_text_field line editor + clipboard paste + UTF-8/Cyrillic support
│   ├── logger.rs       ← FileLogger (custom) + init/configure logger
│   ├── core_spawn.rs   ← spawn_core, find_core_binary, ensure_core_caps (pkexec fallback)
│   ├── systemd.rs      ← Unit-file generation, linger, enable/disable service
│   ├── net_info.rs     ← fetch_public_ip / fetch_public_ipv6 (HTTPS via ureq to ipify)
│   ├── deeplink/       ← wit:// parser, imports, and desktop entry/icon registration (parse, actions, desktop)
│   ├── launcher.rs     ← `whoisthat run <app>` — launch into split-tunnel slice (foreground/background)
│   ├── terminal.rs     ← Re-exec the TUI in a terminal when launched without a tty
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
│       ├── routing/    ← Routing tab, presets catalog, rule form & popup rendering
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
│           ├── xraymgr/     ← Pinned Xray-core runtime manager (v26.10.31 download & checksum verification)
│           ├── logger/      ← Level-filtered log wrapper (rotation configured in main.go)
│           ├── TCPServer/   ← IPC server + dispatcher (server, framing, auth, command dispatch)
│           ├── AppConfig/   ← Core configuration, defaults, HWID gen (config, storage, platform, apply)
│           ├── PortPool/    ← Dynamic port allocator for test/profile instances
│           ├── crypto/      ← AES-256-GCM encrypt/decrypt for DB files
│           ├── geo/         ← Auto-download + verify geoip.dat / geosite.dat from v2fly
│           ├── happ/        ← Happ routing: link parsing, payload decoder, geo sandbox
│           ├── happdns/     ← Happ DNS / FakeDNS compilation
│           ├── xrayjson/    ← Xray JSON assembly: routing, DNS, JSON subscriptions
│           ├── version/     ← CoreVersion and protocol.txt
│           └── proxy/       ← mainproxy (connect/stop/status, routing+DNS, tester+probes), xray wrapper, TUN manager + scripts
└── .github/workflows/
    ├── ci.yml          ← Automated CI: Rust, Go, shell, docs, versions and static musl build
    ├── release.yml     ← Builds release assets, publishes drafts and whoisthat-bin
    ├── aur-publish.yml ← Tag-triggered source-package publish to AUR
    ├── core-sync.yml   ← Mirrors core/core into kvunoff/whoisthat-core
    └── deploy-docs.yml ← Automated documentation build and deploy to GitHub Pages
```

---

## Log Files Reference

| Log File | Produced by | Lifecycle & Rotation | Default Severity |
| :--- | :--- | :--- | :---: |
| `~/.config/whoisthat/core.log` | Go Core | Lumberjack, 20 MB max, 2 backups | `info` (standalone); `warn` when spawned by the TUI |
| `$TMPDIR/whoisthat-<uid>/core.log` | Go Core | Same rotation; used when the config dir is not writable | same |
| `~/.local/share/whoisthat/tui.log` | Rust TUI | Rotated on write at 10 MiB, one `.1` backup, `0600` | `warn`; `info`+ with **Verbose TUI log** |
| `$TMPDIR/whoisthat-<uid>/tui.log` | Rust TUI | Same rotation; used when the data dir is not writable | same |
| `~/.local/share/whoisthat/core-crash.log` | Go Core | Unhandled panics/fatal errors; rotated at 1 MB | always |

Both components write `YYYY/MM/DD HH:MM:SS [LEVEL] message` (no fractional seconds, so older clients can still parse `core.log`).

---

## Running the Test Suites

Both the Rust client and Go daemon feature extensive unit test suites that run entirely in memory in milliseconds without requiring network access.

### 1. Rust Test Suite

```bash
cargo test
```

**Areas Covered:**
* **Protocol Framing & Dispatch** (`src/core_client/`): Tests handling of all core notification types, deserialization resilience, profile moving, and reordering.
* **Responsive Layout Tiers** (`src/ui/layout.rs`, `src/ui/app/helpers.rs`): Tests width tiers (Compact, Medium, Wide, UltraWide), height tiers (Tiny, Short, Normal, Tall), adaptive panel collapsing, and safety threshold (<45x8).
* **Routing Logic & Presets** (`src/ui/routing/`): Form-to-rule and rule-to-form round trips for all 6 match types, preset idempotency, live rule toggling, and Global / inherited / custom page rendering.
* **Global & Per-Group Routing State** (`src/ui/app/state/`): Page navigation including the Global page, inheritance fallback, custom-rule seeding, cached overrides, and page clamping after group deletion.
* **Text Editing & Cyrillic** (`src/text_edit.rs`): UTF-8 character insertion, boundary backspace, cursor positioning, Home/End boundaries.
* **Traffic History Ring Buffer** (`src/ui/traffic.rs`): 60-sample rolling buffer push, dynamic max-Y scale calculations.
* **Deep Links** (`src/deeplink/`): Scheme parser validation for `wit://sub`, `wit://import`, `wit://toggle`, and raw proxy URIs, plus desktop entry/icon generation.
* **Terminal Launch** (`src/terminal.rs`): Terminal detection order, per-emulator flags, explicit `terminal`/`$TERMINAL` prefixes, and system-default fallbacks.
* **CLI** (`src/cli/`): clap definition consistency, legacy alias normalization, selector resolution, settings validation, log parsing, secret masking, and fake-core request-id correlation.

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

### 3. IPC contract snapshots

`core/core/testdata/ipc-contract.json` is generated from the Go structs and parsed back by the Rust types. The Go test fails when the structs change, and the Rust test fails when the client can no longer round-trip the same payloads — so a field rename cannot silently drift across the language boundary.

After an intentional IPC change:

```bash
cd core/core && go test ./structs -update-contract
cd ../.. && cargo test ipc_contract_matches_go_snapshots
```

### 4. Privileged network-namespace tests

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
make test         # cargo test + go test ./... + scripts/bump.test.sh
make lint         # rustfmt --check, clippy, go vet, gofmt
make integration  # privileged netns suite
make docs         # VitePress build
make docs-check   # docs names/links/versions vs the code
make install      # install both binaries and set capabilities
```

The TUI needs **Rust 1.88 or newer** (`Cargo.toml` declares `rust-version`).
Source builds (`make build`, `install.sh --from-source`) use `--locked`, so
`Cargo.lock` is the authoritative dependency set; update it deliberately with
`cargo update`.

Release assets are static musl builds for `x86_64-unknown-linux-musl` and
`aarch64-unknown-linux-musl`. Locally install `musl-tools`, add the Rust target,
and build with `cargo build --release --locked --target <triple>`; the linker and
static flags live in `.cargo/config.toml`.

---

## Runtime Debugging & Inspection

### Verbose Logging

The TUI reads its level from `config.toml` (`log_level`). When `log_enabled = false` the level is capped at `warn` (errors always, warnings unless `log_level = "error"`); enabling **Verbose TUI log** removes the cap and writes the configured level. `RUST_LOG` or `WHOISTHAT_LOG_LEVEL` (a bare level, or `whoisthat=debug`) overrides both values for the session and applies the same level to the core; the override is not saved to `config.toml`. The TUI also passes the configured level when spawning the core and applies it to a running core over IPC (`set-log-level`).

For a standalone daemon with debug logging:

```bash
WHOISTHAT_LOG_LEVEL=debug ./core/core/whoisthat-core
```

Without the variable a standalone daemon uses `info`. `WHOISTHAT_LOG_LEVEL=debug cargo run` enables TUI debug output for that session. Debug logs are shared with all clients of the daemon.

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

## Version Bumps & Releases

`scripts/bump.sh` turns the release procedure into one validated command. It
never edits version files directly: every write goes through
`scripts/check-versions.sh --set`, which updates all mirrors (Cargo, npm,
PKGBUILD/.SRCINFO, docs) and re-checks them before returning.

Fill the two working texts first (they are gitignored under `.local/bump/`):

```text
.local/bump/commit.md    first non-empty line = commit summary, rest = body
.local/bump/release.md   release notes (bullets or prose)
```

Then run:

| Situation | Command |
| :--- | :--- |
| Major: `0.15.1 → 1.0.0` | `./scripts/bump.sh --kind tui --bump-type 1` |
| Minor: `0.15.1 → 0.16.0` | `./scripts/bump.sh --kind tui --bump-type 2` |
| Patch: `0.15.1 → 0.15.2` | `./scripts/bump.sh --kind tui --bump-type 3` |
| Packaging-only: new patch bundle | `./scripts/bump.sh --kind packaging --bump-type 3` |
| Core fix: new `core-v*` + TUI patch | `./scripts/bump.sh --kind core --bump-type 3` |
| Core + TUI | `./scripts/bump.sh --kind core-bundle --bump-type 3` |
| Breaking IPC | `./scripts/bump.sh --kind protocol --bump-type 3 --protocol 4` |

Add `--dry-run` to print the version diff and the planned commits/tags/releases
without writing anything, and `--push` to push `main`/tags and create **draft**
GitHub releases. After a successful `--push`, the texts move to `*.md.backup`
and the working files are reset to an empty template.

Published release assets are immutable. Every kind creates new tags; the script
never moves or force-pushes an existing `v*` tag. `core` therefore bumps the TUI
patch too, and `packaging` ships as a new patch bundle.

Safety rules enforced by the script:

- the texts must be filled (the template marker is rejected);
- the tree must already pass `scripts/check-versions.sh`;
- new tags and releases must not exist;
- `kind packaging` refuses `src/**` or `core/core/**` changes unless
  `--allow-source` is passed;
- untracked files are refused unless `--allow-untracked` is passed;
- version files are restored if a bump step fails before committing.

### Release assets

`bump.sh --push` creates `core-v*` and `v*` as drafts. The `release.yml`
workflow then builds static musl binaries and publishes:

* `v*`: `whoisthat-linux-<arch>.tar.gz` (both binaries, both desktop entries,
  icon and license), `.deb`, `.rpm`, and `SHA256SUMS`;
* `core-v*`: `whoisthat-core-linux-<arch>.tar.gz` and `SHA256SUMS`.

Only after every asset is uploaded does CI publish `core-v*`, publish `v*`, set
`v*` as GitHub Latest, and push `whoisthat-bin` to AUR. `install.sh` resolves
`/releases/latest`, so users never see a release without binaries. Every archive
also carries a GitHub build-provenance attestation (`gh attestation verify`).
The AUR
source package (`whoisthat`) is published independently by `aur-publish.yml`
from the tag. The first push of `whoisthat-bin` creates the AUR package base;
the workflow needs only the `AUR_SSH_KEY` secret.

`release.yml` can also be dispatched manually with a tag. It checks out that
tag (never the branch) and refuses unless `check-versions.sh --tag <tag>`
confirms the version files match. Published assets are immutable: a dispatch
attaches only missing assets and never overwrites an existing one, then
rebuilds `SHA256SUMS` from the release contents so `install.sh` still verifies.
Archives are not byte-reproducible (tar/gzip and deb/rpm embed build
timestamps), so content is not hash-compared.

---

## CI/CD Workflows

Continuous integration is handled via GitHub Actions in `.github/workflows/`:

* **`ci.yml`**: Runs on push and PRs targeting `main`:
  * Rust: `cargo fmt --all --check`, `cargo clippy --all-targets --no-deps -- -D warnings`, `cargo test` on a pinned rustc.
  * Go: `go vet ./...`, `go build ./...`, `go test ./...`, `go mod tidy -diff`, `govulncheck`.
  * Shell: `shellcheck -S warning` over `install.sh` and `scripts/*.sh`, then `./scripts/install.test.sh`, `./scripts/check-docs.test.sh`, `./scripts/release-upload.test.sh`.
  * Bump driver: `./scripts/bump.test.sh` (separate job).
  * Version consistency: `./scripts/check-versions.sh` (Cargo/package/PKGBUILD/aur-bin/.SRCINFO/docs/Xray pins).
  * Static build: `cargo build --target x86_64-unknown-linux-musl` plus a `CGO_ENABLED=0` core build, asserting neither binary has an `INTERP` segment.
  * Docs: `npm run docs:build` plus `./scripts/check-docs.sh` (IPC/config/CLI names, links, anchors, toolchain strings).
  * Integration: privileged netns suite (`-tags=integration`), failing if no `TestNetns*` ran.
* **`release.yml`**: Triggers on every `v*` tag push (or manually with `tag`):
  1. `verify` checks out the tag, runs `cargo test`, `go vet` and `go test`; `build` depends on it.
  2. Builds amd64/arm64 static musl binaries and verifies neither ELF has an `INTERP` segment (static or static-pie).
  3. Creates the bundle tarballs, `.deb`/`.rpm` via `nfpm`, and smoke-tests the packages in Debian and Fedora containers.
  4. Attests build provenance for `dist/*`.
  5. Uploads assets to the existing releases (drafts replaceable, published assets never overwritten), verifies the attestations, publishes `core-v*` first and `v*` last, marks `v*` Latest.
  6. Publishes `whoisthat-bin` to AUR with the release checksums.
  7. Runs the real `install.sh` in Debian, Ubuntu, Fedora and Arch containers.
* **`aur-publish.yml`**: Triggers on every `v*` tag push and publishes the
  **source** AUR package (`whoisthat`):
  1. Parses version from tag and runs `./scripts/check-versions.sh --tag <tag>`.
  2. Clones the live AUR repo.
  3. Computes `pkgrel` (increments if `pkgver` matches latest AUR, resets to 1 for a new version, and reuses the repo `pkgrel` when rolling back to an older version).
  4. Regenerates `PKGBUILD` and `.SRCINFO` in CI.
  5. Pushes to AUR using the `AUR_SSH_KEY` secret.
* **`core-sync.yml`**: Mirrors `core/core` into `kvunoff/whoisthat-core` on
  `core/core/**` changes and `core-v*` tags.
* **`deploy-docs.yml`**: Builds and deploys this VitePress documentation to GitHub Pages whenever changes are committed to `docs/**`.
