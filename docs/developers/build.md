---
title: "Build & Run"
description: "Build the Go core and Rust client, apply capabilities, and run a local development build."
audience: "developer"
type: "how-to"
---

# Build & Run

This page covers the local build loop and the unified `make` targets. For the full test matrix see [Testing](/developers/testing); for release builds see [Releasing](/developers/releasing).

## Local development loop

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

## Unified build commands

A root `Makefile` wraps the common workflows:

```bash
make build        # whoisthat-core + release TUI
make test         # cargo test + go test ./... + scripts/bump.test.sh
make lint         # rustfmt --check, clippy, go vet, gofmt
make integration  # privileged netns suite
make test-e2e     # real core in a temp XDG sandbox, read back through the CLI
make coverage     # Rust lcov + Go coverprofile (report only, no gate)
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

## See also

- [Repository Map](/developers/repository)
- [Testing](/developers/testing)
- [Debugging](/developers/debugging)
