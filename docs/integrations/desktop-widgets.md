---
title: "Desktop Status Widgets (Waybar, Polybar)"
description: "Waybar, Polybar, and NDJSON status output for desktop widgets."
audience: "integrator"
type: "how-to"
---

# Desktop Status Widgets (Waybar, Polybar)

WhoisThat is engineered for integration with Linux desktop status bars, tiling window managers, and shell panels (**Waybar**, **Polybar**, **i3status**, **dwmbars**, **Eww**).

---

The stable JSON contract lives in [Status JSON](/reference/status-json). This
page only shows how to adapt it to a status bar.

## Waybar

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
    "on-click": "whoisthat toggle",
    "on-click-right": "whoisthat mode toggle",
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

## Polybar

### `~/.config/polybar/config.ini`

```ini
[module/whoisthat]
type = custom/script
exec = whoisthat status --short
interval = 1
click-left = whoisthat toggle
click-right = whoisthat mode toggle
format-prefix = "󰖂 "
format-prefix-foreground = ${colors.primary}
```

---

## Compact status string (`status --short`)

For simple status scripts that do not parse JSON, `whoisthat status --short` provides clean pre-formatted outputs:

* **Connected via TUN**: `[Connected] [TUN] ↓ 2.4 MB/s ↑ 380.0 KB/s`
* **Connected via Proxy**: `[Connected] [Proxy] ↓ 1.1 MB/s ↑ 120.0 KB/s`
* **Disconnected**: `[Disconnected]`
* **Core Daemon Offline**: `[Offline]`

## See also

- [Status JSON](/reference/status-json)
- [CLI Reference](/reference/cli)
- [GNOME Shell Extension](/integrations/gnome-extension)
