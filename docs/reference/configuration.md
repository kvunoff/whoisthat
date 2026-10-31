# Configuration Files Reference

WhoisThat stores user preferences, daemon network rules, and encrypted credentials in standardized XDG configuration and data paths.

---

## 1. TUI Configuration (`config.toml`)

**Location**: `~/.config/whoisthat/config.toml`

Manages client-side settings, visual preferences, and IPC endpoint resolution:

```toml
# Port used when connecting via legacy TCP (default: 4897)
core_tcp_port = 4897

# Host for legacy TCP connection (default: "127.0.0.1")
core_host = "127.0.0.1"

# Legacy one-shot migration flag: promotes the old TUI autoconnect into the
# core's autoconnect-enabled setting. Runtime autoconnect lives in config.json.
autoconnect = false
autoconnect_migrated = false

# Last connected group/profile (used for autoconnect and CLI toggles)
last_group_id = 0
last_profile_id = 0

# Last seen core version from the IPC handshake (display fallback only)
core_version = "0.3.12"

# Display public IP address in the header
show_ip = true

# Active color palette: graphite-dark, graphite-light, tokyo-night, dracula
theme = "graphite-dark"

# Enable Rust TUI debug file logging (~/.local/share/whoisthat/tui.log)
log_enabled = false

# Minimum log severity threshold: error, warn, info, debug, trace
log_level = "warn"

# Latency test method: tcp, http-get, http-head
test_method = "http-get"

# Test engine parameters (mirrored to the core over IPC)
test_concurrency = 16
test_timeout_seconds = 5
test_samples = 3
test_endpoint = "https://cp.cloudflare.com/generate_204"
auto_test_on_subscribe = true

# Interface name for TUN adapter (mirrors core configuration)
tun_name = "whoisthattun"

# State of Kill-Switch firewall protection
kill_switch_enabled = false

# Custom Unix socket path (blank = default $XDG_RUNTIME_DIR/whoisthat/core.sock)
ipc_socket_path = ""

# Connect via legacy loopback TCP instead of Unix Domain Socket
use_tcp = false
```

---

## 2. Core Daemon Configuration (`config.json`)

**Location**: `~/.config/whoisthat/config.json`

Auto-generated and synchronized by the Go core daemon:

```json
{
  "socks-port": 3090,
  "http-port": 3091,
  "core-tcp-port": 4897,
  "test-port-range": {
    "start": 3095,
    "end": 30120
  },
  "dns-servers": [
    "1.1.1.1",
    "8.8.8.8",
    "2606:4700:4700::1111",
    "2001:4860:4860::8888"
  ],
  "tun-name": "whoisthattun",
  "hwid-enabled": true,
  "hwid": "1fb1e0141ab3e35a",
  "user-agent": "whoisthat/v0.3.12",
  "kill-switch-enabled": false,
  "autoconnect-enabled": false,
  "autoconnect-group-id": 0,
  "autoconnect-profile-id": 0,
  "autoconnect-mode": "proxy",
  "split-tunnel": {
    "mode": "off"
  },
  "test-config": {
    "concurrency": 16,
    "timeout-seconds": 5,
    "samples-per-test": 3,
    "test-endpoint": "https://cp.cloudflare.com/generate_204",
    "auto-test-on-subscribe": true
  },
  "tcp-enabled": false
}
```

`ipc-socket-path` is omitted while empty (`omitempty`) and only appears when a custom socket path is configured.

### Key Parameter Explanations:
* **`dns-servers`**:
  * Used for resolving remote proxy hostnames (queries configured servers sequentially and merges A/AAAA answers).
  * Injected into Xray's `freedom` (direct) outbound domain resolver when a Happ profile does not provide its own DNS section.
  * In TUN mode without Happ DNS (manual routing or profiles without DNS fields), the **first IPv4/IPv6** entries are targeted by the legacy kernel-level DNS hijack.
  * When the active Happ profile supplies DNS, its `RemoteDNS*` / `DomesticDNS*` servers take precedence; `dns-servers` are only used as a fallback. Happ DNS uses the reserved sink addresses `198.18.0.53` / `fd00::53` rather than the configured resolvers.
* **`test-port-range`**: Ephemeral port boundary leased dynamically for concurrent latency test instances.
* **`split-tunnel.mode`**: Can be `"off"`, `"exclude"`, or `"include"`.

### Setting keys via CLI

`whoisthat settings` is the single entry point for both files. Core-authoritative keys are read from
`application-state` (except `autoconnect-profile`, which lives in `config.json`) and written through
IPC; local keys are written to `config.toml`.

| CLI key | File / backend | Notes |
| :--- | :--- | :--- |
| `kill-switch`, `split-tunnel`, `autoconnect`, `autoconnect-mode` | core (`config.json`) | applied live; the daemon is authoritative |
| `autoconnect-profile` | core + `config.json` | target is stored as `autoconnect-group-id` / `autoconnect-profile-id` |
| `tun-name` | core (`config.json`) | applies on the next reconnect |
| `hwid-enabled`, `user-agent` | core (`config.json`) | subscription headers |
| `test.concurrency`, `test.timeout`, `test.samples`, `test.endpoint`, `test.auto-on-subscribe` | core (`config.json` → `test-config`) | sanitized by the daemon; the CLI re-reads state afterwards |
| `theme`, `show-ip`, `log-enabled`, `log-level`, `test-method` | TUI (`config.toml`) | |
| `core.host`, `core.port`, `core.use-tcp`, `core.socket` | TUI (`config.toml`) | affects the next CLI/TUI connection |

```bash
whoisthat settings list --json
whoisthat settings set split-tunnel=exclude kill-switch=true
whoisthat settings set theme=tokyo-night log-level=debug
```

---

## 3. Data & Cache Storage Directories

| Directory Path | Content & Purpose | Permissions |
| :--- | :--- | :---: |
| `~/.local/share/whoisthat/db/` | AES-256-GCM encrypted database: `config.json`, `routing.json`, and per-group directories (`groups/<id>/group_config.json`, profile files). | `0700` |
| `~/.local/share/whoisthat/backups/db-YYYYMMDD/` | Daily database snapshot (last three kept) used by **Repair WhoisThat** on the next start. | `0700` |
| `~/.config/whoisthat/config.json.bak` | Last known good copy of the daemon settings; `config.json` is never replaced by defaults when it cannot be parsed. | `0600` |
| `~/.local/share/whoisthat/db/routing.json` | Global manual routing rules shared by groups that inherit them. | `0600` |
| `~/.local/share/whoisthat/db/groups/<id>/routing/manual.json` | Custom manual routing rules for one group (`manual_routing_mode = "custom"`). | `0600` |
| `~/.config/whoisthat/db.key` | 32-byte cryptographic master key. | `0600` |
| `~/.local/share/whoisthat/runtimes/xray/v26.10.29/` | Pinned official Xray-core binary (versioned runtime directory). | `0755` |
| `~/.config/whoisthat/geo/` | Global routing databases (`geoip.dat`, `geosite.dat`). | `0755` |
| `~/.local/share/whoisthat/db/groups/<id>/routing/geo/<profile_id>/<version>/` | Isolated Happ routing profile geo assets. | `0700` |
| `$XDG_RUNTIME_DIR/whoisthat/core.sock` | Active Unix Domain Socket IPC endpoint. | `0600` |
| `$XDG_RUNTIME_DIR/whoisthat/ipc.token` | Shared secret required by the opt-in TCP IPC transport (64 hex chars). | `0600` |
