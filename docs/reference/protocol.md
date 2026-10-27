# TCP & IPC Wire Protocol Reference

This document provides a complete technical specification for the length-prefixed JSON protocol utilized by `whoisthat-core`.

---

## Wire Format

```
┌──────────────┬───────────────────────────┐
│ 4 bytes (BE) │ JSON payload              │
│ uint32 len   │ {"msg":"...","data":{...}}│
└──────────────┴───────────────────────────┘
```

* **Length Header**: Exactly 4 bytes containing an unsigned 32-bit integer (`uint32`) in big-endian (network) byte order.
* **Payload**: UTF-8 encoded JSON object.
* **Socket Endpoints**:
  * **Unix Domain Socket (Default)**: `$XDG_RUNTIME_DIR/whoisthat/core.sock` (mode `0600`, owned by the invoking user).
  * **Loopback TCP (Opt-in)**: `127.0.0.1:4897` (when `tcp-enabled: true` in `config.json`).

---

## Client Commands (Client → Core)

| Command (`msg`) | `data` Parameter Object | Expected Response / Notification |
| :--- | :--- | :--- |
| `get-application-state` | `{}` | `application-state` |
| `connect` | `{"profile": {"id": int, "group_id": int}}` | `status-changed` |
| `disconnect` | `{}` | `status-changed` |
| `add-profiles` | `{"uris": "string", "group_id": int}` | `profiles-added` |
| `delete-profiles` | `{"profiles": [{"id": int, "group_id": int}]}` | `profiles-deleted` |
| `test-profile` | `{"profile": {"id": int, "group_id": int}, "method": "string"}` | `profile-updated` |
| `enable-tun` | `{}` | `tun-status-changed` |
| `disable-tun` | `{}` | `tun-status-changed` |
| `is-root` | `{}` | `is-root-answer` |
| `update-profile` | `{"Profile": {"id": int, "group_id": int}, "Name": "string"}` | `profile-updated` |
| `update-group` | `{"id": int, "name": "string", "subscription_url": "string"}` | `group-updated` |
| `add-group` | `{"name": "string", "subscription_url": "string"}` | `group-added` |
| `delete-group` | `{"id": int}` | `group-deleted` |
| `update-subscription` | `{"group_id": int}` | `subscription-updated` |
| `set-tun-name` | `{"tun_name": "string"}` | `tun-name-updated` |
| `set-hwid` | `{"enabled": bool, "user_agent": "string", "reset": bool}` | `hwid-updated` |
| `get-routing` | `{}` | `routing-updated` |
| `update-routing` | `{"config": { ... }}` | `routing-updated` |
| `die` | `{}` | Terminates the core daemon cleanly. |
| `set-kill-switch` | `{"enabled": bool}` | `kill-switch-updated` |
| `set-split-tunnel` | `{"mode": "off" \| "exclude" \| "include"}` | `split-tunnel-updated` |
| `set-autoconnect` | `{"enabled": bool, "group_id": int, "profile_id": int, "mode": "proxy" \| "tun"}` | `autoconnect-updated` |
| `test-group` | `{"group_id": int, "method": "string"}` | `test-progress` & `profile-updated` |
| `cancel-tests` | `{}` | Cancels all active and pending latency tests. |
| `set-test-config` | `{"method": "str", "samples": int, "concurrency": int, "timeout": int, "endpoint": "str", "auto_test": bool}` | `test-config-updated` |
| `import-happ-routing` | `{"group_id": int, "link": "string"}` | `subscription-updated` or `warn` |
| `set-group-routing-mode` | `{"group_id": int, "mode": "auto" \| "manual" \| "subscription"}` | `group-updated` |
| `set-active-routing-profile` | `{"group_id": int, "profile_id": "string"}` | `group-updated` & live hot-reload |
| `delete-routing-profile` | `{"group_id": int, "profile_id": "string"}` | `subscription-updated` |

---

## Core Notifications (Core → All Clients)

| Notification (`msg`) | `data` Object Payload | Description |
| :--- | :--- | :--- |
| `application-state` | Complete system state | Emitted on client handshake. Includes groups, profiles, active connection, TUN status, and configurations. |
| `status-changed` | `{"connection": "connected" \| "disconnected", "profile": { ... }}` | Fired when VPN connects or disconnects. |
| `profiles-added` | `{"profiles": [ ... ]}` | Fired when new server profiles are imported. |
| `profiles-deleted` | `{"deleted-profiles": [ ... ]}` | Fired when profiles are deleted. |
| `profile-updated` | `{"profile": { ... }}` | Fired when a profile is renamed or completes a latency test. |
| `group-added` | `{"id": int, "name": "string", "subscription_url": "string"}` | Fired when a new group is created. |
| `group-deleted` | `{"id": int}` | Fired when a group is deleted. |
| `group-updated` | Full Group structure | Fired when a group's name or URL is modified. |
| `subscription-updated` | `{"group_id": int, "group": { ... }, "profiles": [ ... ], "routing_profiles": [ ... ]}` | Fired when a subscription finishes downloading and parsing. |
| `tun-status-changed` | `{"is_enabled": bool}` | Fired when TUN mode is toggled. |
| `tun-name-updated` | `{"tun_name": "string"}` | Fired when the TUN interface name changes. |
| `is-root-answer` | `{"IsRoot": bool}` | Response indicating whether the daemon holds root privileges. |
| `hwid-updated` | `{"enabled": bool, "hwid": "str", "user_agent": "str", ...}` | Fired when HWID parameters are changed or reset. |
| `traffic-stats` | `{"proxy_up": int, "proxy_down": int, "direct_up": int, "direct_down": int}` | Broadcast every 1 second with real-time transfer counters. |
| `routing-updated` | `{"config": { ... }}` | Fired when routing rules or active Happ profiles change. |
| `warn` | `{"key": "string", "content": "string"}` | Warning or actionable hint (e.g. missing binary, device limits). |
| `kill-switch-updated` | `{"enabled": bool}` | Fired when Kill-Switch state changes. |
| `split-tunnel-updated` | `{"mode": "off" \| "exclude" \| "include"}` | Fired when split-tunnel mode changes. |
| `autoconnect-updated` | `{"enabled": bool, "mode": "proxy" \| "tun"}` | Fired when autoconnect parameters change. |
| `test-progress` | `{"current": int, "total": int}` | Broadcast during batch latency scans. |
| `test-config-updated` | Full TestConfig object | Fired when test timeout, samples, or concurrency are adjusted. |

---

## Core Data Structures

### Profile Structure
```json
{
  "id": 1,
  "group_id": 0,
  "nano-id": "abc123xyz",
  "name": "Frankfurt VLESS",
  "protocol": "vless",
  "uri": "vless://...",
  "address": "198.51.100.1",
  "host": "vpn.example.com",
  "test-result": 42,
  "tested_at": 1712345678,
  "loss-pct": 0,
  "jitter-ms": 3
}
```

#### Latency Testing Metrics:
* **`test-result`**:
  * `> 0`: Median round-trip latency in milliseconds across the configured sample count.
  * `0`: Untested server profile.
  * `-1`: Latency test failed / host unreachable.
  * `-2`: Test is currently in-flight.
* **`tested_at`**: Unix epoch timestamp marking when the last successful test completed.
* **`loss-pct`**: Percentage of failed requests during the multi-sample test batch ($0\%$ to $100\%$).
* **`jitter-ms`**: Difference between the maximum and minimum recorded latency among successful samples ($\Delta t = t_{\max} - t_{\min}$).

### Group Structure
```json
{
  "id": 1,
  "name": "Premium Subscription",
  "subscription_url": "https://provider.com/sub/token",
  "last_id": 48,
  "sub_last_updated": 1712345678,
  "sub_expires": 1720000000,
  "sub_upload": 1048576,
  "sub_download": 52428800,
  "sub_total": 107374182400
}
```

Subscription metadata (`sub_*`) is automatically populated from the standard `subscription-userinfo` HTTP response header.

### Routing Profile Info Structure

`routing_profiles` entries (in `application-state`, `subscription-updated`, and `group-updated`) contain the compiled profile summary plus the DNS display metadata:

```json
{
  "id": "c1f7b82e9a",
  "name": "Provider Full Bypass",
  "state": "ready",
  "last_error": "",
  "active": true,
  "rules_count": 42,
  "global_proxy": true,
  "domain_strategy": "IPIfNonMatch",
  "fake_dns_enabled": false,
  "remote_dns": "DoH cloudflare-dns.com",
  "domestic_dns": "DoU 8.8.8.8",
  "last_updated": 1712345678,
  "geo_updated_at": 1712345678
}
```

* **`fake_dns_enabled`**: the provider's FakeDNS preference. It is parsed and stored; FakeDNS allocation is not active yet.
* **`remote_dns` / `domestic_dns`**: short display labels derived from the profile (`DoH host`, `DoU address`). Empty when the profile has no usable DNS server.
* These fields are additive: older clients can ignore them, and `ProtocolVersion` is unchanged.

---

## Practical Client Implementation Example

Here is how you can communicate with `whoisthat-core` over its Unix domain socket using Python:

```python
import socket
import struct
import json
import os

# Resolve default socket path
runtime_dir = os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
socket_path = os.path.join(runtime_dir, "whoisthat", "core.sock")

# Connect to the core
sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.connect(socket_path)

def send_command(msg_type: str, data: dict = None):
    payload = json.dumps({"msg": msg_type, "data": data or {}}).encode("utf-8")
    header = struct.pack(">I", len(payload))
    sock.sendall(header + payload)

def read_message():
    raw_len = sock.recv(4)
    if not raw_len:
        return None
    length = struct.unpack(">I", raw_len)[0]
    payload = b""
    while len(payload) < length:
        chunk = sock.recv(length - len(payload))
        if not chunk:
            break
        payload += chunk
    return json.loads(payload.decode("utf-8"))

# Request application state
send_command("get-application-state")
state = read_message()
print(f"Connected to WhoisThat Core v{state['data']['core_version']}")

# Stream notifications
while True:
    notification = read_message()
    if notification and notification.get("msg") == "traffic-stats":
        stats = notification["data"]
        print(f"Traffic: ↓ {stats['proxy_down']} B/s ↑ {stats['proxy_up']} B/s")
```
