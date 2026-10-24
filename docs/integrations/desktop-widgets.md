# Desktop Status Widgets (Waybar, Polybar)

WhoisThat is engineered for integration with Linux desktop status bars, tiling window managers, and shell panels (**Waybar**, **Polybar**, **i3status**, **dwmbars**, **Eww**).

---

## The JSON Status Schema

Running `whoisthat status --json` (or streaming with `whoisthat status --watch --json`) emits a structured JSON object:

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
  "rx_speed_bytes": 4823449,
  "tx_speed_bytes": 524288,
  "rx_speed_human": "4.6 MB/s",
  "tx_speed_human": "512.0 KB/s"
}
```

---

## 1. Waybar Integration

Waybar can consume the continuous stream natively via `custom/whoisthat`.

### `~/.config/waybar/config`

```json
"custom/whoisthat": {
    "exec": "whoisthat status --watch --json",
    "return-type": "json",
    "format": "{icon} {}",
    "format-icons": {
        "connected": "🔒",
        "disconnected": "🔓",
        "offline": "⚠️"
    },
    "on-click": "whoisthat -t",
    "on-click-right": "whoisthat -mt",
    "tooltip": true
}
```

### Script Formatter (Optional Helper)
To customize Waybar's JSON output (e.g. mapping `rx_speed_human` into `text` and `tooltip`):

```bash
#!/usr/bin/env bash
# ~/.config/waybar/scripts/whoisthat-waybar.sh
whoisthat status --watch --json | while read -r line; do
    connected=$(echo "$line" | jq -r '.connected')
    mode=$(echo "$line" | jq -r '.mode')
    name=$(echo "$line" | jq -r '.profile_name // "None"')
    rx=$(echo "$line" | jq -r '.rx_speed_human')
    tx=$(echo "$line" | jq -r '.tx_speed_human')

    if [ "$connected" = "true" ]; then
        alt="connected"
        text="[${mode^^}] $name ↓ $rx ↑ $tx"
        tooltip="Profile: $name\nMode: $mode\nDownload: $rx\nUpload: $tx"
        class="connected"
    else
        alt="disconnected"
        text="Disconnected"
        tooltip="VPN is disconnected. Click to connect."
        class="disconnected"
    fi

    printf '{"text":"%s","alt":"%s","tooltip":"%s","class":"%s"}\n' \
        "$text" "$alt" "$tooltip" "$class"
done
```

---

## 2. Polybar Integration

### `~/.config/polybar/config.ini`

```ini
[module/whoisthat]
type = custom/script
exec = whoisthat status --short
interval = 1
click-left = whoisthat -t
click-right = whoisthat -mt
format-prefix = "󰖂 "
format-prefix-foreground = ${colors.primary}
```

---

## 3. Compact Status String (`status --short`)

For simple status scripts that do not parse JSON, `whoisthat status --short` provides clean pre-formatted outputs:

* **Connected via TUN**: `[Connected] [TUN] ↓ 2.4 MB/s ↑ 380.0 KB/s`
* **Connected via Proxy**: `[Connected] [Proxy] ↓ 1.1 MB/s ↑ 120.0 KB/s`
* **Disconnected**: `[Disconnected]`
* **Core Daemon Offline**: `[Offline]`
