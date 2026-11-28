---
title: "Build & Run"
description: "Build the Go core and Rust client, apply capabilities, and run a local development build."
audience: "developer"
type: "how-to"
---

# Build & Run

This page covers the local build loop and the unified `make` targets. For the full test matrix see [Testing](/developers/testing); for release builds see [Releasing](/developers/releasing).

## Local development loop

Proxy mode needs no installation. The Rust TUI resolves `whoisthat-core` in this order: `PATH`, `./whoisthat-core`, `./core/core/whoisthat-core`, `./bin/whoisthat-core`, `/usr/bin`, and the legacy `/usr/local/bin`.

```bash
# 1. Build the Go core daemon (must be built first)
cd core/core
go build -o whoisthat-core
cd ../..

# 2. For TUN mode, stage the core as root-owned and authorize this user
sudo install -Dm755 core/core/whoisthat-core /usr/lib/whoisthat/dev/whoisthat-core
sudo /usr/lib/whoisthat/authorize-core --bin /usr/lib/whoisthat/dev/whoisthat-core --add "$(id -u)"

# 3. Launch the Rust TUI with the staged core first on PATH
PATH="/usr/lib/whoisthat/dev:$PATH" cargo run
```

`authorize-core --bin` only accepts a root-owned binary, so a raw `go build`
output owned by your user is not accepted. Staging it under
`/usr/lib/whoisthat/dev` keeps the packaged `/usr/bin/whoisthat-core` untouched
and puts the dev core first on `PATH`. The helper comes from a package or
`make install`; from a clean checkout, run `sudo scripts/authorize-core` with
the same arguments.

::: warning Inode Invalidation Gotcha
Every time you run `go build`, Linux assigns a new filesystem **inode** to the output binary, which automatically wipes any previously attached file capabilities. Repeat the staging and `authorize-core --bin` steps after recompiling `whoisthat-core`. The TUI detects a missing authorization when it spawns a core and offers a one-time `pkexec` prompt, but only in an interactive terminal; otherwise it prints the `authorize-core --add` command. A build without file capabilities still works in proxy mode, so proxy-only work can skip step 2.
:::

::: tip Standalone core repository
`core/core` is a self-contained Go module (`module whoisthat-core`). It is mirrored to
[kvunoff/whoisthat-core](https://github.com/kvunoff/whoisthat-core) on every change under `core/core/**` and on every
`core-v*` tag. That repository is generated and read-only: open issues and pull requests here.
:::

---

## Run the standalone core

Download the `whoisthat-core-linux-<arch>.tar.gz` asset from a `core-v*`
release and verify it against the release's `SHA256SUMS`. Extract it into a
private directory. For proxy-only use, run `./whoisthat-core` as your ordinary
user. To install system-wide, run `sudo ./install-core --user "$USER"`. Add
`--tun` for TUN authorization, `--service` to register a service, and
`--split` with `--tun` to set up split tunneling. A systemd user service
prints the activation command for the target user. The installer supports
OpenRC, Dinit and runit as well. To upgrade, stop the daemon and run the
new archive's installer with the original options.

Resolve the socket using [IPC & Wire Protocol](/reference/protocol) and
request `get-application-state`. Wait for `xray_status.status` to be `ready`,
using `xray-progress` or another state request if the runtime is downloading.
Create a group with `add-group`, import a share link with `add-profiles`, then
send `connect` with the returned group and profile IDs. Use `proxy_ports` when
the core advertises `config-state`. For system-wide VPN, send `enable-tun`
after connecting. To split an application, send `set-split-tunnel` and run
`whoisthat-run -- <program> [args...]` as the same user.

## Unified build commands

A root `Makefile` wraps the common workflows:

```bash
make build        # whoisthat-core + release TUI
make test         # Go, Rust and every scripts/*.test.sh
make lint         # fmt, clippy, go vet, gofmt, swallowed-error, version and docs checks
make lint-shell   # shellcheck over every tracked shell script
make integration  # privileged netns suite
make test-e2e     # real core in a temp XDG sandbox, read back through the CLI
make test-core-e2e # raw IPC and real Xray without the Rust client
make test-xray    # finalmask combinations against the pinned Xray binary
make test-ppa     # build a PPA source package and inspect the .deb contents
make coverage     # Rust lcov + Go coverprofile with the same line floors as CI
make docs         # VitePress build
make docs-check   # docs names/links/versions vs the code
make install      # install binaries, set capabilities, authorize the user
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

## See also

- [Repository Map](/developers/repository)
- [Testing](/developers/testing)
- [Debugging](/developers/debugging)
