# Settings Reference

The Settings tab (<kbd>5</kbd> or cycle with <kbd>[</kbd> / <kbd>]</kbd>) provides an interactive control panel for configuring all runtime behaviors of WhoisThat.

---

## Interactive Settings Table

| Setting | Allowed Values | Default | Description |
| :--- | :--- | :--- | :--- |
| **Theme** | 4 palettes | `graphite-dark` | Visual color palette with live preview (Graphite Dark, Graphite Light, Tokyo Night, Dracula). |
| **Show IP** | `on` / `off` | `on` | Display public IPv4/IPv6 address in the top status bar (refreshed periodically and on state change). |
| **Autoconnect** | `on` / `off` | `off` | Automatically connect to the last used profile when the core daemon initializes. |
| **Autostart Mode** | `proxy` / `tun` | `proxy` | Connection mode applied when autoconnect triggers (`proxy` = local SOCKS5/HTTP, `tun` = full system VPN). |
| **Systemd Autostart**| `on` / `off` | `off` | Enables `whoisthat-core.service` user unit and configures user lingering to run at boot before login. |
| **TUI Log** | `on` / `off` | `off` | Enables verbose debug logging for the Rust TUI (`~/.local/share/whoisthat/tui.log`). |
| **Log Level** | `error`, `warn`, `info`, `debug`, `trace` | `warn` | Minimum log severity threshold for both the TUI and Core daemon. |
| **Test Method** | `tcp`, `http-get`, `http-head` | `http-get` | Latency testing strategy. `http-get` routes via SOCKS5 proxy to measure real round-trip times. |
| **Test Samples** | `1`, `3`, `5`, `10` | `3` | Number of sequential requests per test; the median round-trip time is recorded. |
| **Test Concurrency**| `4`, `8`, `16`, `32`, `64` | `16` | Maximum simultaneous test subprocesses executed in parallel. |
| **Test Timeout** | `3s`, `5s`, `10s`, `15s` | `5s` | Maximum duration before an individual test sample is marked as failed. |
| **Test Endpoint** | `cloudflare`, `gstatic`, `bing` | `cloudflare` | Target URL used for SOCKS5 HTTP latency tests. |
| **Auto-test on refresh** | `on` / `off` | `on` | Automatically triggers batch latency tests whenever a subscription group is refreshed (<kbd>u</kbd> or <kbd>r</kbd>). |
| **TUN Name** | Editable text | `whoisthattun` | Interface name assigned to the virtual network adapter (1–15 characters). |
| **Kill Switch** | `on` / `off` | `off` | Installs an independent firewall block to halt non-VPN traffic if the connection drops. |
| **Split Tunnel** | `off`, `exclude`, `include` | `off` | cgroup v2 application isolation mode. `exclude` = target apps bypass VPN; `include` = only target apps use VPN. |
| **HWID Enabled** | `on` / `off` | `on` | Send hardware identification headers (`x-hwid`) during subscription fetches. |
| **HWID** | Read-only hex | Auto | 16-character hexadecimal device fingerprint. |
| **Reset HWID** | Action button | — | Generates a new random hardware identifier. |
| **User-Agent** | Editable text | `whoisthat-core/v0.2.3` | Custom HTTP User-Agent header sent with subscription requests. |

---

## Modifying Values in the TUI

* **Navigation**: Press <kbd>j</kbd> / <kbd>k</kbd> (or <kbd>↓</kbd> / <kbd>↑</kbd>) to move between settings. Use <kbd>Ctrl+D</kbd> / <kbd>Ctrl+U</kbd> for half-page navigation, and <kbd>g</kbd> / <kbd>G</kbd> to jump to the top or bottom.
* **Toggle / Cycle**: Press <kbd>Enter</kbd>, <kbd>Space</kbd>, or <kbd>h</kbd> / <kbd>l</kbd> (<kbd>←</kbd> / <kbd>→</kbd>) on a choice or boolean field to cycle values.
* **Text Editing**: Press <kbd>Enter</kbd> on editable fields (e.g. **TUN Name**, **User-Agent**) to summon an in-place text editor modal.
* **Instant Persistence**: Any modification made in the Settings tab is saved immediately to `~/.config/whoisthat/config.toml` (TUI options) and synchronized over IPC to `~/.config/whoisthat/config.json` (Core options).
