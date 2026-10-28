# Log Viewer & Diagnostics

The Logs tab (<kbd>4</kbd>, or cycle with <kbd>[</kbd> / <kbd>]</kbd>) provides a live stream of daemon and proxy events, routing decisions, and diagnostic warnings directly inside the TUI.

---

## Log Viewer Features

* **Live Auto-Scroll**: As new log events arrive from `whoisthat-core`, the viewer automatically tails the stream to the newest entry.
* **Scroll Pausing & Navigation**: Press <kbd>k</kbd> / <kbd>↑</kbd> or <kbd>Ctrl+U</kbd> to scroll upwards and inspect previous logs. Auto-scroll pauses while reading history and resumes when returning to the bottom (<kbd>G</kbd> / <kbd>End</kbd>). Press <kbd>g</kbd> / <kbd>Home</kbd> to jump to the top, and <kbd>Ctrl+D</kbd> / <kbd>Ctrl+U</kbd> for half-page navigation.
* **Level Filtering (<kbd>f</kbd>)**: Press <kbd>f</kbd> to cycle through log levels:
  $$\text{ALL} \longrightarrow \text{DEBUG} \longrightarrow \text{INFO} \longrightarrow \text{WARN} \longrightarrow \text{ERROR} \longrightarrow \text{ALL}$$
* **Visual Syntax Highlighting**:
  * <span style="color: #f43f5e; font-weight: bold;">[ERRO]</span>: Red highlight for critical failures, dial errors, or missing capabilities.
  * <span style="color: #fbbf24; font-weight: bold;">[WARN]</span>: Yellow highlight for throttling, test timeouts, or runtime asset downloads.
  * <span style="color: #38bdf8; font-weight: bold;">[INFO]</span>: Cyan highlight for connection lifecycle events, routing compilations, and subscription updates.

---

## Log File Locations on Disk

WhoisThat writes structured logs to designated disk locations based on component:

| Log File | Component | Format & Rotation | Purpose |
| :--- | :--- | :--- | :--- |
| `~/.config/whoisthat/core.log` | Go Core Daemon | Lumberjack rotation (20 MB max, 1 backup) | Primary daemon log: connection states, Xray output, routing tables. |
| `/tmp/whoisthat-core.log` | Go Core Daemon | Plain text | Emergency fallback if `~/.config/` is read-only or inaccessible. |
| `~/.local/share/whoisthat/tui.log` | Rust TUI | Plain text | TUI debug log (active when **TUI Log** is enabled in Settings). |
| `/tmp/whoisthat-tui.log` | Rust TUI | Plain text | Emergency fallback if `~/.local/share/whoisthat/` is not writable. |

---

## Inspecting Logs via Journald

When running WhoisThat Core as a systemd user service, all standard output and error streams are captured by `systemd-journald`:

```bash
# Follow live daemon logs
journalctl --user -u whoisthat-core -f

# View last 100 entries
journalctl --user -u whoisthat-core -n 100

# Inspect logs since today
journalctl --user -u whoisthat-core --since today
```
