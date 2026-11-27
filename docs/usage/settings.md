---
title: "Settings Reference"
description: "How the Settings screen is organized, and how to find and change a setting."
audience: "user"
type: "reference"
---

# Settings Reference

The Settings tab (<kbd>5</kbd> or <kbd>[</kbd> / <kbd>]</kbd>) shows a tree of
sections on the left and the settings of the selected group on the right. Press
<kbd>/</kbd> to search every setting by name, section, or group; <kbd>Enter</kbd>
jumps to the match. On narrow terminals the two panels collapse into one list
with section headers. Keybindings and mouse actions are owned by
[Navigation & Keybindings](/usage/navigation).

The same values are available from the terminal:

```bash
whoisthat settings list
whoisthat settings set split-tunnel=exclude kill-switch=true
whoisthat settings set theme=tokyo-night log-level=debug
```

Defaults, allowed values, ranges and config keys are owned by
[Configuration Files Reference](/reference/configuration); CLI keys are owned by
the [CLI Reference](/reference/cli).

---

## Sections and groups

| Section | Group | Settings |
| :--- | :--- | :--- |
| Connection | Autoconnect | Autoconnect, Autoconnect mode |
| Appearance | Display | Theme, Show IP |
| Logging | Levels | Verbose TUI log, Log level |
| Profile tests | Engine | Test method, Test samples, Test concurrency, Test timeout, Test endpoint |
| Profile tests | Automation | Auto-test on refresh |
| Subscription & HWID | HWID | HWID enabled, HWID (read-only), Reset HWID (action) |
| Subscription & HWID | Headers | User-Agent |
| Startup & updates | Service | Autostart |
| Startup & updates | Updates | Check for updates |
| Network | Interface | TUN name, TUN MTU |
| Network | Leak protection | Kill Switch, Split tunnel |
| Network | DNS | DNS servers |
| Network | Anti-DPI masks | Fragment and Noise rows |

Autostart cycles through the backends detected on the host; the available
backends and their behavior are described in
[Platforms & Service Management](/guide/platforms).

Several rows cycle through fixed TUI presets: **Test samples** (`1`, `3`, `5`,
`10`), **Test concurrency** (`4`, `8`, `16`, `32`, `64`), **Test timeout**
(`3`, `5`, `10`, `15` seconds), **Test endpoint** (`cloudflare`, `gstatic`,
`bing`), and **Fragment: packets** (`tlshello`, `1-1`, `1-2`, `1-3`). The CLI
and `config.json` accept other valid values, including any valid `N-M` range.

The **Anti-DPI masks** group adds global Xray fragmentation and UDP noise. It
takes effect on the next connect; an active connection keeps its current masks.
Provider masks always win. The mask fields and their precedence are owned by
[Supported Protocols & Transports](/networking/protocols).

`RUST_LOG` or `WHOISTHAT_LOG_LEVEL` overrides the TUI logging rows for one
session without writing to `config.toml`.

::: warning Kill Switch and Split Tunnel include
`Kill Switch: on` and `Split Tunnel: include` cannot be active together. Use
`exclude` or `off` with the kill-switch.
:::

---

## Changing values

<kbd>Enter</kbd> or <kbd>Space</kbd> activates a row: it toggles a boolean,
cycles a choice, opens the text editor, or runs the **Reset HWID** action.
Editable rows are marked with `✎`. Changes are saved immediately to
`~/.config/whoisthat/config.toml` (TUI options) and synchronized over IPC to
`~/.config/whoisthat/config.json` (core options). The daemon is authoritative
for core values: the screen follows its replies rather than applying changes
optimistically.

If the running core predates a group of settings, its rows show
`unsupported (core)` and cannot be edited. Restart the core with the newer
binary to enable them.

## See also

- [Navigation & Keybindings](/usage/navigation)
- [Configuration Files Reference](/reference/configuration)
- [CLI Reference](/reference/cli)
- [Supported Protocols & Transports](/networking/protocols)
- [Platforms & Service Management](/guide/platforms)
