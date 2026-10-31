# Desktop Status Widgets (Waybar, Polybar)

WhoisThat is engineered for integration with Linux desktop status bars, tiling window managers, and shell panels (**Waybar**, **Polybar**, **i3status**, **dwmbars**, **Eww**).

---

## The JSON Status Schema

Running `whoisthat status --json` (or streaming with `whoisthat status --watch --json`) emits a structured JSON object.
`whoisthat mode status --json` and `whoisthat core status --json` provide smaller objects for widgets
that only need the mode or daemon health; the `status` contract below is unchanged. In watch mode a line is printed per `traffic-stats` tick (1/s while connected) plus on status/TUN/state changes; an idle disconnected client can stay silent. Optional fields (`group_id`, `profile_name`, `xray_version`, …) are omitted when empty.

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
  "xray_version": "v26.10.29",
  "rx_speed_bytes": 4823449,
  "tx_speed_bytes": 524288,
  "rx_speed_human": "4.6 MB/s",
  "tx_speed_human": "512.0 KB/s"
}
```

---

## 1. Waybar Integration

Waybar's `custom` module substitutes `{}`/`{icon}` only from the `text`/`alt`/`class` keys. Raw `status --json` deliberately exposes machine fields (`connected`, `mode`, `rx_speed_human`, …) instead, so use the wrapper script below.

### `~/.config/waybar/config`

```json
"custom/whoisthat": {
    "exec": "~/.config/waybar/scripts/whoisthat-waybar.sh",
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

### Wrapper Script
Maps the stable JSON fields into Waybar's `text` / `alt` / `tooltip` / `class` contract:

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
