---
title: "Recipes & Common Setups"
description: "Copy-paste configurations for headless, proxy-only, split-tunnel, and Docker setups."
audience: "user"
type: "how-to"
---

# Recipes & Common Setups

Short, copy-pasteable configurations for the situations that come up most often. Each recipe links to the deep-dive page instead of repeating it.

---

## 1. Headless Server (No GUI)

The CLI and the daemon do not need a terminal UI. Install normally, then run the core as a systemd user service:

```bash
whoisthat systemd enable                # writes and enables whoisthat-core.service
sudo loginctl enable-linger "$USER"     # start at boot, before login
systemctl --user start whoisthat-core
whoisthat doctor
```

Connect and make it survive a reboot:

```bash
whoisthat profile list
whoisthat settings set autoconnect-profile="Vienna" autoconnect=true autoconnect-mode=tun
whoisthat settings set kill-switch=true
whoisthat status --short
```

- `autoconnect-profile` must be set **before** `autoconnect=true`; the core refuses to arm autoconnect without a target.
- `autoconnect-mode=tun` needs the file capabilities; if `whoisthat doctor` reports them missing, re-run `setcap`.
- Enabling the unit does not start it immediately — run `systemctl --user start` once (or log out/in).
- For monitoring, `whoisthat status --watch --json` is the stable NDJSON stream.

See [Systemd User Service](/integrations/systemd) and [CLI](/integrations/cli).

---

## 2. Proxy-Only Desktop (No TUN)

If you only need a local proxy and do not want kernel-level routing:

```bash
whoisthat connect "Vienna"
whoisthat mode proxy
whoisthat status --short
```

Point applications at:

- SOCKS5: `127.0.0.1:3090`
- HTTP: `127.0.0.1:3091`

TUN capabilities are not required for this mode. `whoisthat mode tun` switches to the system-wide tunnel later; see [Quick Start](/guide/quick-start#5-choosing-your-connection-mode).

---

## 3. Per-App Bypass (Spotify, Games, Banking)

Exclude mode keeps the OS on the VPN and sends selected apps through the physical link:

```bash
whoisthat settings set split-tunnel=exclude
whoisthat mode tun
whoisthat run -d spotify
whoisthat run curl 2ip.io          # verify the public IP
```

Single-instance apps must be fully closed first, otherwise the new process just signals the existing instance outside the slice:

```bash
pkill -9 spotify
whoisthat run -d spotify
```

For Steam, use the launch option `whoisthat run %command%` **without** `-d`, and prefix wrappers (`gamemoderun`, `mangohud`, `gamescope`) inside it. Full details: [Split Tunneling](/networking/split-tunnel#gaming-steam-integration).

---

## 4. Docker Host and Local Servers

TUN mode does not break inbound LAN or Docker-published ports: reply packets are matched by conntrack and marked so they return through the physical interface instead of entering the tunnel.

What to expect:

- `-p 8080:80` containers stay reachable from the LAN while the VPN is connected;
- loopback and private ranges are always allowed by the kill-switch;
- split tunneling is **process-based** (`whoisthat run`); container processes live in Docker's own cgroups, so there is no per-container split policy. Container traffic follows the host's TUN and bridge routing.

See [TUN Mode & Conntrack](/networking/tun-mode#incoming-connections-server-hosting-conntrack).

---

## 5. Several Subscriptions with Different Routing

Each group can use its own Happ profile or manual rules.

```bash
whoisthat sub add 'https://provider.example/sub/token' --name "Work"
whoisthat sub add 'https://provider.example/other/token' --name "Media"

whoisthat route mode Work manual          # ignore provider Happ profiles
whoisthat route mode Media subscription   # provider rules only, LAN-only fail-safe
whoisthat route add --type domain --value example.com --outbound direct --group Work
```

- `manual_routing_mode` (`inherit`/`custom`) selects Global rules or the group's own set; editing a rule on an inheriting group switches it to Custom.
- JSON-subscription groups always keep manual rules on top of the provider baseline and cannot switch modes.
- The exact order and fallbacks are in [Routing Precedence](/reference/routing-precedence).

---

## 6. Scheduled Refresh and Monitoring

```bash
whoisthat sub auto 12 on
whoisthat sub interval 12 24        # hours; "none" clears the override
whoisthat sub list                  # AUTO column: on 24h / on — / off
```

The daemon checks subscriptions every 10 minutes; background refreshes do not run latency tests or reload a live TUN session. Use `whoisthat test group Work --wait` when you want fresh numbers now.

---

## 7. Desktop Integration

```bash
whoisthat status --short             # Polybar/style scripts
whoisthat status --watch --json      # Waybar custom module
whoisthat link register              # wit:// handler + app-menu launcher
whoisthat link handle 'wit://toggle'
whoisthat settings set terminal=alacritty   # terminal for the app-menu launcher
```

Widgets and deep-link syntax are covered in [Desktop Widgets](/integrations/desktop-widgets) and [Deep Links](/integrations/deeplinks).
