---
title: "Testing"
description: "Run the Rust, Go, contract, netns, end-to-end, and fuzz test suites."
audience: "developer"
type: "how-to"
---

# Testing

The test suites are layered: fast in-memory unit tests, contract snapshots, privileged network tests, and an end-to-end IPC smoke test.

## Test suites

Both the Rust client and Go daemon feature extensive unit test suites that run entirely in memory in milliseconds without requiring network access.

### Rust Test Suite

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

### Go Core Test Suite

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

### IPC contract snapshots

`core/core/testdata/ipc-contract.json` is generated from the Go structs and parsed back by the Rust types. The Go test fails when the structs change, and the Rust test fails when the client can no longer round-trip the same payloads — so a field rename cannot silently drift across the language boundary.

After an intentional IPC change:

```bash
cd core/core && go test ./structs -update-contract
cd ../.. && cargo test ipc_contract_matches_go_snapshots
```

### Privileged network-namespace tests

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

### End-to-end IPC smoke test

The contract snapshot above checks the payload types, not the transport or the
daemon startup. `scripts/ipc-e2e.test.sh` starts a real `whoisthat-core` in a
throwaway XDG sandbox and reads its state back through
`whoisthat core status --json`.

`WHOISTHAT_TEST_MODE=1` skips the geo/Xray downloads, the startup firewall
reconcile and subscription updates, so the test needs neither root nor network
and leaves the user's config and nftables alone.

```bash
make core && cargo build   # both binaries are required
make test-e2e
```

It refuses to run as root.

---

## Manual release QA on non-systemd

CI has no non-systemd runner, so this is the release checklist:

```sh
sudo /usr/lib/whoisthat/cgroup-setup "$USER"
sudo /usr/lib/whoisthat/install-init cgroup enable
sudo /usr/lib/whoisthat/install-init runner enable "$USER" "$HOME"
sudo /usr/lib/whoisthat/install-init core enable "$USER" "$HOME"

whoisthat doctor
whoisthat run /bin/sh -c 'cat /proc/self/cgroup'   # /whoisthat-<uid>
whoisthat run vim /tmp/x                            # full-screen, Ctrl-C, resize
whoisthat core stop; whoisthat core start; whoisthat core restart
whoisthat autostart status --json
```

Reboot once and repeat: the cgroup hook must recreate
`/sys/fs/cgroup/whoisthat-<uid>`, clear `core-stop`/`core-*` markers, and start
the core before login. Then check `whoisthat autostart enable --backend init`
survives a reboot and `disable` stops the core after it exits.

## Latency tester internals

The tester does not use ICMP. Each sample simulates application traffic:

- `tcp`: fast direct TCP handshake prefilter.
- `http-get` (default): starts a temporary Xray instance and performs an HTTP
  GET over SOCKS5.
- `http-head`: same path with a lighter HEAD request.

The median of the configured samples is shown in the tree; jitter is
`max - min`; loss is the share of failed samples. Pending profiles show `…`, and
the status bar reports `[Testing done/total]`. The core throttles failure
warnings to one per reason per five seconds.

When **Auto-test on refresh** is enabled, a subscription refresh schedules tests
for the imported profiles; groups larger than 50 profiles are skipped to avoid a
test storm.

---

## See also

- [Build & Run](/developers/build)
- [Debugging](/developers/debugging)
- [Profiles & Subscriptions](/usage/profiles)
