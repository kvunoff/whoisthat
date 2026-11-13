---
title: "IPC & Event Flow"
description: "How requests, replies, broadcasts, reconnects, and protocol mismatches flow between the client and core."
audience: "developer"
type: "explanation"
---

# IPC & Event Flow

The client and core speak a length-prefixed JSON protocol over a Unix socket
(or opt-in TCP). The wire schema, command table and payload shapes are in
[IPC & Wire Protocol](/reference/protocol). This page is about the flow: who
waits for what, how replies are correlated, and what happens when the core
disappears.

## Framing and transports

Each frame is a 4-byte big-endian length followed by UTF-8 JSON:

```text
[uint32 length][{"msg":"...","data":{...}}]
```

The 64 MiB cap is enforced on both sides. UDS is the default (`0600`,
`$XDG_RUNTIME_DIR/whoisthat/core.sock` or the runtime-dir fallback). The TCP
listener is loopback-only and requires a `hello` with the token from
`ipc.token` plus protocol version 5. A failed handshake is closed without
processing commands.

## Attach sequence

1. The client connects to the UDS or TCP endpoint.
2. On attach it sends `get-application-state`.
3. The core replies with `application-state` and starts broadcasting live
   events to that connection.
4. The TUI applies the snapshot, then keeps the read loop open for
   `CoreEvent`s.

Read-only CLI commands do not spawn a core. Mutating commands call
`open_or_spawn`, which starts a local core only for the default endpoint;
`--tcp`/`--socket` never spawn, because the new daemon would listen elsewhere.

## Requests and correlation

A request may carry an `id` string. The core copies it into a per-request
`cmd.Cmd`, and every notification produced while handling that request echoes
the id. This lets a CLI process ignore unrelated broadcasts and other clients'
events.

Two patterns coexist:

| Pattern | Example | What to wait for |
| :--- | :--- | :--- |
| Direct reply | `get-application-state`, `profile-details` | the id-carrying reply |
| Broadcast reply | `status-changed`, `traffic-stats`, `test-progress` | a matching broadcast, usually with a predicate |
| Two frames | `update-routing` / `set-manual-routing-source` | `group-updated`, then `routing-updated` |
| Two frames | `add-group` / `update-group` | `group-added`/`group-updated`, then `subscription-updated` |
| Two frames | `test-group` | the first `test-progress` carries the id; later ones are broadcasts |

`set-kill-switch` and `set-split-tunnel` are special: the reply is
authoritative and may be followed (or preceded) by a `*-failed` warning.
Compare the requested value with the reply, not the warning alone. The
kill-switch reply carries both `enabled` (intent) and `armed` (kernel state).

## Broadcasts

Events without a request id go to every connected client:

- `status-changed`, `tun-status-changed`;
- `traffic-stats` (1/s while connected);
- `test-progress` after the first frame;
- `profile-updated` from background test results;
- `subscription-updated` from the scheduler;
- `xray-progress`;
- `warn` with key `died`.

The TUI consumes them through `core_rx`; the CLI filters for the frame it
needs.

## Disconnects and protocol mismatch

On UDS, a mismatched core is detected from `application-state.protocol_version`.
The TUI does not attach: it stops the mismatched core and respawns it. On TCP,
the `hello` handshake rejects the connection instead.

If the socket closes, the client read loop ends. The TUI can respawn the core
through the normal lifecycle path and reattach; a CLI command exits with code 3
when spawning is disabled or fails. Background events are not replayed: the
client requests a fresh `application-state` on the next attach.

## Client implementations

- `src/core_client/dispatch.rs` maps frames to typed `CoreEvent`s and drives the
  TUI read loop.
- `src/cli/client.rs::Session` adds request-id waiting, fatal-warning handling,
  per-command timeouts, and `open_or_spawn`.
- `lib/TCPServer/server.go` accepts connections, performs the TCP handshake,
  and copies a `Cmd` per request.
- `lib/TCPServer/dispatch.go` routes `msg` to a handler.

## Source map

| Area | Files |
| :--- | :--- |
| Client transport and read loop | `src/core_client/{connection,dispatch,commands}.rs` |
| CLI correlation | `src/cli/client.rs` |
| Core framing and server | `core/core/lib/TCPServer/{framing,server}.go` |
| Core dispatch | `core/core/lib/TCPServer/dispatch.go` |
| Per-request state | `core/core/commands/commands.go` |
| Wire schema | `docs/reference/protocol.md` |

## See also

- [IPC & Wire Protocol](/reference/protocol)
- [Rust Client](/architecture/rust-client)
- [Go Core Daemon](/architecture/go-core)
