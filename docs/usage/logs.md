# Log Viewer & Diagnostics

The Logs tab (<kbd>4</kbd>, or cycle with <kbd>[</kbd> / <kbd>]</kbd>) provides a live stream of daemon events, routing decisions, and diagnostic warnings directly inside the TUI.

From a terminal:

```bash
whoisthat logs --follow
whoisthat logs --level error
whoisthat logs --json
whoisthat logs --tui
```

---

## Log Viewer Features

* **Sources**: the tab shows the core log by default. Press <kbd>s</kbd> to switch to the TUI log and back. Both files use the same line format.
* **Live Auto-Scroll**: as new lines arrive the viewer tails the file; auto-scroll pauses when you scroll up and resumes at the bottom.
* **Scroll Pausing & Navigation**: <kbd>j</kbd>/<kbd>k</kbd> or <kbd>↑</kbd>/<kbd>↓</kbd> scroll line by line, <kbd>Ctrl+U</kbd>/<kbd>Ctrl+D</kbd> by half a page, <kbd>g</kbd> jumps to the top, <kbd>G</kbd> to the bottom.
* **Level Filtering (<kbd>f</kbd>)**: cycles
  $$\text{ALL} \longrightarrow \text{ERROR+} \longrightarrow \text{WARN+} \longrightarrow \text{INFO+} \longrightarrow \text{DEBUG+} \longrightarrow \text{TRACE+} \longrightarrow \text{ALL}$$
* **Visual Syntax Highlighting**:
  * <span style="color: #f43f5e; font-weight: bold;">[ERRO] / [FATA]</span>: red — failures that stop an operation or the daemon.
  * <span style="color: #fbbf24; font-weight: bold;">[WARN]</span>: yellow — degraded paths, failed probes, kill-switch not armed.
  * <span style="color: #22c55e; font-weight: bold;">[INFO]</span>: green — connection lifecycle, routing compilation, subscription updates.
  * <span style="color: #94a3b8; font-weight: bold;">[DEBU] / [TRAC]</span>: dim — verbose diagnostics (only when the level is `debug`/`trace`).

Lines whose level cannot be recognized are shown only under `ALL`.

---

## Line Format

Both components write:

```
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

Xray stderr is no longer left in `/tmp`. When xray exits unexpectedly (including a rejected config), the last ~4 KB of its stderr are written to `core.log` as one `[ERRO]` line and the temporary file is deleted.

---

## Verbosity

| Level | What appears |
| :--- | :--- |
| `error` | `[ERRO]` / `[FATA]` |
| `warn` | + `[WARN]` (default) |
| `info` | + `[INFO]` |
| `debug` | + `[DEBU]` (the core treats `trace` as `debug`) |
| `trace` | + `[TRAC]` (TUI only) |

The TUI's **Verbose TUI log** toggle only affects the TUI file: with `off` the level is capped at `warn` (errors always, warnings unless Log Level is `error`; `info`/`debug` are not written). The core level changes take effect immediately over IPC; `WHOISTHAT_LOG_LEVEL` is used only as the startup value for a daemon launched outside the TUI.

---

## Inspecting Logs via Journald

When running WhoisThat Core as a systemd user service, stdout/stderr and the Go crash output are captured by `systemd-journald`:

```bash
journalctl --user -u whoisthat-core -f
journalctl --user -u whoisthat-core -n 100
journalctl --user -u whoisthat-core --since today
```

The daemon's regular log records go to `core.log`; journald only sees stdout/stderr and crash output. Check `core.log` first for connection and routing events.
