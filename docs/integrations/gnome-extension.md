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
* **Panel Customization**: Relocate the indicator across panel positions (**Right**, **Center**, or **Left**), configure visible labels, and select your preferred terminal emulator (Alacritty, Kitty, Foot, WezTerm, GNOME Terminal).

---

## Installation

### Method A: GNOME Extensions Website (Recommended)

1. Ensure you have the `gnome-browser-connector` package installed on your system.
2. Open the official extension page:
   👉 **[WhoisThat VPN Indicator on extensions.gnome.org](https://extensions.gnome.org/extension/10998/whoisthat-vpn-indicator/)**
3. Flip the toggle switch to **ON** and accept the installation prompt.

### Method B: Manual Source Installation

```bash
git clone https://github.com/kvunoff/whoisthat-extension.git
cd whoisthat-extension
make install
```

After installing, enable the extension:

```bash
gnome-extensions enable whoisthat-vpn-indicator@kvunoff.github.io
```

---

## Extension Source & Contributions

The extension is open-source and maintained on GitHub:
* 🔗 Repository: [github.com/kvunoff/whoisthat-extension](https://github.com/kvunoff/whoisthat-extension)
