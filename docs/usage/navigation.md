# Navigation & Keybindings Cheat Sheet

WhoisThat features a high-speed, keyboard-driven interface with vim-inspired motions, instant modal switchers, and mouse support for lists, the top-bar actions and the Tab Switcher.

Routing and testing are also scriptable:

```bash
whoisthat route list --group Work
whoisthat route add --type geosite --value geosite:openai --outbound proxy
whoisthat test group Work --wait
```

---

## Global Navigation Keys

| Key | Alternative | Action |
| :---: | :---: | :--- |
| <kbd>[</kbd> / <kbd>]</kbd> | — | Switch to previous / next tab without popups |
| <kbd>1</kbd> – <kbd>5</kbd> | — | Jump directly to tab 1..5 |
| <kbd>Tab</kbd> | <kbd>BackTab</kbd> | Toggle focus between Tree and Details panels |
| <kbd>w</kbd> | — | Open compact **Tab Switcher** modal |
| <kbd>j</kbd> | <kbd>↓</kbd> | Move cursor down |
| <kbd>k</kbd> | <kbd>↑</kbd> | Move cursor up |
| <kbd>Ctrl+D</kbd> / <kbd>Ctrl+U</kbd> | <kbd>PgDn</kbd> / <kbd>PgUp</kbd> | Page down / page up (`Ctrl+D`/`Ctrl+U` in the Details and Logs panes, `PgDn`/`PgUp` in the profile tree) |
| <kbd>g</kbd> | — | Jump to the first item in the active view |
| <kbd>G</kbd> | — | Jump to the last item in the active view |
| <kbd>/</kbd> | — | Open live search / filter bar (filter by name, protocol, host, address) |
| <kbd>Esc</kbd> | — | Clear search, close modal popup, or return focus from Details to Tree |
| <kbd>v</kbd> | — | Toggle between TUN mode (system-wide) and Proxy mode |
| <kbd>d</kbd> | — | Disconnect active VPN session |
| <kbd>q</kbd> | — | **Detach** TUI (leaves VPN running in background) |
| <kbd>Q</kbd> | <kbd>Ctrl+C</kbd> | **Quit** completely (terminates VPN tunnel and exits daemon) |
| <kbd>?</kbd> | — | Open interactive help dialog |

---

## Connection & Latency Testing

| Key | Context | Action |
| :---: | :--- | :--- |
| <kbd>c</kbd> / <kbd>Enter</kbd> | Profile Item | Connect to highlighted server profile |
| <kbd>d</kbd> | Any View | Disconnect active VPN session |
| <kbd>v</kbd> | Any View | Toggle between TUN mode (system-wide) and Proxy mode |
| <kbd>t</kbd> | Profiles Tab | **Test Focused**: Test highlighted profile or subscription group |
| <kbd>T</kbd> | Profiles Tab | **Test All**: queue every profile in the tree (deduplicated by group/profile ID) |
| <kbd>C</kbd> | Any View | **Cancel Tests**: drop queued tests; a sample already in flight finishes or times out |

---

## Profile & Group Management (Tab 1)

| Key | Context | Action |
| :---: | :--- | :--- |
| <kbd>Space</kbd> / <kbd>Enter</kbd> | Group Header | Fold or unfold subscription group (collapsible tree) |
| <kbd>h</kbd> / <kbd>←</kbd> | Group Header | Collapse group, or jump from child profile to parent group |
| <kbd>l</kbd> / <kbd>→</kbd> | Group Header | Expand group, or shift focus to the Details pane |
| <kbd>Tab</kbd> | Profiles View | **Switch Focus**: Toggle focus between Tree and Details pane |
| <kbd>J</kbd> / <kbd>Alt+↓</kbd> / <kbd>Shift+↓</kbd> | Profile / Group | **Move Down**: Reorder profile or group downwards (instant & persistent) |
| <kbd>K</kbd> / <kbd>Alt+↑</kbd> / <kbd>Shift+↑</kbd> | Profile / Group | **Move Up**: Reorder profile or group upwards (instant & persistent) |
| <kbd>A</kbd> / <kbd>n</kbd> | Profiles Tab | **Add Group**: Create a new subscription group by URL or name |
| <kbd>u</kbd> / <kbd>r</kbd> | Group Header | **Update Subscription**: Fetch fresh profiles from subscription URL |
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
| <kbd>h</kbd> / <kbd>l</kbd> / <kbd>←</kbd> / <kbd>→</kbd> | Routing Tab | Switch pages: Global rules and per-group routing |
| <kbd>g</kbd> / <kbd>G</kbd> | Group Page | Toggle between Global and the group's custom rules |
| <kbd>H</kbd> | Routing Tab | Open **Happ Routing Profiles** modal (<kbd>Enter</kbd> activate, <kbd>o</kbd> deactivate, <kbd>d</kbd>/<kbd>x</kbd> delete, <kbd>m</kbd> cycle group routing mode) |
| <kbd>m</kbd> | Routing Tab | Cycle group routing mode (`auto` → `manual` → `subscription` → `auto`) |
| <kbd>i</kbd> | Routing Tab | Import Happ routing profile link (`happ://routing/...`) from clipboard |
| <kbd>p</kbd> / <kbd>P</kbd> | Routing Tab | Open the **Presets Catalog** (11 presets: Bypass Russia/LAN/China, Block Ads, Media, Telegram, AI, Socials, torrent block/direct, Steam) |
| <kbd>a</kbd> | Routing Tab | Add new manual routing rule |
| <kbd>Space</kbd> | Rule Item | **Toggle Rule**: Enable/disable rule with instant live hot-reload |
| <kbd>e</kbd> / <kbd>Enter</kbd> | Rule Item | Edit selected rule parameters |
| <kbd>x</kbd> / <kbd>Delete</kbd> | Rule Item | Delete selected routing rule |

---

## Tab Switching Shortcuts

| Shortcut | Destination Tab | Key Highlights |
| :---: | :--- | :--- |
| <kbd>[</kbd> / <kbd>]</kbd> | **Prev / Next** | Cycle tabs horizontally without modal interruptions |
| <kbd>1</kbd> | **Profiles** | Tree navigation, details panel, latency indicators |
| <kbd>2</kbd> | **Routing** | Rule editor, presets catalog, Happ profiles modal |
| <kbd>3</kbd> | **Traffic** | Real-time Braille bandwidth monitor (RX green, TX red) |
| <kbd>4</kbd> | **Logs** | Live core log tail with level filter (<kbd>f</kbd>) |
| <kbd>5</kbd> | **Settings** | Configuration editor for themes, autostart, TUN name, kill-switch, split tunnel, logging, latency tests, HWID and User-Agent |

---

## Full Mouse Navigation

* **Click Profile / Group**: Selects and highlights the item.
* **Double Click Profile**: Connects immediately.
* **Double Click / Click Group Header**: Folds or unfolds the group tree.
* **Bottom Status Bar**: Clicking the `TUN`/mode area toggles TUN mode on or off.
* **Top Bar Right Side**: Opens the Tab Switcher; the far-right help and quit areas open Help or exit.
* **Tab Switcher / Presets Modals**: Clickable list items. Other form popups are keyboard-only (Tab to move between fields).
