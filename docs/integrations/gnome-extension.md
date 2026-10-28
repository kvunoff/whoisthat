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

---

## Extension Source & Contributions

The extension is open-source and maintained on GitHub:
* 🔗 Repository: [github.com/kvunoff/whoisthat-extension](https://github.com/kvunoff/whoisthat-extension)
