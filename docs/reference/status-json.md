---
title: "Status JSON"
description: "Stable JSON and NDJSON contract for `whoisthat status`."
audience: "integrator"
type: "reference"
---

# Status JSON

`whoisthat status --json` emits one object. `whoisthat status --watch --json`
emits one object per line (NDJSON) on traffic ticks and state changes. The fields
below are the stable contract for widgets and scripts.

```json
{
  "core_running": true,
  "connected": true,
  "status": "connected",
  "mode": "tun",
  "tun_enabled": true,
  "group_id": 1,
  "group_name": "Premium",
  "profile_id": 4,
  "profile_name": "Frankfurt-10G",
  "protocol": "vless",
  "xray_version": "v26.10.31",
  "rx_speed_bytes": 4823449,
  "tx_speed_bytes": 524288,
  "rx_speed_human": "4.6 MB/s",
  "tx_speed_human": "512.0 KB/s"
}
```

| Field | Type | Notes |
| :--- | :--- | :--- |
| `core_running` | bool | The core socket is reachable |
| `connected` | bool | A VPN session is active |
| `status` | string | `connected`, `disconnected`, `offline`, or a transition state |
| `mode` | string | `tun`, `proxy`, or empty when disconnected |
| `tun_enabled` | bool | TUN mode is on |
| `group_id`, `group_name` | int, string | Omitted when no profile is active |
| `profile_id`, `profile_name` | int, string | Omitted when no profile is active |
| `protocol` | string | Omitted when unknown |
| `xray_version` | string | Omitted when unknown |
| `rx_speed_bytes`, `tx_speed_bytes` | int | Raw bytes per second |
| `rx_speed_human`, `tx_speed_human` | string | Preformatted rates |

The object is not shaped for Waybar/Polybar directly: those expect `text`,
`alt`, `tooltip` and `class`. Use a wrapper, as shown in
[Desktop Status Widgets](/integrations/desktop-widgets).

## Smaller objects

`whoisthat mode status --json` returns the mode view; `whoisthat core status
--json` returns daemon health. They are for widgets that need only that slice.

## See also

- [CLI Reference](/reference/cli)
- [Desktop Status Widgets](/integrations/desktop-widgets)
- [IPC & Wire Protocol](/reference/protocol)
