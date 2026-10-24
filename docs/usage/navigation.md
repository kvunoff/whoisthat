# Navigation & Keybindings Cheat Sheet

WhoisThat features a high-speed, keyboard-driven interface with full mouse support, vim-inspired motions, and instant modal switchers.

---

## Global Navigation Keys

| Key | Alternative | Action |
| :---: | :---: | :--- |
| <kbd>j</kbd> | <kbd>↓</kbd> | Move cursor down |
| <kbd>k</kbd> | <kbd>↑</kbd> | Move cursor up |
| <kbd>g</kbd> | <kbd>Home</kbd> | Jump to first item in the active view |
| <kbd>G</kbd> | <kbd>End</kbd> | Jump to last item in the active view |
| <kbd>Tab</kbd> | — | Open compact **Tab Switcher** modal (or toggle Tree ↔ Details in single-panel mode) |
| <kbd>/</kbd> | — | Open live search / filter bar (filter by name, protocol, host, address) |
| <kbd>Esc</kbd> | — | Clear search, close modal popup, or return focus to tree |
| <kbd>q</kbd> | — | **Detach** TUI (leaves VPN running in background) |
| <kbd>Q</kbd> | <kbd>Ctrl+C</kbd> | **Quit** completely (terminates VPN tunnel and exits daemon) |
| <kbd>?</kbd> | <kbd>h</kbd> | Open context-sensitive help dialog |

---

## Connection & Latency Testing

| Key | Context | Action |
| :---: | :--- | :--- |
| <kbd>c</kbd> / <kbd>Enter</kbd> | Profile Item | Connect to highlighted server profile |
| <kbd>d</kbd> | Any View | Disconnect active VPN session |
| <kbd>v</kbd> | Any View | Toggle between TUN mode (system-wide) and Proxy mode |
| <kbd>t</kbd> | Profiles Tab | **Test All**: Scan all profiles top-to-bottom with deduplication |
| <kbd>T</kbd> | Profiles Tab | **Test Focused**: Test highlighted profile or subscription group |
| <kbd>C</kbd> | Any View | **Cancel Tests**: Abort in-flight latency scans and drop queued tests |

---

## Profile & Group Management (Tab 1)

| Key | Context | Action |
| :---: | :--- | :--- |
| <kbd>Space</kbd> / <kbd>Enter</kbd> | Group Header | Fold or unfold subscription group (collapsible tree) |
| <kbd>h</kbd> / <kbd>←</kbd> | Group Header | Collapse group, or jump from child profile to parent group |
| <kbd>l</kbd> / <kbd>→</kbd> | Group Header | Expand group, or shift focus to the Details pane |
| <kbd>J</kbd> / <kbd>Alt+↓</kbd> / <kbd>Shift+↓</kbd> | Profile / Group | **Move Down**: Reorder profile or group downwards (instant & persistent) |
| <kbd>K</kbd> / <kbd>Alt+↑</kbd> / <kbd>Shift+↑</kbd> | Profile / Group | **Move Up**: Reorder profile or group upwards (instant & persistent) |
| <kbd>U</kbd> | Profiles Tab | **Add Group**: Create a new subscription group by URL or name |
| <kbd>u</kbd> | Group Header | **Update Subscription**: Fetch fresh profiles from subscription URL |
| <kbd>a</kbd> | Profiles Tab | **Import Profile**: Paste proxy URI (`vless://`, `hysteria2://`, `trojan://`, etc.) |
| <kbd>e</kbd> | Profile / Group | **Edit / Rename**: Modify group URL/name or rename selected profile |
| <kbd>x</kbd> | Profile Item | **Delete Profile**: Delete selected server profile |
| <kbd>X</kbd> | Group Header | **Delete Group**: Delete entire subscription group and all its profiles |
| <kbd>y</kbd> | Profile Item | **Copy URI**: Copy raw proxy URI to system clipboard |
| <kbd>Ctrl+V</kbd> | Input Popups | Paste text from system clipboard |

---

## Routing Tab Shortcuts (Tab 2)

| Key | Context | Action |
| :---: | :--- | :--- |
| <kbd>←</kbd> / <kbd>→</kbd> | Routing Tab | Switch selected group for group-specific routing policies |
| <kbd>H</kbd> / <kbd>h</kbd> | Routing Tab | Open **Happ Routing Profiles** modal (view, activate, deactivate <kbd>o</kbd>, delete <kbd>d</kbd>/<kbd>x</kbd>) |
| <kbd>m</kbd> | Routing Tab | Cycle group routing mode (`auto` → `manual` → `subscription` → `auto`) |
| <kbd>i</kbd> | Routing Tab | Import Happ routing profile link (`happ://routing/...`) from clipboard |
| <kbd>p</kbd> / <kbd>P</kbd> | Routing Tab | Open **Presets Catalog** (Bypass RU/LAN, Block Ads, Media, Telegram, AI, Socials) |
| <kbd>a</kbd> | Routing Tab | Add new manual routing rule |
| <kbd>Space</kbd> | Rule Item | **Toggle Rule**: Enable/disable rule with instant live hot-reload |
| <kbd>e</kbd> / <kbd>Enter</kbd> | Rule Item | Edit selected rule parameters |
| <kbd>x</kbd> / <kbd>Delete</kbd> | Rule Item | Delete selected routing rule |

---

## Tab Switching Shortcuts

| Shortcut | Destination Tab | Key Highlights |
| :---: | :--- | :--- |
| <kbd>1</kbd> | **Profiles** | Tree navigation, details panel, latency indicators |
| <kbd>2</kbd> / <kbd>r</kbd> | **Routing** | Rule editor, presets catalog, Happ profiles modal |
| <kbd>3</kbd> / <kbd>m</kbd> | **Traffic** | Real-time Braille bandwidth monitor (RX green, TX red) |
| <kbd>4</kbd> / <kbd>l</kbd> | **Logs** | Live core & TUI log tail with level filter (<kbd>f</kbd>) |
| <kbd>5</kbd> / <kbd>s</kbd> | **Settings** | Configuration editor for themes, TUN, HWID, ports |

---

## Full Mouse Navigation

* **Click Profile / Group**: Selects and highlights item.
* **Double Click Profile**: Connects immediately.
* **Double Click Group Header**: Folds or unfolds the group tree.
* **Bottom Status Bar Click**: Clicking `TUN` toggles TUN mode on/off.
* **Header Tabs Click**: Click tab headers to switch between views.
* **Popups & Dialogs**: Click buttons (<kbd>Cancel</kbd>, <kbd>Confirm</kbd>) or form fields to focus.
