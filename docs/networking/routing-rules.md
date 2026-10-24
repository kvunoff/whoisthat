# Routing Rules & Presets Catalog

The Routing tab (<kbd>2</kbd> or <kbd>r</kbd>) allows you to define fine-grained traffic policies, deciding exactly which domains, IP ranges, or applications should traverse the VPN proxy, exit directly through your local ISP, or be blocked entirely.

---

## Supported Rule Match Types

WhoisThat supports six distinct matching criteria:

| Type | Internal Field | Description & Syntax Example |
| :--- | :--- | :--- |
| **`domain`** | `rule.domain` | Matches full domain or subdomain suffix: `example.com`, `sub.domain.org`. |
| **`ip`** | `rule.ip` | Individual IP addresses or CIDR network blocks: `1.2.3.4`, `192.168.1.0/24`. |
| **`protocol`** | `rule.protocol` | Protocol signature matching: `bittorrent`, `tls`, `http`. |
| **`port`** | `rule.port` | Destination port or range: `443`, `80,443`, `8000-8080`. |
| **`geoip`** | `geoip:<country>` | Country or geographic IP database lookup: `geoip:private`, `geoip:ru`, `geoip:us`. |
| **`geosite`** | `geosite:<category>`| Curated domain lists from v2fly: `geosite:category-ads`, `geosite:youtube`, `geosite:telegram`. |

---

## Outbound Targets

Every rule routes matching packets to one of three destination outbounds:

1. **`proxy`**: Encrypted and forwarded through the active VPN connection.
2. **`direct`**: Bypasses the VPN and exits through your physical local network gateway. (In TUN mode, this is achieved cleanly via `SO_MARK = 1` and Table 100).
3. **`block`**: Instantly blackholed and dropped. Ideal for ad-blocking and telemetry prevention.

---

## Live Hot-Reload (Zero Downtime)

Unlike traditional proxy clients that require tearing down the active connection, generating a new config file, and restarting the binary whenever a rule changes:

::: tip Instant Application
Pressing <kbd>Space</kbd> to toggle a rule, editing a domain, or adding a preset **applies immediately to the live connection** without dropping your active SSH sessions, game streams, or downloads.
:::

---

## The Presets Catalog (<kbd>p</kbd>)

Press <kbd>p</kbd> in the Routing tab to open the built-in **Routing Presets Catalog**. Each preset can be toggled on or off with a single keystroke:

```
┌──────────────────────── Routing Presets Catalog ────────────────────────┐
│                                                                        │
│  [X] 🇷🇺 Bypass Russia       Direct access to Russian domains & IPs     │
│  [X] 🛡️ Block Ads & Trackers Drop ad servers and telemetry networks    │
│  [X] 🏠 Bypass LAN / Local   Direct access to private RFC1918 subnets   │
│  [X] 📺 Proxy Media          Route YouTube, Twitch, and Discord         │
│  [X] 💬 Proxy Telegram       Route official Telegram domains and CIDRs  │
│  [X] 🤖 Proxy AI Services    Route OpenAI and Anthropic / Claude        │
│  [X] 📸 Proxy Blocked Socials Route Twitter/X, Instagram, and Meta      │
│                                                                        │
│             [Press Space/Enter to Toggle • Esc to Close]               │
└────────────────────────────────────────────────────────────────────────┘
```

### Preset Breakdown:
* **🇷🇺 Bypass Russia**: Injects `geosite:category-ru` and `geoip:ru` directed to `direct`.
* **🛡️ Block Ads & Trackers**: Injects `geosite:category-ads-all` directed to `block`.
* **🏠 Bypass LAN / Local**: Injects `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `127.0.0.0/8` directed to `direct`.
* **📺 Proxy Media**: Injects `geosite:youtube`, `geosite:twitch`, `geosite:discord` directed to `proxy`.
* **💬 Proxy Telegram**: Injects `geosite:telegram` and `geoip:telegram` directed to `proxy`.
* **🤖 Proxy AI Services**: Injects `geosite:openai` and `domain:anthropic.com`, `domain:claude.ai` directed to `proxy`.
* **📸 Proxy Blocked Socials**: Injects `geosite:twitter`, `geosite:instagram`, and `geosite:facebook` directed to `proxy`.

---

## GeoIP & GeoSite Pipeline

WhoisThat automates database updates for `geoip.dat` and `geosite.dat`:
1. Databases are stored in `~/.config/whoisthat/geo/`.
2. On startup, files are verified for size (minimum 10 MB).
3. If missing, they are downloaded from official [v2fly releases](https://github.com/v2fly/domain-list-community/releases) with exponential retry.
4. The core performs a sanity test with a `geoip:private` verification rule before finalizing startup.
