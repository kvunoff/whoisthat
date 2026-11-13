---
title: "Debugging"
description: "Find log files, raise log levels, and inspect the core socket, routing, and firewall."
audience: "developer"
type: "how-to"
---

# Debugging

Start with logs, then inspect the socket and kernel state. For runtime diagnostics use `whoisthat doctor`.

## Log files

| Log File | Produced by | Lifecycle & Rotation | Default Severity |
| :--- | :--- | :--- | :---: |
| `~/.config/whoisthat/core.log` | Go Core | Lumberjack, 20 MB max, 2 backups | `info` (standalone); `warn` when spawned by the TUI |
| `$TMPDIR/whoisthat-<uid>/core.log` | Go Core | Same rotation; used when the config dir is not writable | same |
| `~/.local/share/whoisthat/tui.log` | Rust TUI | Rotated on write at 10 MiB, one `.1` backup, `0600` | `warn`; `info`+ with **Verbose TUI log** |
| `$TMPDIR/whoisthat-<uid>/tui.log` | Rust TUI | Same rotation; used when the data dir is not writable | same |
| `~/.local/share/whoisthat/core-crash.log` | Go Core | Unhandled panics/fatal errors; rotated at startup above 1 MB | always |

Both components write `YYYY/MM/DD HH:MM:SS [LEVEL] message` (no fractional seconds, so older clients can still parse `core.log`).

---

## Runtime debugging and inspection

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

## See also

- [Log Viewer & Diagnostics](/usage/logs)
- [Doctor Diagnostics](/reference/doctor)
- [Testing](/developers/testing)
