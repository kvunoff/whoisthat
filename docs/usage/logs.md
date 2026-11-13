---
title: "Log Viewer & Diagnostics"
description: "Use the log viewer and locate, filter, and rotate WhoisThat logs."
audience: "user"
type: "reference"
---

# Log Viewer & Diagnostics

The Logs tab (<kbd>4</kbd>, or cycle with <kbd>[</kbd> / <kbd>]</kbd>) provides a live stream of daemon events, routing decisions, and diagnostic warnings directly inside the TUI.

From a terminal:

```bash
whoisthat logs --follow
whoisthat logs --level error
whoisthat logs --json
whoisthat logs --source tui
whoisthat logs --since 1h --grep timeout
whoisthat logs clear --yes
```

---

## Log Viewer Features

* **Sources**: the tab shows the core log by default. Press <kbd>s</kbd> to switch to the TUI log and back. Both files use the same line format. From the CLI, `--source core|tui` selects the file (`--tui` is still accepted as an alias).
* **Search (<kbd>/</kbd>)**: filters the current file by a case-insensitive substring while you type. <kbd>Enter</kbd> keeps the filter, <kbd>Esc</kbd> clears it. Search combines with the level filter and survives a source switch.
* **Clear (<kbd>c</kbd>)**: after confirmation, truncates the current source and removes its rotated backup (and `core-crash.log` when the source is core).
* **Live Auto-Scroll**: as new lines arrive the viewer tails the file; auto-scroll pauses when you scroll up and resumes at the bottom.
* **Scroll Pausing & Navigation**: <kbd>j</kbd>/<kbd>k</kbd> or <kbd>↑</kbd>/<kbd>↓</kbd> scroll line by line, <kbd>Ctrl+U</kbd>/<kbd>Ctrl+D</kbd> by half a page, <kbd>g</kbd> jumps to the top, <kbd>G</kbd> to the bottom.
* **Level Filtering (<kbd>f</kbd>)**: cycles
  `ALL → ERROR+ → WARN+ → INFO+ → DEBUG+ → TRACE+ → ALL`.
* **Visual Syntax Highlighting**:
  * <span style="color: #f43f5e; font-weight: bold;">[ERRO] / [FATA]</span>: red — failures that stop an operation or the daemon.
  * <span style="color: #fbbf24; font-weight: bold;">[WARN]</span>: yellow — degraded paths, failed probes, kill-switch not armed.
  * <span style="color: #22c55e; font-weight: bold;">[INFO]</span>: green — connection lifecycle, routing compilation, subscription updates.
  * <span style="color: #94a3b8; font-weight: bold;">[DEBU] / [TRAC]</span>: dim — verbose diagnostics (only when the level is `debug`/`trace`).

Lines whose level cannot be recognized are shown only under `ALL`.

---

## Line Format

Both components write:

```text
2026/10/31 13:37:11 [WARN] message
```

Older files with `12:00:00 [WARN] ...` (core) or `12:00:00 WARN  ...` (TUI) are still parsed by the viewer and by `whoisthat logs`.

---

## Log File Locations on Disk

| Log File | Component | Format & Rotation | Purpose |
| :--- | :--- | :--- | :--- |
| `~/.config/whoisthat/core.log` | Go Core Daemon | Text; lumberjack, 20 MB max, 2 backups | Primary daemon log: connection states, routing decisions, xray failures. |
| `$TMPDIR/whoisthat-<uid>/core.log` | Go Core Daemon | Text; same rotation | Fallback when the config directory exists but is not writable. |
| `~/.local/share/whoisthat/core-crash.log` | Go Core Daemon | Text, 0600; rotated at startup above 1 MB | Unhandled Go panics and fatal runtime errors (stderr is discarded when the TUI spawns the daemon). |
| `~/.local/share/whoisthat/tui.log` | Rust TUI | Text; rotated on write at 10 MiB, one `.1` backup, 0600 | TUI events: IPC, connection status, warnings/errors, panics. |
| `$TMPDIR/whoisthat-<uid>/tui.log` | Rust TUI | Text; same rotation | Fallback when the data directory is not writable. |

The TUI backup (`tui.log.1`) is removed at startup when older than a week. Files are `0600`; the per-user fallback directory is `0700`.

`core-crash.log` is rotated only at core startup, when it exceeds 1 MB. Crash output is written while the process is dying, so a live rotation cannot bound a dump that ends the process; the startup check bounds crash loops instead.

Xray stderr is no longer left in `/tmp`. When xray exits unexpectedly (including a rejected config), the last ~4 KB of its stderr are written to `core.log` as one `[ERRO]` line and the temporary file is deleted.

---

## Clearing Logs

```bash
whoisthat logs clear --yes                    # core.log and tui.log
whoisthat logs clear --source core --yes      # core.log, backups, core-crash.log
whoisthat logs clear --source tui --yes       # tui.log and tui.log.1
```

Active files are truncated in place instead of deleted, so the core and TUI keep writing to the same file without a restart. Rotated backups (`.1`, lumberjack's `core-YYYY-….log`) and, for `--source core`, `core-crash.log` are removed. The command requires `--yes`; text output lists every file it touched (`truncated`, `removed`, `failed`) and `--json` also reports `missing` entries.

A log owned by another user (for example a core started with `sudo`) cannot be cleared by an unprivileged client; the command reports `failed: permission denied` and exits `1`. Clearing `core.log` from a terminal while the core is running may make lumberjack rotate once, because its in-memory size counter is not reset by an external truncate; no records are lost.

Clearing `core-crash.log` destroys the only post-mortem record of a daemon crash — inspect it first (`whoisthat doctor --logs` summarizes it).

---

## Verbosity

The default depends on who started the process: the TUI log defaults to `warn`
(capped by **Verbose TUI log**), a TUI-spawned core starts at `warn`, and a
standalone core starts at `info` unless `WHOISTHAT_LOG_LEVEL` is set.

| Level | What appears |
| :--- | :--- |
| `error` | `[ERRO]` / `[FATA]` |
| `warn` | + `[WARN]` (TUI default) |
| `info` | + `[INFO]` |
| `debug` | + `[DEBU]` (the core treats `trace` as `debug`) |
| `trace` | + `[TRAC]` (TUI only) |

The TUI's **Verbose TUI log** toggle only affects the TUI file: with `off` the level is capped at `warn` (errors always, warnings unless Log Level is `error`; `info`/`debug` are not written). The core level changes take effect immediately over IPC; a standalone daemon launched outside the TUI uses `WHOISTHAT_LOG_LEVEL` as its startup value (default `info`).

For a one-off session, `RUST_LOG=debug whoisthat` or `WHOISTHAT_LOG_LEVEL=debug whoisthat` turns the TUI file log on at that level and applies the same level to the core. `RUST_LOG` wins; a bare level or `whoisthat=debug` is accepted (module filters are ignored). The override is not written back to `config.toml`, and changing the level in Settings afterwards takes over for the rest of the session.

---

## Inspecting Logs via Journald

When running WhoisThat Core as a systemd user service, stdout/stderr are captured by `systemd-journald`. Go crash reports do **not** go to journald: `debug.SetCrashOutput` writes them to `core-crash.log` (when that file can be opened).

```bash
journalctl --user -u whoisthat-core -f
journalctl --user -u whoisthat-core -n 100
journalctl --user -u whoisthat-core --since today
```

The daemon's regular log records go to `core.log`; journald only sees stdout/stderr and crash output. Check `core.log` first for connection and routing events.
