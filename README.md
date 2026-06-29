# WhoisThat

A modern terminal-based VPN client. Rust TUI frontend. Go engine backed by Xray-core.

**Supports**: VLESS (Reality/xHTTP/gRPC), VMess, Trojan, Shadowsocks, SOCKS5, Hysteria2. Full TUN-mode VPN. Subscription-based profile management. HWID device identification.

![WhoisThat](whoisthat-screen.jpg)

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
tun_name = "whoisthattun"
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
  "user-agent": "whoisthat/v0.7.0",
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
- **Profile testing** — three methods: TCP connect, HTTP GET (SOCKS5 → Cloudflare), HTTP HEAD
- **Scan-all testing** — `t` scans all profiles across all groups with dedup; `T` tests only focused profile/subscription
- **Custom routing rules** — domain, IP, protocol, port, geoip, geosite → proxy/direct/block (`r` tab). `direct` outbound works correctly in TUN mode via SO_MARK + fwmark routing (no root required). Use ←/→ to cycle type/outbound in the form.
- **Kill-switch** — When enabled, blocks all non-VPN traffic if the connection drops. Uses a dedicated firewall table (`whoisthat_ks`) independent of TUN rules. Works in both SOCKS and TUN modes. Toggle in Settings.
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

2. **Xray-core** handles all protocol-level work: VLESS/VMess/Trojan/Shadowsocks/SOCKS/Hysteria2 handshakes, Reality authentication, xHTTP/gRPC/WS/TCP transport, SOCKS5 local proxy. Its JSON config is generated on-the-fly from profile URIs by the bundled `whoisthat-parser`.

3. **TUN mode** creates a virtual network interface (configurable name, default `whoisthattun`), sets up `iptables`/`nftables` rules (DNS hijack, MASQUERADE, auto-detected at runtime), and routes all system traffic through the Xray SOCKS5 proxy via `tun2socks`.

4. **WhoisThat TUI** (this Rust binary) connects to the core over TCP on `127.0.0.1:4897`. It sends commands and receives asynchronous notifications. The TUI never touches networking directly — all VPN logic lives in the core.

### Routing rules

Custom routing rules (domain, IP, protocol, port) can redirect traffic to `proxy`, `direct`, or `block` outbounds. Rules are stored in `~/.local/share/whoisthat/db/routing.json` and injected into xray's JSON config on every connect.

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
| `user-agent` | `whoisthat/v0.7.0` | User-configurable (Settings) |

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

`test-result`: `>0` = latency in ms, `-1` = failed, `-2` = testing, `0` = untested.

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
| `v` | Toggle TUN mode (auto-setup via pkexec on first run) |

### Profiles & Groups

| Key | Action |
| --- | --- |
| `a` | Import profile URI (clipboard or manual input — vless://, vmess://, trojan://, ss://, socks://, hysteria2://, hy2://) |
| `x` | Delete selected profile |
| `X` | Delete current group (with confirmation) |
| `e` | Edit group (name + subscription URL) |
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
| Test method | tcp/http-get/http-head | Latency test method (tcp = direct dial, http = via SOCKS5 proxy) |
| TUN name | editable text | TUN interface name (1-15 chars, letters/digits/underscore/dash, default `whoisthattun`) |
| Kill Switch | on/off | Block all non-VPN traffic on connection drop (dedicated firewall table, works in both SOCKS and TUN modes) |
| HWID: Enabled | on/off | Send HWID headers with subscription requests |
| HWID | 1fb1e0141ab3e35a | Device identifier (read-only, auto-generated) |
| Reset HWID | ⏎ | Generate a new random HWID |
| User-Agent | whoisthat/v0.7.0 | User-Agent header (editable — press Enter to modify) |

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

### Subscription Workflow

1. Press `U` to add a new group — enter a name and subscription URL
2. Press `u` with the group selected to fetch and parse profiles from the URL (sends HWID headers if enabled)
3. The details panel shows subscription metadata when available (traffic used, expiry, last updated)
4. Press `e` to edit the group name or subscription URL at any time
5. Press `X` to delete the entire group and its profiles

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

## File Structure

```text
whoisthat/
├── Cargo.toml          ← Rust project manifest
├── src/                ← Rust TUI source
│   ├── main.rs         ← Entry point, event loop, autoconnect, systemd setup
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
├── install.sh          ← Universal installer script
├── parser/             ← URI → Xray JSON parser (Rust)
│   ├── Cargo.toml
│   └── src/
├── core/               ← WhoisThat Core (Go VPN engine)
│   └── core/
│       ├── main.go     ← Daemon entry point
│       ├── commands/   ← TCP command handlers
│       ├── db/         ← JSON file-based profile DB
│       ├── lib/        ← Core libraries
│       │   ├── logger/     ← Structured logger ([INFO]/[WARN]/[ERRO])
│       │   ├── TCPServer/  ← TCP server + dispatcher
│       │   ├── AppConfig/  ← Core configuration
│       │   ├── PortPool/   ← Dynamic port allocator
│       │   └── proxy/      ← Xray wrapper, TUN manager, tun2socks
│       ├── structs/    ← Shared data types
│       └── utils/      ← Binary detection, DNS resolution, capabilities
└── .gitignore
```

---

## Credits

Powered by [Xray-core](https://github.com/XTLS/Xray-core), [tun2socks](https://github.com/xjasonlyu/tun2socks).

---

## License

MIT — see [LICENSE](LICENSE).
