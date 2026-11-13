---
title: "Routing"
description: "Create manual routing rules, presets, and per-group overrides in the TUI."
audience: "user"
type: "how-to"
---

# Routing

Routing decides whether a connection goes direct, through the proxy, or is
blocked. This page covers the rule editor, presets, and per-group modes. For the
concepts and the packet lifecycle see [Routing Rules](/networking/routing-rules);
for the exact first-match order see
[Routing Precedence](/reference/routing-precedence).

## Global Rules and Per-Group Overrides

The Routing tab is paginated: page 0 is **Global**, followed by one page per subscription group. Use <kbd>←</kbd> / <kbd>→</kbd> (or <kbd>h</kbd> / <kbd>l</kbd>) to move between pages.

Manual rules have two layers:

* **Global** — the shared rule set shown on page 0. It is stored in `~/.local/share/whoisthat/db/routing.json` and is used by every group that inherits it.
* **Custom** — a group-specific rule set stored in `~/.local/share/whoisthat/db/groups/<GID>/routing/manual.json`.

A group's `manual_routing_mode` decides which layer applies:

| Value | Behavior |
| :--- | :--- |
| `inherit` (default, also stored as an empty value) | The group uses the Global rules. |
| `custom` | The group uses its own rule set instead of Global. |

Press <kbd>g</kbd> on a group page to switch between the two. Switching to **Custom** seeds the group with a copy of Global if no custom rules exist yet. Switching back to **Global** keeps the custom file on disk, so the rules return when Custom is enabled again.

Editing a rule (add / edit / delete / toggle / preset) on a group page that still inherits Global automatically creates the custom override, seeded from the stored custom rules if present, otherwise from Global.

How the manual layer combines with Happ profiles, `auto`/`manual`/`subscription` and JSON subscriptions is in [Routing Precedence](/reference/routing-precedence#effective-routing-source).

---

## Supported Rule Match Types

WhoisThat supports six distinct matching criteria in manual and imported rules:

| Type | Internal Field | Description & Syntax Example |
| :--- | :--- | :--- |
| **`domain`** | `rule.domain` | Matches full domain or subdomain suffix: `example.com`, `sub.domain.org`. |
| **`ip`** | `rule.ip` | Individual IP addresses or CIDR network blocks: `1.2.3.4`, `192.168.1.0/24`. |
| **`protocol`** | `rule.protocol` | Protocol signature matching: `bittorrent`, `tls`, `http`. |
| **`port`** | `rule.port` | Destination port or range: `443`, `80,443`, `8000-8080`. |
| **`geoip`** | `rule.ip` (`geoip:<country>`) | Country or geographic IP database lookup: `geoip:private`, `geoip:ru`, `geoip:us`. |
| **`geosite`** | `rule.domain` (`geosite:<category>`) | Curated domain lists from v2fly: `geosite:category-ads-all`, `geosite:youtube`, `geosite:telegram`. |

---

## The Presets Catalog (<kbd>p</kbd>)

Press <kbd>p</kbd> in the Routing tab to open the built-in **Routing Presets Catalog**. Each preset can be toggled on or off with a single keystroke:

```text
┌──────────────────────── Routing Presets Catalog ────────────────────────┐
│                                                                        │
│  [X] 🇷🇺 Bypass Russia       Direct access to Russian domains & IPs     │
│  [X] 🛡️ Block Ads & Trackers Drop ad servers and telemetry networks    │
│  [X] 🏠 Bypass LAN / Local   Direct access to private RFC1918 subnets   │
│  [X] 📺 Proxy Media          Route YouTube, Twitch, and Discord         │
│  [X] 💬 Proxy Telegram       Route official Telegram domains and CIDRs  │
│  [X] 🤖 Proxy AI Services    Route OpenAI and Anthropic / Claude        │
│  [X] 📸 Proxy Blocked Socials Route Twitter/X, Instagram, and Meta      │
│  [X] 🛑 Block BitTorrent     Drop P2P traffic via Xray blackhole        │
│  [X] ⚡ Direct BitTorrent     Send P2P traffic outside the VPN           │
│  [X] 🇨🇳 Bypass China         Direct access to Chinese sites (cn)        │
│  [X] 🎮 Proxy Gaming         Route Steam Community through proxy        │
│                                                                        │
│          [j/k or ↑/↓ Navigate • Enter/Space Toggle • Esc Close]        │
└────────────────────────────────────────────────────────────────────────┘
```

Each preset is additive: toggling it on ensures its rules exist and are enabled, toggling it off disables them. There are no separate Apply/Delete keys.

### Preset Definitions:

* **🇷🇺 Bypass Russia**: Directs `geosite:category-ru` and `geoip:ru` to `direct`.
* **🛡️ Block Ads & Trackers**: Directs `geosite:category-ads-all` to `block`.
* **🏠 Bypass LAN / Local**: Directs `geoip:private` to `direct`.
* **📺 Proxy Media**: Directs `geosite:youtube`, `geosite:twitch`, `geosite:discord` to `proxy`.
* **💬 Proxy Telegram**: Directs `geosite:telegram` and `geoip:telegram` to `proxy`.
* **🤖 Proxy AI Services**: Directs `geosite:openai` and `geosite:anthropic` to `proxy`.
* **📸 Proxy Blocked Socials**: Directs `geosite:twitter`, `geosite:instagram`, `geosite:facebook` to `proxy`.
* **🛑 Block BitTorrent / P2P**: Matches protocol `bittorrent` and sends it to `block`.
* **⚡ Direct BitTorrent / P2P**: Matches protocol `bittorrent` and sends it to `direct`.
* **🇨🇳 Bypass China**: Directs `geosite:cn` and `geoip:cn` to `direct`.
* **🎮 Proxy Gaming (Steam Community)**: Directs `geosite:steam` to `proxy`.

---

## GeoIP & GeoSite Database Pipeline

WhoisThat automates database lifecycle management for `geoip.dat` and `geosite.dat`:
1. Databases are stored globally in `~/.config/whoisthat/geo/` (or isolated per-profile in `~/.local/share/whoisthat/db/.../geo/` when using custom profile assets).
2. On startup, files are verified for presence and binary integrity.
3. If missing, global assets are downloaded from the official v2fly releases ([geoip](https://github.com/v2fly/geoip/releases) for `geoip.dat`, [domain-list-community](https://github.com/v2fly/domain-list-community/releases) for `geosite.dat`).
4. The core performs an in-memory configuration test (`xray run -test`) with a baseline `geoip:private` verification rule before committing routes to the network stack.

## See also

- [Routing Rules](/networking/routing-rules)
- [Routing Precedence](/reference/routing-precedence)
- [Happ Routing Profiles](/networking/happ-routing)
- [JSON Subscriptions](/networking/json-subscriptions)
