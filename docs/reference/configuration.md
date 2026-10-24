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

# Connect to last used profile automatically when TUI starts
autoconnect = false

# Last focused group and profile ID for UI state restoration
last_group_id = 0
last_profile_id = 0

# Display public IP address in the header
show_ip = true

# Active color palette: tokyo-night, catppuccin-mocha, nord, dracula,
# gruvbox-dark, solarized-dark, cyberpunk, monokai-pro
theme = "tokyo-night"

# Enable Rust TUI debug file logging (~/.local/share/whoisthat/tui.log)
log_enabled = false

# Minimum log severity threshold: error, warn, info, debug, trace
log_level = "warn"

# Latency test method: tcp, http-get, http-head
test_method = "http-get"

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
  "user-agent": "whoisthat-core/v0.2.0",
  "kill-switch-enabled": false,
  "autoconnect-enabled": false,
  "autoconnect-group-id": 0,
  "autoconnect-profile-id": 0,
  "autoconnect-mode": "proxy",
  "split-tunnel": {
    "mode": "off"
  },
  "ipc-socket-path": "",
  "tcp-enabled": false
}
```

### Key Parameter Explanations:
* **`dns-servers`**:
  * Used for resolving remote proxy hostnames (queries all servers, merges answers).
  * Injected into Xray's `freedom` (direct) outbound domain resolver.
  * In TUN mode, the **first IP** in this array is targeted for kernel-level DNS hijacking.
* **`test-port-range`**: Ephemeral port boundary leased dynamically for concurrent latency test instances.
* **`split-tunnel.mode`**: Can be `"off"`, `"exclude"`, or `"include"`.

---

## 3. Data & Cache Storage Directories

| Directory Path | Content & Purpose | Permissions |
| :--- | :--- | :---: |
| `~/.local/share/whoisthat/db/` | AES-256-GCM encrypted database files (`groups.json`, `profiles.json`, `routing.json`). | `0700` |
| `~/.local/share/whoisthat/db/.key` | 32-byte cryptographic master key. | `0600` |
| `~/.local/share/whoisthat/runtimes/xray/` | Pinned official Xray-core binary (v26.9.9). | `0755` |
| `~/.config/whoisthat/geo/` | Global routing databases (`geoip.dat`, `geosite.dat`). | `0755` |
| `~/.local/share/whoisthat/geo/groups/` | Isolated Happ routing profile geo assets. | `0700` |
| `$XDG_RUNTIME_DIR/whoisthat/core.sock` | Active Unix Domain Socket IPC endpoint. | `0600` |
