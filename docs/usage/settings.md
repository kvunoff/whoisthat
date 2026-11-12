---
title: "Settings Reference"
description: "Every row in the WhoisThat Settings screen, with defaults and CLI equivalents."
audience: "user"
type: "reference"
---

# Settings Reference

The Settings tab (<kbd>5</kbd> or cycle with <kbd>[</kbd> / <kbd>]</kbd>) provides an interactive control panel for configuring all runtime behaviors of WhoisThat.

The same values are available from the terminal:

```bash
whoisthat settings list
whoisthat settings set split-tunnel=exclude kill-switch=true
whoisthat settings set theme=tokyo-night log-level=debug
whoisthat settings set terminal=alacritty     # app-menu launcher only
```

`terminal` controls which emulator the desktop launcher opens; it is not exposed
in the interactive Settings tab (set it from the CLI or `config.toml`).

---

## Interactive Settings Table

| Setting | Allowed Values | Default | Description |
| :--- | :--- | :--- | :--- |
| **Theme** | 4 palettes | `graphite-dark` | Visual color palette with live preview (Graphite Dark, Graphite Light, Tokyo Night, Dracula). |
| **Show IP** | `on` / `off` | `on` | Display public IPv4/IPv6 in the top status bar; refreshed on startup and whenever the connection state changes. |
| **Autoconnect** | `on` / `off` | `off` | Automatically connect to the last used profile when the core daemon initializes. |
| **Autoconnect Mode** | `proxy` / `tun` | `proxy` | Connection mode applied when autoconnect triggers (`proxy` = local SOCKS5/HTTP, `tun` = full system VPN). |
| **Autostart** | `off`, `systemd`, `init`, `desktop` (available backends are cycled) | `off` | Enables the selected autostart backend. `systemd` uses a user unit plus lingering; `init` writes the per-user marker for the init service; `desktop` creates `~/.config/autostart/whoisthat-core.desktop`. |
| **Verbose TUI log** | `on` / `off` | `off` | `off` caps the TUI at `warn`: errors are always written, warnings unless Log Level is `error`; `on` writes the configured level (including `info`/`debug`). |
| **Log Level** | `error`, `warn`, `info`, `debug`, `trace` | `warn` | Threshold for both components. Applied to the TUI immediately and to the running core over IPC (`set-log-level`); `trace` is treated as `debug` by the core. When **Verbose TUI log** is off, the TUI caps this at `warn`. |
| **Test Method** | `tcp`, `http-get`, `http-head` | `http-get` | Latency testing strategy. `http-get` routes via SOCKS5 proxy to measure real round-trip times. |
| **Test Samples** | `1`, `3`, `5`, `10` | `3` | Number of sequential requests per test; the median round-trip time is recorded. |
| **Test Concurrency**| `4`, `8`, `16`, `32`, `64` | `16` | Maximum simultaneous test subprocesses executed in parallel. |
| **Test Timeout** | `3s`, `5s`, `10s`, `15s` | `5s` | Maximum duration before an individual test sample is marked as failed. |
| **Test Endpoint** | `cloudflare`, `gstatic`, `bing` | `cloudflare` | Target URL used for SOCKS5 HTTP latency tests. |
| **Auto-test on refresh** | `on` / `off` | `on` | Automatically triggers batch latency tests after a subscription refresh (<kbd>u</kbd> or <kbd>r</kbd>). Groups larger than 50 profiles are skipped to avoid a test storm. |
| **TUN Name** | Editable text | `whoisthattun` | Interface name for the virtual adapter: must start with a letter and contain only letters, digits, `_` or `-` (max 15 characters). |
| **Kill Switch** | `on` / `off` | `off` | Installs an independent firewall block to halt non-VPN traffic if the connection drops. |
| **Split Tunnel** | `off`, `exclude`, `include` | `off` | cgroup v2 application isolation mode. `exclude` = target apps bypass VPN; `include` = only target apps use VPN. |
| **HWID Enabled** | `on` / `off` | `on` | Send HWID headers (`x-hwid`, `x-device-os`, `x-ver-os`, `x-device-model`) during subscription fetches. |
| **HWID** | Read-only hex | Auto | 16-character hexadecimal device fingerprint. |
| **Reset HWID** | Action button | — | Generates a new random hardware identifier. |
| **User-Agent** | Editable text | `whoisthat/v0.5.4` | Custom HTTP User-Agent header sent with subscription requests. |
| **Check for updates** | `on` / `off` | `on` | Checks GitHub for a newer stable release at startup (at most once a day) and shows a popup with a copyable update command. |

`RUST_LOG` or `WHOISTHAT_LOG_LEVEL` (for example `WHOISTHAT_LOG_LEVEL=debug whoisthat`) overrides **Verbose TUI log** and **Log Level** for that session and is never written to `config.toml`; `RUST_LOG` wins when both are set.

::: warning Kill Switch and Split Tunnel include
`Kill Switch: on` and `Split Tunnel: include` cannot be active together. The
core rejects enabling either while the other is set and normalizes a hand-edited
configuration to `kill-switch: false`. Use `exclude` or `off` with the
kill-switch.
:::

---

## Modifying Values in the TUI

* **Navigation**: Press <kbd>j</kbd> / <kbd>k</kbd> (or <kbd>↓</kbd> / <kbd>↑</kbd>) to move between settings.
* **Toggle / Cycle**: Press <kbd>Enter</kbd> or <kbd>Space</kbd> to activate the highlighted row (toggle a boolean, cycle a choice, open a text editor, or run the **Reset HWID** action). On the **Theme** row, <kbd>h</kbd> / <kbd>l</kbd> (and <kbd>←</kbd> / <kbd>→</kbd>) cycle the palette directly; other rows ignore <kbd>h</kbd> / <kbd>l</kbd>.
* **Text Editing**: Press <kbd>Enter</kbd> on editable fields (e.g. **TUN Name**, **User-Agent**) to summon an in-place text editor modal.
* **Instant Persistence**: Any modification made in the Settings tab is saved immediately to `~/.config/whoisthat/config.toml` (TUI options) and synchronized over IPC to `~/.config/whoisthat/config.json` (Core options).

---

## Themes and responsive layout

WhoisThat includes four curated palettes designed for readability and contrast:

| Theme | Style | Accents |
| :--- | :--- | :--- |
| **Graphite Dark** *(default)* | Monochrome graphite with obsidian depth | Platinum silver, crisp white, emerald |
| **Graphite Light** | Minimalist paper and chalk | Ink black, charcoal, deep forest green |
| **Tokyo Night** | Deep midnight blue inspired by Tokyo at dusk | Neon cyan, soft sky blue, coral red |
| **Dracula** | Gothic contrast with deep purple tones | Neon pink, electric purple, spring green |

To switch themes: open Settings (<kbd>5</kbd>), highlight **Theme**, and press
<kbd>Enter</kbd>, <kbd>Space</kbd>, or <kbd>l</kbd>/<kbd>→</kbd> to cycle
forward; <kbd>h</kbd>/<kbd>←</kbd> cycles backward with live preview. The choice
is saved to `~/.config/whoisthat/config.toml`.

The layout engine adapts to the terminal size:

- **Compact (< 80 columns)**: stacked 60/40 tree/details when the content area is
  at least 18 rows; below that only the tree is shown. <kbd>Tab</kbd> switches
  focus.
- **Medium (80–125 columns)**: side-by-side 55/45 tree/details. <kbd>Tab</kbd>
  toggles focus.
- **Wide (126–160 columns)**: 45/55 split with full server parameters, TLS
  properties, and statistics.
- **UltraWide (> 160 columns)**: same 45/55 split with the widest panes.

Height tiers:

- **Tiny (< 16 rows)**: compressed header, auxiliary cards hidden.
- **Short (16–24 rows)**: standard layout with a compact footer.
- **Normal (25–42 rows)**: full layout with latency metrics, quota bars, and
  interface cards.
- **Tall (> 42 rows)**: expanded logs pane and 60-second full-height traffic
  charts.

Below **45 columns × 8 rows**, rendering is suspended and a minimum-size warning
is shown to prevent buffer overflow and visual artifacts.

## See also

- [Navigation & Keybindings](/usage/navigation)
- [Configuration Files Reference](/reference/configuration)
- [Log Viewer & Diagnostics](/usage/logs)
