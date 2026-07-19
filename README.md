# WhoisThat

A modern terminal-based VPN client. Rust TUI frontend. Go engine backed by Xray-core.

**Supports**: VLESS (Reality/xHTTP/gRPC), VMess, Trojan, Shadowsocks, SOCKS5, Hysteria2. Full TUN-mode VPN. Subscription-based profile management. HWID device identification. Optional systemd user service for boot autostart.

![WhoisThat](whoisthat-screen.jpg)

---

## Table of Contents

- [Installation](#installation) — quick install, AUR, manual build, configuration
- [Features](#features)
- [Architecture](#architecture) — how it works, routing rules, HWID
- [TCP API Protocol](#tcp-api-protocol) — wire format, commands, notifications, structures
- [Usage](#usage) — keybindings, settings, TUN mode, systemd, subscriptions
- [Troubleshooting](#troubleshooting)
- [Testing](#testing)
- [Development](#development) — dev loop, logs, debugging, CI
- [File Structure](#file-structure)
- [Credits](#credits)
- [License](#license)

---

## Installation

### Quick install (any Linux distribution)

```bash
curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash
```

The script auto-detects your distro, installs Go and Rust from official channels,
builds everything from the latest tagged release, and copies binaries to `/usr/local/bin`.
Xray-core is included. tun2socks is offered as an opt-in for TUN mode.

### Arch Linux (AUR)

```bash
paru -S whoisthat
# or
yay -S whoisthat
```

### Manual build

Prerequisites: **Rust** 1.80+, **Go** 1.25+, **git**, **curl**, a C compiler.

```bash
git clone https://github.com/kvunoff/whoisthat.git
cd whoisthat

# Parser (standalone Rust binary — URI → Xray JSON)
cd parser && cargo build --release && cd ..

# Core (Go daemon — VPN engine)
cd core/core && go build -o whoisthat-core && cd ../..

# TUI (Rust — terminal interface)
cargo build --release

# Install
sudo install -Dm755 target/release/whoisthat              /usr/local/bin/whoisthat
sudo install -Dm755 core/core/whoisthat-core               /usr/local/bin/whoisthat-core
sudo install -Dm755 parser/target/release/whoisthat-parser /usr/local/bin/whoisthat-parser

# TUN mode capability setup (one-time, optional — TUI auto-prompts if skipped)
sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /usr/local/bin/whoisthat-core
```

Xray-core can be installed via `go install github.com/XTLS/Xray-core@latest`
and moved to PATH. tun2socks is only needed for TUN mode.

### Configuration

**TUI config** — `~/.config/whoisthat/config.toml`:

```toml
core_tcp_port = 4897
core_host = "127.0.0.1"
autoconnect = false
last_group_id = 0
last_profile_id = 0
show_ip = true
log_enabled = false
log_level = "warn"
test_method = "http-get"
tun_name = "whoisthattun"                  # mirrors core's `tun-name` — kept in sync by the TUI
kill_switch_enabled = false
```

**Core config** — `~/.config/whoisthat/config.json` (auto-generated):

```json
{
  "socks-port": 3090,
  "http-port": 3091,
  "core-tcp-port": 4897,
  "test-port-range": { "start": 3095, "end": 30120 },
  "dns-servers": ["1.1.1.1", "8.8.8.8"],
  "tun-name": "whoisthattun",
  "hwid-enabled": true,
  "hwid": "1fb1e0141ab3e35a",
  "user-agent": "whoisthat/v0.7.2",
  "kill-switch-enabled": false,
  "autoconnect-enabled": false,
  "autoconnect-group-id": 0,
  "autoconnect-profile-id": 0,
  "autoconnect-mode": "proxy"
}
```

`dns-servers` is a list of DNS server IPs used in three contexts:

- **Profile resolution** — resolving proxy hostnames to IPs (all servers queried, results merged)
- **Xray direct outbound** — DNS servers injected into xray's config for `freedom` (direct) outbound domain resolution
- **TUN mode** — the first server in the list is used for system-wide DNS hijack via iptables/nftables DNAT rules

`socks-port` and `http-port` set the local SOCKS5/HTTP proxy ports. `test-port-range` defines the port pool for latency testing (spawns temporary xray instances).

Profile data is stored under `~/.local/share/whoisthat/db/`.
Encrypted at rest with AES-256-GCM — key auto-generated on first run.

---

## Features

- **Subscription support** — add groups with subscription URLs, refresh profiles, view metadata (traffic used/limit, expiry)
- **HWID device identification** — auto-generated hardware ID sent with subscription requests (Remnawave-compatible x-hwid headers). Configurable: toggle on/off, reset, custom user-agent. Respects `x-hwid-max-devices-reached` and other HWID response headers from the subscription server.
- **Group management** — add, rename, edit subscription URL, delete entire groups
- **Profile import** — VLESS, VMess, Trojan, Shadowsocks, SOCKS5, Hysteria2 URIs (paste, clipboard, or subscription refresh)
- Connect / disconnect / switch profiles
- Full system-wide TUN-mode VPN (`tun2socks` + `iptables`/`nftables`, auto-detected)
- **Profile testing** — three methods: TCP connect (fast prefilter), HTTP GET, HTTP HEAD via SOCKS5. Multi-sample (default 3) with median latency, jitter, and packet-loss %.
- **Per-protocol dispatch** — xray protocols (vless/vmess/trojan/ss/socks/http) spawn a mini xray; hysteria2 profiles spawn the official `hysteria` client. No more "everything goes to xray and fails for hy2".
- **Test progress** — pending profiles show `…` immediately; in-flight batches broadcast tested/total progress. `C` cancels in-flight tests gracefully (epoch counter; no orphan subprocesses).
- **Scan-all testing** — `t` scans all profiles across all groups with dedup; `T` tests only focused profile/subscription. Group-focused tests use the single `test-group` TCP command for efficiency.
- **Auto-test on subscription refresh** — profiles are tested automatically after `u` so you see live latencies immediately. Toggle in Settings → Diagnostics.
- **Custom routing rules** — domain, IP, protocol, port, geoip, geosite → proxy/direct/block (`r` tab). `direct` outbound works correctly in TUN mode via SO_MARK + fwmark routing (no root required). Use ←/→ to cycle type/outbound in the form.
- **Kill-switch** — When enabled, blocks all non-VPN traffic if the connection drops. Uses a dedicated firewall table (`whoisthat_ks`, `whoisthat_ks_v6`) entirely independent of TUN rules — safe to combine with any routing setup. Works in both SOCKS and TUN modes. Toggle in Settings.
- Real-time connection status with uplink/downlink traffic stats
- **Log viewer** — live tail from core log, auto-scroll, [WARN]/[ERRO] highlighting
- **Configurable** — DNS servers, proxy ports, log level, test method, HWID, user-agent via settings and config files
- **Detach/reattach** — `q` leaves VPN running in background, reopen TUI to reattach
- **Boot autostart** — autoconnect on startup with configurable mode (proxy or TUN); optional systemd user service for starting VPN at boot (before login via lingering)
- **Profile search** — `/` to filter profiles by name, protocol, address, or host
- Public IP display (auto-refreshed every 30s and on connect/disconnect/TUN-toggle)
- Dark color scheme (Tokyo Night inspired)
- Keyboard-driven — mouse is optional

---

## Architecture

```text
┌──────────────────────────────┐
│  WhoisThat (Rust TUI)        │  ratatui + crossterm
│  ⋅ Profiles ⋅ Logs ⋅ Settings│
└──────────┬───────────────────┘
           │  TCP / JSON
           │  4-byte big-endian length prefix + JSON payload
           │  localhost:4897
           ▼
┌──────────────────────────────┐
│  WhoisThat Core (Go daemon)  │
│  ⋅ Profile DB (JSON files)   │
│  ⋅ Proxy manager             │
│  ⋅ TUN manager               │
│  ⋅ TCP server (commands)     │
└──────────┬───────────────────┘
           │  subprocess (stdin JSON config)
           ▼
┌──────────────────────────────┐
│  Xray-core                   │
│  ⋅ VLESS / Reality / gRPC    │
│  ⋅ xHTTP inbound             │
│  ⋅ SOCKS5 / HTTP outbound    │
└──────────┬───────────────────┘
           │
     ┌─────┴─────┐
     ▼           ▼
   TUN device   DNS routing
   (tun2socks)  (iptables/nftables)
```

### How it works

1. **WhoisThat Core** is a long-running Go daemon. It manages VPN profiles (stored as JSON files under `~/.local/share/whoisthat/db/`), launches Xray-core as a subprocess, and controls the TUN device via `iproute2` + `tun2socks`.

2. **Protocol subprocesses.** The core spawns one of two subprocesses per profile:
   - **Xray-core** — VLESS (incl. Reality/xTLS Vision), VMess, Trojan (incl. reality/WS/gRPC), Shadowsocks, SOCKS5. xray-core JSON config is generated on-the-fly from the profile URI by the bundled `whoisthat-parser`. xray-core does **not** implement the Hysteria2 protocol.
   - **Hysteria2 client** (apernet/hysteria2, optional — installed separately by `install.sh`) — spawned only for `hysteria2://` / `hy2://` profiles. The parser emits a YAML config (server, auth, TLS, obfs/salamander, bandwidth, port-hopping) which is fed via stdin to `hysteria run -c -`. xray's stats/routing/DNS injection does not apply.

3. **TUN mode** creates a virtual network interface (configurable name, default `whoisthattun`), sets up `iptables`/`nftables` rules (DNS hijack, MASQUERADE, auto-detected at runtime), and routes all system traffic through the Xray SOCKS5 proxy via `tun2socks`.

4. **WhoisThat TUI** (this Rust binary) connects to the core over TCP on `127.0.0.1:4897`. It sends commands and receives asynchronous notifications. The TUI never touches networking directly — all VPN logic lives in the core.

### Routing rules

Custom routing rules can redirect traffic to `proxy`, `direct`, or `block` outbounds. Six match types are supported:

| Type | Stored as | Example |
| -------- | ---------- | -------- |
| `domain` | `rule.domain` | `example.com` |
| `ip` | `rule.ip` | `1.2.3.4` or `10.0.0.0/8` |
| `protocol` | `rule.protocol` | `bittorrent` |
| `port` | `rule.port` | `443` |
| `geoip` | `rule.ip` prefixed `geoip:` | `geoip:private` |
| `geosite` | `rule.domain` prefixed `geosite:` | `geosite:category-ads` |

Rules are stored in `~/.local/share/whoisthat/db/routing.json` and injected into xray's JSON config on every connect (DNS-bypass rule first, then user rules in order, disabled rules skipped).

**Default rule:** private IP ranges → direct, hardcoded as CIDR (not `geoip:private`) so it works even when geo files aren't available.

**GeoIP / GeoSite support** — `geoip.dat` and `geosite.dat` are auto-downloaded from [v2fly GitHub releases](https://github.com/v2fly/domain-list-community/releases) on first startup to `~/.config/whoisthat/geo/`. The download pipeline:

1. If a valid `geoip.dat` (≥10 MB) already exists locally → skip download
2. Fall back to system paths (`/usr/share/xray`, etc.) → copy if found
3. Otherwise download from v2fly with retry (3 attempts, exponential backoff)
4. **Verify** by running xray with a `geoip:private` test rule — broken files trigger re-download
5. If all downloads fail → `XRAY_LOCATION_ASSET` is not set; xray uses bundled geo data (if any) or ignores geo rules

The verification pass adds ~5s to first startup. Set `XRAY_LOCATION_ASSET` manually to point xray at a custom asset directory.

**Proxy mode (SOCKS5):**

- DNS queries (UDP port 53) always go through `proxy` — this prevents user domain rules from interfering with DNS resolution
- Matched traffic follows the user's rule; unmatched traffic implicitly falls back to `proxy` (first outbound)
- `direct` rules work correctly: xray resolves the domain via its internal DNS (through proxy), then connects directly via the `freedom` outbound

**TUN mode:**

- The TUN default route sends ALL system traffic through `tun2socks` → SOCKS5 → xray. Without special handling, xray's own `freedom` outbound traffic would loop back into TUN
- **Root mode:** Xray runs under a dedicated UID (61000+ range). `ip rule uidrange` + table 100 routes xray traffic through the physical gateway, bypassing TUN
- **Capability mode (no root):** Freedom outbound sets `SO_MARK` via xray's `sockopt.mark`. `ip rule fwmark 1 table 100` routes marked packets through the physical gateway. Works under file capabilities — no root needed
- User applications retain their normal routing and stay under TUN protection
- This ensures `direct` outbound connections from xray bypass TUN while all user traffic stays protected
- **Incoming connections to local services** (e.g., a web or game server) are also handled correctly via conntrack-based reply routing: incoming flows on the physical interface are tagged in the connection tracker, and reply packets are marked with `fwmark 1` to bypass TUN and exit through the physical gateway. Works for both host-local servers and Docker-published ports, in both nftables and iptables backends

### HWID (Device Identification)

When subscription updates are fetched, the core sends HTTP headers identifying the device (Remnawave/Happ standard):

| Header | Value | Source |
| -------- | ------- | -------- |
| `x-hwid` | `1fb1e0141ab3e35a` | Auto-generated 8-byte hex (stored in config.json) |
| `x-device-os` | `Linux` | `runtime.GOOS` |
| `x-ver-os` | `6.12.0-arch1-1` | `uname -r` |
| `x-device-model` | `Arch Linux` | `/etc/os-release` PRETTY_NAME |
| `user-agent` | `whoisthat/v0.7.2` | User-configurable (Settings) |

Response headers (`x-hwid-max-devices-reached`, `x-hwid-not-supported`, `x-hwid-limit`) are inspected and trigger warnings when device limits are reached.

HWID can be toggled off, reset, or have its user-agent customized in Settings.

---

## TCP API Protocol

### Wire format

```text
┌──────────────┬───────────────────────────┐
│ 4 bytes (BE) │ JSON payload              │
│ uint32 len   │ {"msg":"...","data":{...}}│
└──────────────┴───────────────────────────┘
```

Both client→core commands and core→client notifications use the same framing. The core broadcasts notifications to all connected clients.

### Commands (Client → Core)

| Message | Data | Response |
| --- | --- | --- |
| `get-application-state` | `{}` | `application-state` |
| `connect` | `{"profile":{"id":int,"group_id":int}}` | `status-changed` |
| `disconnect` | `{}` | `status-changed` |
| `add-profiles` | `{"uris":"...","group_id":int}` | `profiles-added` |
| `delete-profiles` | `{"profiles":[{"id":int,"group_id":int}]}` | `profiles-deleted` |
| `test-profile` | `{"profile":{"id":int,"group_id":int},"method":"str"}` | `profile-updated` |
| `enable-tun` | `{}` | `tun-status-changed` |
| `disable-tun` | `{}` | `tun-status-changed` |
| `is-root` | `{}` | `is-root-answer` |
| `update-profile` | `{"Profile":{"id":int,"group_id":int},"Name":"str"}` | `profile-updated` |
| `update-group` | `{"id":int,"name":"str","subscription_url":"str"}` | `group-updated` |
| `add-group` | `{"name":"str","subscription_url":"str"}` | `group-added` |
| `delete-group` | `{"id":int}` | `group-deleted` |
| `update-subscription` | `{"group_id":int}` | `subscription-updated` |
| `set-tun-name` | `{"tun_name":"str"}` | `tun-name-updated` |
| `set-hwid` | `{"enabled":true/false,"user_agent":"str","reset":true/false}` | `hwid-updated` |
| `get-routing` | `{}` | `routing-updated` |
| `update-routing` | `{"config":{...}}` | `routing-updated` |
| `die` | `{}` | (stops core) |
| `set-kill-switch` | `{"enabled":bool}` | `kill-switch-updated` |
| `set-autoconnect` | `{"enabled":bool,"group_id":int,"profile_id":int,"mode":"proxy"\|"tun"}` | `autoconnect-updated` |

### Notifications (Core → All Clients)

| Message | Data |
| --- | --- |
| `application-state` | Full state: groups, profiles, connection status, TUN status, HWID info, kill switch, autoconnect |
| `status-changed` | `{"connection":"connected"\|"disconnected","profile":{...}}` |
| `profiles-added` | `{"profiles":[...]}` |
| `profiles-deleted` | `{"deleted-profiles":[...]}` |
| `profile-updated` | `{"profile":{...}}` (also fires on test result) |
| `group-added` | `{"id":int,"name":"str","subscription_url":"str"}` |
| `group-deleted` | `{"id":int}` |
| `group-updated` | `{"id":int,"name":"str",...}` (full Group object) |
| `subscription-updated` | `{"group_id":int,"group":{...},"profiles":[...]}` |
| `tun-status-changed` | `{"is_enabled":bool}` |
| `tun-name-updated` | `{"tun_name":"str"}` |
| `is-root-answer` | `{"IsRoot":bool}` |
| `hwid-updated` | `{"enabled":bool,"hwid":"str","user_agent":"str","platform":"str","kernel":"str","model":"str"}` |
| `traffic-stats` | `{"proxy_up":int,"proxy_down":int,"direct_up":int,"direct_down":int}` |
| `routing-updated` | `{"config":{...}}` |
| `warn` | `{"key":"str","content":"str"}` |
| `kill-switch-updated` | `{"enabled":bool}` |
| `autoconnect-updated` | `{"enabled":bool,"mode":"proxy"\|"tun"}` |

### Profile structure

```json
{
  "id": 1,
  "group_id": 0,
  "nano-id": "abc123",
  "name": "My Server",
  "protocol": "vless",
  "uri": "vless://...",
  "address": "1.2.3.4",
  "host": "example.com",
  "test-result": 45
}
```

`test-result`: `>0` = median latency in ms across N samples, `-1` = failed, `-2` = testing in flight, `0` = untested. Each profile also carries optional `tested_at` (unix ts the last successful test completed), `loss-pct` (percentage of samples that failed), and `jitter-ms` (max − min latency of successful samples).

### Group structure

```json
{
  "id": 1,
  "name": "My Sub",
  "subscription_url": "https://...",
  "last_id": 42,
  "sub_last_updated": 1712345678,
  "sub_expires": 1720000000,
  "sub_upload": 1048576,
  "sub_download": 52428800,
  "sub_total": 107374182400
}
```

Subscription metadata (`sub_*`) is populated from the `subscription-userinfo` HTTP header returned by the subscription server. Fields are `omitzero` — omitted from JSON when zero.

---

## Usage

### Navigation

| Key | Action |
| --- | --- |
| `j` / `↓` | Move cursor down |
| `k` / `↑` | Move cursor up |
| `g` | Jump to top |
| `G` | Jump to bottom |
| `Tab` | Switch focus (list ↔ details panel) |
| `/` | Search / filter profiles (type to filter, Esc to clear) |
| `h` / `?` | Show help (context-aware) |

### Connection

| Key | Action |
| --- | --- |
| `c` / `Enter` | Connect to selected profile |
| `d` | Disconnect |
| `t` | Test all profiles (starts from cursor, top-to-bottom, dedup) |
| `T` | Test focused profile or subscription group only |
| `v` | Toggle TUN mode (checks caps first; on first run offers one-time `pkexec setcap` setup, then enables TUN) |

### Profiles & Groups

| Key | Action |
| --- | --- |
| `a` | Import profile URI (clipboard or manual input — vless://, vmess://, trojan://, ss://, socks://, hysteria2://, hy2://) |
| `x` | Delete selected profile |
| `X` | Delete current group (with confirmation) |
| `e` | Edit group (name + subscription URL) **or** rename selected profile |
| `U` | Add new group (name + subscription URL) |
| `u` | Update subscription (refresh profiles from URL) |
| `Ctrl+V` | Paste from clipboard in input popups |

### Tabs & Quit

| Key | Action |
| --- | --- |
| `l` | Logs view (live tail with auto-scroll, `f` to filter by level) |
| `r` | Routing rules (domain/IP/protocol/port/geoip/geosite → proxy/direct/block) |
| `s` | Settings |
| `1` / `Esc` | Back to Profiles |
| `q` | Detach TUI (VPN stays connected in background) |
| `Q` / `Ctrl+C` | Full quit (stop VPN + exit) |

### Settings

| Setting | Values | Description |
| --- | --- | --- |
| Autoconnect | on/off | Auto-connect to last used profile on startup (core handles boot autostart) |
| Autostart mode | proxy/tun | VPN mode for boot autostart (proxy = SOCKS5, tun = full system VPN) |
| Systemd autostart | on/off | Start VPN core at boot via systemd user service (auto-enables linger) |
| Show IP | on/off | Display public IP in top bar |
| TUI log | on/off | Enable Rust TUI debug log (`~/.local/share/whoisthat/tui.log`) |
| Log level | error/warn/info/debug/trace | Minimum log level for TUI and core |
| Test method | tcp/http-get/http-head | Latency test method (tcp = direct dial prefilter, http = via SOCKS5 proxy, multi-sample) |
| Test samples | 1/3/5/10 | How many HTTP round-trips per profile test (median is reported) |
| Test concurrency | 4/8/16/32/64 | Max simultaneous in-flight test subprocesses |
| Test timeout | 3s/5s/10s/15s | Per-sample HTTP timeout |
| Test endpoint | cloudflare/gstatic/bing | Target URL for SOCKS5-routed HTTP tests (cycles to fallback on failure) |
| Auto-test on refresh | on/off | Automatically test profiles after a successful `u` subscription refresh |
| TUN name | editable text | TUN interface name (1-15 chars, letters/digits/underscore/dash, default `whoisthattun`) |
| Kill Switch | on/off | Block all non-VPN traffic on connection drop (dedicated firewall table, works in both SOCKS and TUN modes) |
| HWID: Enabled | on/off | Send HWID headers with subscription requests |
| HWID | 1fb1e0141ab3e35a | Device identifier (read-only, auto-generated) |
| Reset HWID | ⏎ | Generate a new random HWID |
| User-Agent | whoisthat/v0.7.2 | User-Agent header (editable — press Enter to modify) |

Navigate with `j`/`k`, press `Enter`/`Space` to toggle, cycle values, open edit popups, or execute actions.

### TUN Mode

TUN mode creates a virtual interface (configurable in Settings, default `whoisthattun`), configures `iptables` or `nftables` rules (DNS hijack, MASQUERADE — auto-detected at runtime), and routes all system traffic through the VPN. DNS queries are redirected to the first server in `dns-servers` config.

**No root required.** TUN mode runs under file capabilities (`cap_net_admin`, `cap_net_raw`, `cap_setpcap`). On first launch, the TUI detects missing capabilities and offers a one-time `pkexec` setup. After that, TUN works as a normal user. The install script sets capabilities automatically.

**How it works internally:**

- `whoisthat-core` has `cap_net_admin,cap_net_raw,cap_setpcap=+ep` set on its binary
- At startup, the core uses `capset(2)` to move permitted capabilities into the inheritable set (`CAP_SETPCAP` enables this)
- `prctl(PR_CAP_AMBIENT_RAISE)` promotes them to the ambient set
- All subprocesses (`sh`, `ip`, `iptables`/`nftables`, `tun2socks`) automatically inherit the capabilities
- No `sudo`, no root, no setuid — pure Linux capabilities

For debugging or manual setup: `sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /path/to/whoisthat-core`.

**Capability detection** (`v` key before enabling TUN) creates a real test TUN device (`wt-capcheck`) and tears it down — a true functional check, not just a UID test. This means capabilities mode works correctly even when the binary has `+ep` but the user is not root.

### Systemd Integration

WhoisThat Core can run as a **systemd user service**, starting the VPN engine at boot — before you log in — via [lingering](https://wiki.archlinux.org/title/Systemd/User#Automatic_start-up_of_systemd_user_instances). Toggle it from **Settings → Systemd autostart** or manage it manually from the shell.

**Unit file:** `~/.config/systemd/user/whoisthat-core.service`

```ini
[Unit]
Description=WhoisThat VPN Core
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/path/to/whoisthat-core
Environment=WHOISTHAT_LOG_LEVEL=warn
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

The TUI generates this unit automatically when you enable the setting (it resolves the absolute path to `whoisthat-core` and substitutes the current log level). It also enables lingering via `pkexec loginctl enable-linger $USER` so the user service starts at boot.

**Service control:**

```bash
systemctl --user status  whoisthat-core   # current status
systemctl --user start   whoisthat-core   # start now
systemctl --user stop    whoisthat-core   # stop now
systemctl --user restart whoisthat-core   # restart
systemctl --user disable whoisthat-core   # remove from boot
```

**Logs:**

```bash
# Live tail of the core's stdout/stderr (captured by journald)
journalctl --user -u whoisthat-core -f

# Last 200 lines
journalctl --user -u whoisthat-core -n 200

# Today's logs only
journalctl --user -u whoisthat-core --since today
```

The core also writes its own rotating log to `~/.config/whoisthat/core.log` (20 MB rotation, one backup) regardless of the systemd setting. The journal captures the subprocess-prefixed output; the file log contains the daemon's own structured lines.

**Linger check / enable:**

```bash
loginctl show-user $USER --property=Linger   # → Linger=yes/no
sudo loginctl enable-linger  $USER            # allow user services at boot
sudo loginctl disable-linger $USER           # revoke
```

**Detach vs systemd:** `q` in the TUI detaches but the core keeps running *for this session*. The systemd service is for boot autostart — they're orthogonal. You can detach the TUI, reattach later, and the systemd service continues independently across reboots.

**After rebuilding the core:** the unit's `ExecStart=` points at a binary with a new inode. `systemctl --user restart whoisthat-core` is enough — no `daemon-reload` needed unless you edit the unit file by hand. The TUI runs `daemon-reload` automatically when toggling the setting.

### Subscription Workflow

1. Press `U` to add a new group — enter a name and subscription URL
2. Press `u` with the group selected to fetch and parse profiles from the URL (sends HWID headers if enabled)
3. The details panel shows subscription metadata when available (traffic used, expiry, last updated)
4. Press `e` to edit the group name or subscription URL at any time
5. Press `X` to delete the entire group and its profiles

---

## Troubleshooting

| Symptom | Likely cause | Fix |
| --------- | ------------- | ----- |
| `TUN mode needs network capabilities on core binary` popup | Missing caps on `whoisthat-core` (new inode after `go build`, fresh clone, new machine) | `sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep ./core/core/whoisthat-core` (or accept the `pkexec` prompt the TUI offers) |
| TUN toggle says "Checking root…" then nothing | Core has caps but kernel/distro blocks ambient set — rare on hardened kernels | Run `whoisthat-core` directly: `./core/core/whoisthat-core` and check `~/.config/whoisthat/core.log`; verify `CanTun()` tests a real TUN device `wt-capcheck` |
| `Failed to connect to core: Connection refused` on startup | Port 4897 occupied by a stale core, or core crashed | `ss -tlnp \| grep 4897` to find the culprit; kill it, or `pkill -f whoisthat-core` |
| TUI detaches but VPN drops when I close terminal | Ran with `q` (detach) — core keeps living, but was originally launched as foreground child of the shell | Use **Settings → Systemd autostart** so the core runs as a user service, independent of any terminal |
| Profile test shows `err` | Server down, wrong creds, or firewall blocking the test port range | Check `~/.config/whoisthat/core.log` for the test config and dialer error; verify outbound on the `test-port-range` (config.json) is open |
| Routing rule doesn't trigger | Disabled in the rules list, or geo files missing | Press `Space` to enable; for `geoip`/`geosite` rules check `~/.config/whoisthat/geo/*.dat` exists and is ≥10 MB; re-trigger download by removing the files and restarting |
| Kill-switch left dangling rules after exit | `Q` full-quit killed the core before it could clean up firewall rules | `rm` the `whoisthat_ks*` tables manually: `nft delete table whoisthat_ks 2>/dev/null; nft delete table ip6 whoisthat_ks_v6 2>/dev/null` (or reclaim via core restart) |
| Settings toggle shows "Could not enable lingering" | `pkexec loginctl enable-linger $USER` failed or was cancelled | Run in a terminal: `sudo loginctl enable-linger $USER` then retry the toggle in the TUI |
| `whoisthat-screen.jpg` doesn't exist in build artifact | Image is checked into the repo but not in `target/` — only used by the README on GitHub | Ignore — it's display-only, not a runtime asset |
| Logs pane is empty | No core log file, or log level filtering hides everything | Press `f` in the Logs tab to cycle the level filter; or raise log level in Settings |
| `Cannot decrypt DB file` style errors in core log | Key file `~/.local/share/whoisthat/db/.key` was moved or deleted, but encrypted files remain | Keep the `.key` file — it's the AES-256-GCM master key, no fallback. If unsalvageable: stop core, delete `~/.local/share/whoisthat/db/`, restart to generate fresh key + empty DB |

**If you hit something not listed here:** `tail -f ~/.config/whoisthat/core.log whoisthat.log` and reproduce. Both logs are the first place to look — not the source code.

---

## Testing

The project has unit tests for both the Rust TUI and the Go core. No external dependencies or network access required — all tests run in milliseconds.

### Rust

```bash
cargo test
```

Covers:

- **Message dispatch** (`src/core_client/dispatch.rs`) — all notification message types (19), unknown type handling, malformed JSON
- **Routing form logic** (`src/ui/routing.rs`) — `form_to_rule` / `rule_to_form` for all 6 match types and 3 outbounds, round-trip consistency
- **Settings layout** (`src/ui/settings.rs`) — grouped layout, cursor navigation skipping headers, clamping
- **Text editor** (`src/main.rs`) — `edit_text_field`: insert, backspace, delete, cursor movement, Home/End boundary conditions

### Go

```bash
cd core/core
go test ./lib/crypto/... ./lib/AppConfig/... ./db/...
```

Covers:

- **Crypto** (`lib/crypto`) — AES-256-GCM encrypt/decrypt round-trip, base64 wrappers, wrong key, short ciphertext, empty plaintext, nonce randomness
- **Config** (`lib/AppConfig`) — default port values, DNS servers, HWID format (16 lowercase hex chars), HWID randomness
- **Database** (`db`) — path helpers, encrypt/decrypt round-trip via `writeEncryptedJSON`/`readEncryptedJSON`, encrypted file detection, key file creation and reuse across instances

---

## Development

### Local dev loop

No install step needed during development — `cargo run` plus a `go build` in `core/core/` is enough. The TUI auto-spawns `whoisthat-core` from `./core/core/whoisthat-core`, then `whoisthat-core` on `PATH`, then `/usr/local/bin/whoisthat-core`.

```bash
# Build core (must be first — TUI spawns this binary)
cd core/core && go build -o whoisthat-core && cd ../..

# Build parser (used by core at runtime)
cd parser && cargo build --release && cd ..

# Run TUI
cargo run
```

**Gotcha:** every `go build` creates a new binary inode, so file capabilities (`setcap`) are **lost after rebuild**. The TUI detects this at startup and offers a one-time `pkexec setcap` — or re-run manually:

```bash
sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep ./core/core/whoisthat-core
```

### Log files

| File | Produced by | When | Default level |
| -------- | ------------- | ----- | --------------- |
| `whoisthat.log` (CWD) | Rust TUI | Always (file only, never to screen) | `warn` |
| `~/.config/whoisthat/core.log` | Go core | Always, lumberjack rotation 20 MB + 1 backup | `warn` |
| `/tmp/whoisthat-core.log` | Go core | Fallback if config dir is unwritable | `warn` |
| `~/.local/share/whoisthat/tui.log` | Rust TUI | When "TUI log" setting is enabled in Settings | configured level |

Enable verbose logs at runtime via env var (overrides config):

```bash
WHOISTHAT_LOG_LEVEL=debug cargo run
```

Or toggle from **Settings → TUI log** + **Log level** — both apply to the TUI and are forwarded to the spawned core.

### Inspecting the core daemon manually

The core listens on `127.0.0.1:4897`. Anything that speaks the length-prefixed JSON protocol can talk to it. During development it's often useful to:

```bash
# Check if the core is up
ss -tlnp | grep 4897

# Tail the core's rotating log
tail -f ~/.config/whoisthat/core.log

# Inspect encrypted DB files (decryption requires the core; files are AES-256-GCM)
ls -la ~/.local/share/whoisthat/db/
```

### Debugging routing / TUN rules at runtime

```bash
ip route show table 100                       # dedicated-UID / fwmark bypass table
ip rule show | grep 1                         # fwmark rule
nft list table 2> /dev/null || iptables -L -n # active firewall rules
ip link show whoisthattun                     # the TUN interface
```

### Continuous Integration (AUR autoupdate)

File: `.github/workflows/aur-publish.yml` — triggers on every `v*` tag push.

1. Parse the version from the tag
2. Clone the live AUR repo
3. Compute `pkgrel` (increment if `pkgver` matches latest AUR, otherwise reset to 1)
4. Regenerate the `PKGBUILD` and `.SRCINFO` **in CI** (the files in the repo are reference-only)
5. Push to AUR using the `AUR_SSH_KEY` secret

So `pkgrel` in `pkg/aur/PKGBUILD` is reference-only — never hand-bump it for releases. See `UPDATE.md` for the full release procedure.

---

## File Structure

```text
whoisthat/
├── Cargo.toml          ← Rust project manifest (TUI)
├── README.md / AGENTS.md / UPDATE.md
├── install.sh          ← Universal installer / updater
├── src/                ← Rust TUI source
│   ├── main.rs         ← Entry point, event loop, autoconnect, systemd setup, caps
│   ├── config.rs       ← Config loader (~/.config/whoisthat/config.toml)
│   ├── core_client/    ← TCP client for the Go core
│   │   ├── protocol.rs ← All serde types mirroring Go structs
│   │   ├── connection.rs ← TCP + 4-byte length framing
│   │   ├── dispatch.rs ← Read loop → typed event channel
│   │   └── commands.rs ← High-level async send functions
│   └── ui/             ← ratatui components
│       ├── app/        ← Main app state + rendering (mod, types, state, render, tree, details, popups, helpers)
│       ├── theme.rs    ← Color palette (Tokyo Night)
│       ├── settings.rs ← Settings screen
│       ├── routing.rs  ← Routing rules tab + popups
│       ├── logs.rs     ← Log viewer (live tail + auto-scroll + level filter)
│       ├── uri.rs      ← URI detail parser (VLESS/VMess/Trojan/SS/SOCKS/Hysteria2)
├── parser/             ← whoisthat-parser — URI → Xray JSON (standalone Rust binary)
│   ├── Cargo.toml
│   └── src/
├── core/               ← WhoisThat Core (Go VPN engine)
│   └── core/
│       ├── main.go     ← Daemon entry point; RaiseAmbientCaps() before everything
│       ├── go.mod
│       ├── commands/   ← TCP command handlers (connect, test, groups, subscription, ...)
│       ├── structs/    ← Shared data types (mirror of src/core_client/protocol.rs)
│       ├── db/         ← JSON file-based profile DB + readEncryptedJSON / MigrateToEncrypted
│       ├── utils/      ← caps.go (RaiseAmbientCaps, CanTun), user.go (UIDs)
│       └── lib/        ← Core libraries
│           ├── logger/      ← Structured logger (lumberjack rotation, 20 MB)
│           ├── TCPServer/  ← TCP server + dispatcher (length-prefixed JSON)
│           ├── AppConfig/  ← Core configuration, defaults, HWID gen
│           ├── PortPool/   ← Dynamic port allocator for test/profile instances
│           ├── crypto/     ← AES-256-GCM encrypt/decrypt for DB files
│           ├── geo/        ← Auto-download + verify geoip.dat / geosite.dat from v2fly
│           └── proxy/      ← mainproxy (connect/stop/status), xray wrapper, TUN manager + scripts, routing
├── pkg/aur/            ← AUR packaging (PKGBUILD + .SRCINFO)
└── .github/workflows/
    └── aur-publish.yml ← Tag-triggered CI: pushes updated PKGBUILD to AUR
```

---

## Credits

Powered by [Xray-core](https://github.com/XTLS/Xray-core), [tun2socks](https://github.com/xjasonlyu/tun2socks).

---

## License

MIT — see [LICENSE](LICENSE).
