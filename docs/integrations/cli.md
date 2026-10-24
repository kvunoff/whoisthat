# CLI Subcommands & Flags

WhoisThat features a rich command-line interface allowing complete control over the VPN engine without opening the interactive TUI. These commands are optimized for sub-millisecond execution, scripting, keyboard daemon hotkeys (e.g. `sxhkd`, `hyprland`, `i3`), and desktop status widgets.

---

## Command Reference Matrix

| Command | Shorthand | Description |
| :--- | :---: | :--- |
| `whoisthat --toggle` / `toggle` | `-t` | **Smart Toggle**: Connects to the last used profile if disconnected, or disconnects if active. |
| `whoisthat --start [target]` / `connect` | `-c [target]` | Connect to a profile by ID (e.g. `-c 2`), group:id (`-c 1:3`), or name substring (`-c "Frankfurt"`). Connects to last profile if omitted. |
| `whoisthat --stop` / `disconnect` | `-d` | Disconnect the active VPN session. |
| `whoisthat --mode-toggle` / `toggle-tun` | `-mt` | Toggle between system-wide **TUN mode** and local **Proxy mode**. |
| `whoisthat --mode-tun` / `--tun` | — | Force enable TUN mode. |
| `whoisthat --mode-proxy` / `--proxy` | — | Force enable Proxy mode (TUN disabled). |
| `whoisthat --systemd-toggle` | `-st` | Toggle systemd user service (`whoisthat-core.service`). |
| `whoisthat --systemd-on` / `--enable` | — | Enable and start systemd user service (auto-configures linger). |
| `whoisthat --systemd-off` / `--disable` | — | Disable and stop systemd user service. |
| `whoisthat --systemd-status` | — | Check if systemd user service is currently enabled or disabled. |
| `whoisthat --killswitch-toggle` | `-kt` | Toggle Kill-Switch firewall protection. |
| `whoisthat --killswitch-on` / `--off` | — | Explicitly enable or disable Kill-Switch protection. |
| `whoisthat status --short` | `status -s` | Single-line compact status string: `[Connected] [TUN] ↓ 2.4 MB/s ↑ 410.0 KB/s`. |
| `whoisthat status --json` | `status -j` | Single snapshot of active daemon state formatted as JSON. |
| `whoisthat status --watch --json` | `status -w -j` | Continuous live NDJSON stream (1 emission per second) for reactive desktop widgets. |
| `whoisthat --profiles [--json]` | `-p` | List all subscription groups and contained profiles. |
| `whoisthat --ip` | `ip` | Query and print public IPv4 and IPv6 addresses. |
| `whoisthat doctor [--json]` | — | Run system diagnostic suite checking binaries, capabilities, and sockets. |
| `whoisthat run [-d] <app> [args...]` | `-b`, `--detach` | Launch application inside split-tunnel cgroup slice (`-d` runs detached). |
| `whoisthat add-sub <url> [name]` | — | Add subscription URL headlessly; immediately fetches, parses, and latency-tests profiles. |
| `whoisthat handle-url <url> [--group <id>]` | `<url>` | Ingest `wit://` or `happ://` deep links from CLI or browser. |
| `whoisthat register-deeplink` | — | Register desktop `.desktop` file and `x-scheme-handler/wit` with `xdg-mime`. |
| `whoisthat --version` | `-v` | Display version and exit. |
| `whoisthat --help` | `-h` | Display help screen. |

---

## Practical Scripting Examples

### Assigning Global Hotkeys (e.g. `hyprland.conf`)

```ini
# Toggle VPN connection with Super+V
bind = $mainMod, V, exec, whoisthat -t

# Toggle TUN mode with Super+Shift+V
bind = $mainMod SHIFT, V, exec, whoisthat -mt

# Quick disconnect with Super+Escape
bind = $mainMod, Escape, exec, whoisthat -d
```

### Checking Public IP in Bash Scripts

```bash
#!/usr/bin/env bash
if whoisthat status -s | grep -q "Connected"; then
    echo "VPN is Active. Public IP: $(whoisthat --ip)"
else
    echo "VPN is Offline."
fi
```
