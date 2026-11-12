---
title: "Official GNOME Shell Extension"
description: "Install and use the GNOME Shell extension for WhoisThat."
audience: "user"
type: "how-to"
---

# Official GNOME Shell Extension

WhoisThat features an official, first-class desktop extension for the GNOME desktop environment: **WhoisThat VPN Indicator**.

---

## Extension Overview

The GNOME Shell Extension integrates WhoisThat directly into your top panel:

* **Top Bar Live Indicator**: Displays a status icon/dot, active connection mode badge (`TUN` or `Proxy`), and live transfer speeds (`↓ 1.4 MB/s ↑ 210.0 KB/s`).
* **Quick Control Dropdown**:
  * One-click Connect / Disconnect toggle.
  * One-click TUN mode toggle.
  * Profile selector organized by subscription groups.
  * Direct launcher button to spawn the interactive `whoisthat` terminal TUI.
* **Panel Customization**: Relocate the indicator across panel positions (**Right**, **Center**, or **Left**), configure visible labels, and select your preferred terminal emulator (Auto-detect, Alacritty, GNOME Terminal, Ptyxis, Kitty, Ghostty, XTerm).

---

## Installation

::: warning Not on extensions.gnome.org yet
The extension is not currently listed in the GNOME Extensions catalog — the formerly documented `/extension/10998/` page returns 404. Install it from source using the steps below.
:::

### Method A: From Source (Recommended)

```bash
git clone https://github.com/kvunoff/whoisthat-extension.git
cd whoisthat-extension
make install
```

### Method B: Manual `.zip` Build

```bash
git clone https://github.com/kvunoff/whoisthat-extension.git
cd whoisthat-extension
make pack
gnome-extensions install --force whoisthat-indicator@kvunoff.shell-extension.zip
```

After installing, log out and back in (X11) or restart GNOME Shell (Wayland), then enable the extension:

```bash
gnome-extensions enable whoisthat-indicator@kvunoff
```

## Requirements

- GNOME Shell with the version range declared in the extension's `metadata.json`.
- `whoisthat` in `PATH`; the extension talks to the same core/socket as the TUI.
- A running core, or an autostart backend so the extension can start it.
- The `gnome-extensions` CLI (part of GNOME Shell) for install/enable/remove.

The extension does not ship its own daemon. If the core is offline, the panel
shows the offline state; connection actions start it through the normal CLI
path.

## Disable or remove

```bash
gnome-extensions disable whoisthat-indicator@kvunoff
gnome-extensions uninstall whoisthat-indicator@kvunoff
```

Removing the extension does not stop the core or change any WhoisThat settings.

## Troubleshooting

- **Indicator missing after install**: log out and back in on X11, or restart
  GNOME Shell on Wayland; then check `gnome-extensions list`.
- **Indicator shows offline**: start the TUI once, or enable an autostart
  backend, and run `whoisthat doctor`.
- **Connect button does nothing**: verify `whoisthat status --json` works in a
  terminal; the extension uses the same CLI.
- **Wrong terminal opens**: set `terminal` in `config.toml` or through
  `whoisthat settings set terminal=...`.

---

## Extension Source & Contributions

The extension is open-source and maintained on GitHub:
* 🔗 Repository: [github.com/kvunoff/whoisthat-extension](https://github.com/kvunoff/whoisthat-extension)
