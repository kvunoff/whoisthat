---
title: "Go Core Daemon (`whoisthat-core`)"
description: "How the Go daemon starts, dispatches commands, manages state, and tears down."
audience: "developer"
type: "explanation"
---

# Go Core Daemon (`whoisthat-core`)

The core is a long-running Go daemon. It owns the encrypted database,
subscriptions, routing, TUN, the kill-switch, the pinned Xray runtime and the
IPC server. The Rust client only sends commands and renders events.

## Startup lifecycle

`core/core/main.go` runs this sequence:

1. Parse `--version`/`--help`.
2. Configure the rotating log and crash output.
3. Refuse to start (exit 78) when the process holds `CAP_NET_ADMIN` but the uid
   has no entry in `/etc/whoisthat/authorized.d/`. Capless builds skip this.
4. Take the host-wide `flock` on `/var/lib/whoisthat/run.lock` and exit 78 when
   another core holds it; this keeps exactly one core on the host. A capless
   build continues when the lock file is absent; a capped core fails closed.
5. Create and validate the runtime directory (owner, mode `0700`, no symlink).
6. Load `config.json` (`AppConfig`). This happens before the per-uid lock so an
   `ipc-socket-path` override is known.
7. Acquire the per-uid single-instance flock. A second core of the same uid
   exits 0, so systemd `Restart=on-failure` does not loop.
8. Raise permitted network capabilities (`CAP_NET_ADMIN`, `CAP_NET_RAW`) into
   the inheritable and ambient sets for privileged children.
9. Refuse to start if another core still owns the IPC socket, and refuse (exit
   78) when an older core that does not take the global lock is still running.
10. If no core is live, run `tunmode.ReconcileOrphanedRules()` and re-arm the
    kill-switch when `kill-switch-enabled` is set.
11. Ensure geo assets (`geoip.dat`, `geosite.dat`).
12. Initialize the encrypted database and load the master key.
13. Initialize `ProxyManager`, its pre-start kill-switch handler, and the
    persisted test config.
14. Initialize `TunModeManager`.
15. Initialize `xraymgr` and, if needed, start a background runtime download.
16. Check for missing external binaries and prepare the warning hints.
17. Start `TCPServer` (UDS, optional TCP).
18. Start boot autoconnect and the subscription auto-update loop in a
    goroutine.
19. Install the SIGINT/SIGTERM handler and block.

The startup order matters: the lock and socket probes run before the orphan
reconcile, so a new core never tears down rules owned by a live one.

## Command dispatch

`lib/TCPServer` accepts connections and reads frames:

- 4-byte big-endian length + JSON, maximum 64 MiB (`framing.go`);
- UDS is gated by filesystem mode `0600` and a `SO_PEERCRED` uid check;
- the opt-in TCP listener requires a `hello` with the per-user token and a
  matching protocol version before any command is processed.

`dispatch.go` switches on `msg` and unmarshals into the matching
`structs.*Data` type. A per-request `cmd.Cmd` carries:

- `DB`, `TunManager` and `Broadcast`;
- `RequestID`, echoed on notifications produced by the request;
- an optional background `WaitGroup` for subscription work started by
  `AddGroup`/`UpdateGroup`.

The server copies `Cmd` per request, so one request cannot mutate another's
correlation id or wait group. `commands/connection-mutex.go` serializes
connection changes so two clients cannot race the Xray/TUN transition.

## State and subsystems

- `db` owns the encrypted files, group/profile/routing persistence, snapshots
  and the degraded read-only state.
- `ProxyManager` resolves routing, builds Xray JSON in memory, preflights it,
  starts/stops Xray and publishes stats.
- `TunModeManager` owns TUN, policy routing, split cgroups, DNS hijack,
  kill-switch and orphan reconciliation.
- `xraymgr` pins the Xray version, downloads and verifies it, and can adopt an
  exact system binary.
- `PortPool` leases ephemeral ports to temporary test Xray instances.
- `commands/subscription_scheduler.go` probes unknown intervals, then refreshes
  groups on their provider or manual schedule with backoff. Background refreshes
  do not run latency tests or reload a live DNS/routing/TUN session.

## Warnings, errors, and logs

Commands report recoverable failures with `cmd.warn(key, message)`, which emits
a `warn` notification carrying the request id. Fatal protocol/notification
problems use the `error` notification. Handlers log the underlying cause and
avoid claiming success after a failed state write.

The core log is `~/.config/whoisthat/core.log` (Lumberjack, 20 MB, two backups).
`core-crash.log` captures Go panic output. The TUI can change the live level
with `set-log-level`.

## Shutdown

SIGINT/SIGTERM or the `die` command starts a bounded teardown:

1. `ProxyManager.Stop()` stops the Xray child.
2. `TunModeManager.Stop()` removes TUN, policy routing, split and DNS-hijack
   rules.
3. The Unix socket is removed.
4. A `warn` with key `died` is broadcast.
5. Cleanup is bounded to 10 seconds; the next startup reconcile removes
   anything left behind.

The kill-switch block is deliberately **not** removed on shutdown. It is
removed only by `kill-switch=false` or by the startup reconcile (which re-arms
it if the intent is still enabled).

## Source map

| Area | Files |
| :--- | :--- |
| Entry point and startup | `core/core/main.go` |
| IPC server and dispatch | `core/core/lib/TCPServer/` |
| Command handlers | `core/core/commands/` |
| Database | `core/core/db/` |
| Proxy, routing, tester | `core/core/lib/proxy/mainproxy/` |
| TUN and kernel networking | `core/core/lib/proxy/tun/` |
| Xray runtime | `core/core/lib/xraymgr/` |
| Port leasing | `core/core/lib/PortPool/` |
| Config and storage | `core/core/lib/AppConfig/` |

## See also

- [Rust Client](/architecture/rust-client)
- [IPC & Event Flow](/architecture/ipc-flow)
- [Kernel Networking](/architecture/networking)
- [Routing Engine](/architecture/routing-engine)
