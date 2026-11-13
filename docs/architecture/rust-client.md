---
title: "Rust Client"
description: "How the Rust TUI, CLI, event loop, UI state, and core client fit together."
audience: "developer"
type: "explanation"
---

# Rust Client

The `whoisthat` binary is both the TUI and the CLI. It never owns VPN state:
profiles, routing, TUN, the database and Xray belong to the Go core. The Rust
side renders, collects input, and translates commands and events over IPC.

## Startup

`src/main.rs` is the entry point:

1. A `wit://` argument is routed to `deeplink::run_deeplink` and exits.
2. `cli::run` parses the command tree. A subcommand runs head-less and exits;
   no subcommand continues into the TUI.
3. Privilege-wrapper checks reject `sudo`/`pkexec` launches.
4. `config::load_config` loads `config.toml`; `terminal::relaunch` re-execs in a
   terminal when the desktop entry starts without a tty.
5. The core is attached or spawned, and the TUI enters `event_loop::run_loop`.

The CLI and TUI share `core_client`; they differ in who owns the terminal and
whether they keep a long-lived event stream.

## Event loop

`src/event_loop.rs` defines `AppEvent` and a single `tokio::select!` loop:

```text
core_rx (CoreEvent) ─┐
input_rx (AppEvent) ─┼─► run_loop ─► handle_core_event / handle_input
ticks / public IP  ─┘                     │
                                          ▼
                                       App::render
```

`run_loop` draws every iteration, then waits for one of:

- a `CoreEvent` from the IPC read loop;
- an input or timer event (`Input`, `Tick`, `PublicIp`, `PublicIpv6`,
  `UpdateAvailable`);
- channel closure, which ends the loop.

Before drawing, the loop asks for details of the selected managed JSON profile
once (`request_selected_profile_details`). This keeps provider stream metadata
out of `application-state`.

## Core events and UI state

`src/core_events.rs::handle_core_event` maps each `CoreEvent` into the UI:

- `ApplicationState` applies groups, profiles, connection, TUN, routing,
  `config_status`/`db_status`, and the core's authoritative `tun_name` /
  `test_config`;
- `StatusChanged`, `TunStatusChanged`, `TrafficStats`, `ProfileUpdated` update
  live state;
- `Warning` and `Error` become notices;
- `SubscriptionUpdated` replaces a group and its profiles.

The state machine lives in `src/ui/app/state/`; rendering is split into
`render.rs`, `tree.rs`, `details.rs`, and `popups.rs`. Input dispatch lives in
`src/input/` and is resolved through `src/keymap/` (`KeyContext`,
`resolve_action`), so the help tables and the actual bindings share one source.

## Core client

`src/core_client/` owns the transport:

| Module | Responsibility |
| :--- | :--- |
| `connection.rs` | `CoreConnection`, UDS/TCP streams, 4-byte frame I/O, 64 MiB cap |
| `dispatch.rs` | read loop, typed `CoreEvent`, reconnect/respawn |
| `commands.rs` | typed request helpers |
| `protocol.rs` | serde types mirroring `core/core/structs/` |

The TUI uses `CoreClient` and a broadcast channel. The CLI uses
`cli/client.rs::Session`, which adds request-id correlation: a request can wait
for a frame carrying its `id`, while background broadcasts are ignored. The same
file implements the spawn policy (`open_or_spawn`) and the per-command reply
timeouts.

If the core protocol version does not match
`EXPECTED_CORE_PROTOCOL_VERSION`, the TUI does not attach: it stops a mismatched
core and respawns it. [IPC & Event Flow](/architecture/ipc-flow) describes the
wire side.

## Lifecycle and services

`src/lifecycle.rs` selects the service backend (`systemd`, `init`, `desktop`)
and writes the session markers used by the init wrapper. `src/autostart.rs`
handles enable/disable, `src/systemd.rs` renders the user unit, and
`src/core_spawn.rs` resolves the core binary, checks capabilities, and offers
`pkexec` only when the situation allows it.

`whoisthat-runner` is a separate binary (`src/bin/whoisthat-runner.rs`) built
from `src/runner/`: framed spawn/control messages, a socket server, and PTY
handling. It is the non-systemd split-tunnel path.

## Logging and diagnostics

`src/logger.rs` writes the TUI log with rotation; `src/logfile.rs` tails log
files; `src/logfmt.rs` parses the shared line format. The TUI applies its level
locally and sends `set-log-level` to the core. `src/doctor/` is a separate
diagnostic framework with typed checks and a JSON report.

## Source map

| Area | Files |
| :--- | :--- |
| Entry point and startup | `src/main.rs`, `src/core_spawn.rs`, `src/terminal.rs` |
| Event loop | `src/event_loop.rs`, `src/core_events.rs` |
| UI and state | `src/ui/app/`, `src/ui/` |
| Input and keymap | `src/input/`, `src/keymap/` |
| Core client | `src/core_client/`, `src/cli/client.rs` |
| Lifecycle and services | `src/lifecycle.rs`, `src/autostart.rs`, `src/systemd.rs`, `src/platform.rs` |
| Runner | `src/bin/whoisthat-runner.rs`, `src/runner/` |
| Logging and doctor | `src/logger.rs`, `src/logfile.rs`, `src/logfmt.rs`, `src/doctor/` |

## See also

- [IPC & Event Flow](/architecture/ipc-flow)
- [Go Core Daemon](/architecture/go-core)
- [Build & Run](/developers/build)
- [Repository Map](/developers/repository)
