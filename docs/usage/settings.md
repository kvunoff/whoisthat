# Settings Reference

The Settings tab (<kbd>5</kbd> or cycle with <kbd>[</kbd> / <kbd>]</kbd>) provides an interactive control panel for configuring all runtime behaviors of WhoisThat.

---

## Interactive Settings Table

| Setting | Allowed Values | Default | Description |
| :--- | :--- | :--- | :--- |
| **Theme** | 4 palettes | `graphite-dark` | Visual color palette with live preview (Graphite Dark, Graphite Light, Tokyo Night, Dracula). |
| **Show IP** | `on` / `off` | `on` | Display public IPv4/IPv6 in the top status bar; refreshed on startup and whenever the connection state changes. |
| **Autoconnect** | `on` / `off` | `off` | Automatically connect to the last used profile when the core daemon initializes. |
| **Autostart Mode** | `proxy` / `tun` | `proxy` | Connection mode applied when autoconnect triggers (`proxy` = local SOCKS5/HTTP, `tun` = full system VPN). |
| **Systemd Autostart**| `on` / `off` | `off` | Enables `whoisthat-core.service` user unit and configures user lingering to run at boot before login. |
| **TUI Log** | `on` / `off` | `off` | Enables verbose debug logging for the Rust TUI (`~/.local/share/whoisthat/tui.log`). |
| **Log Level** | `error`, `warn`, `info`, `debug`, `trace` | `warn` | TUI file-log threshold. The same value is passed to the core as `WHOISTHAT_LOG_LEVEL` when it is spawned; the core logger only supports `error`/`warn`/`info` (`debug` and `trace` map to `info`), and changing it does not reconfigure an already running daemon. |
| **Test Method** | `tcp`, `http-get`, `http-head` | `http-get` | Latency testing strategy. `http-get` routes via SOCKS5 proxy to measure real round-trip times. |
| **Test Samples** | `1`, `3`, `5`, `10` | `3` | Number of sequential requests per test; the median round-trip time is recorded. |
| **Test Concurrency**| `4`, `8`, `16`, `32`, `64` | `16` | Maximum simultaneous test subprocesses executed in parallel. |
| **Test Timeout** | `3s`, `5s`, `10s`, `15s` | `5s` | Maximum duration before an individual test sample is marked as failed. |
| **Test Endpoint** | `cloudflare`, `gstatic`, `bing` | `cloudflare` | Target URL used for SOCKS5 HTTP latency tests. |
| **Auto-test on refresh** | `on` / `off` | `on` | Automatically triggers batch latency tests after a subscription refresh (<kbd>u</kbd> or <kbd>r</kbd>). Groups larger than 50 profiles are skipped to avoid a test storm. |
| **TUN Name** | Editable text | `whoisthattun` | Interface name for the virtual adapter: must start with a letter and contain only letters, digits, `_` or `-` (max 15 characters). |
| **Kill Switch** | `on` / `off` | `off` | Installs an independent firewall block to halt non-VPN traffic if the connection drops. |
| **Split Tunnel** | `off`, `exclude`, `include` | `off` | cgroup v2 application isolation mode. `exclude` = target apps bypass VPN; `include` = only target apps use VPN. |
| **HWID Enabled** | `on` / `off` | `on` | Send hardware identification headers (`x-hwid`) during subscription fetches. |
| **HWID** | Read-only hex | Auto | 16-character hexadecimal device fingerprint. |
| **Reset HWID** | Action button | — | Generates a new random hardware identifier. |
| **User-Agent** | Editable text | `whoisthat-core/v0.3.5` | Custom HTTP User-Agent header sent with subscription requests. |

---

## Modifying Values in the TUI

* **Navigation**: Press <kbd>j</kbd> / <kbd>k</kbd> (or <kbd>↓</kbd> / <kbd>↑</kbd>) to move between settings.
* **Toggle / Cycle**: Press <kbd>Enter</kbd> or <kbd>Space</kbd> to activate the highlighted row (toggle a boolean, cycle a choice, open a text editor, or run the **Reset HWID** action). On the **Theme** row, <kbd>h</kbd> / <kbd>l</kbd> (and <kbd>←</kbd> / <kbd>→</kbd>) cycle the palette directly; other rows ignore <kbd>h</kbd> / <kbd>l</kbd>.
* **Text Editing**: Press <kbd>Enter</kbd> on editable fields (e.g. **TUN Name**, **User-Agent**) to summon an in-place text editor modal.
* **Instant Persistence**: Any modification made in the Settings tab is saved immediately to `~/.config/whoisthat/config.toml` (TUI options) and synchronized over IPC to `~/.config/whoisthat/config.json` (Core options).
