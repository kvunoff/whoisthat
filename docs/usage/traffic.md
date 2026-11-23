---
title: "Real-Time Traffic Monitoring"
description: "Read the real-time traffic tab: rates, totals, and Braille graphs."
audience: "user"
type: "reference"
---

# Real-Time Traffic Monitoring

The Traffic Monitoring dashboard (<kbd>3</kbd>, or cycle with <kbd>[</kbd> / <kbd>]</kbd>) shows throughput, session totals and a Braille history chart. All numbers come from the core's `traffic-sample` snapshot; the TUI only renders them.

---

## High-Resolution Braille Charts

WhoisThat uses Unicode Braille dot patterns (`⠋`, `⠙`, `⠸`, etc.) to plot bandwidth curves inside your terminal:

```text
Bandwidth Monitor (RX: Green, TX: Red)
 12.4 MB/s ┼                     ⢠⣾⣿⣷⣄
            │                   ⢀⣼⣿⣿⣿⣿⣿⣦⡀
   8.2 MB/s ┼                 ⢠⣾⣿⣿⣿⣿⣿⣿⣿⣿⣷⡄
            │      ⢀⣀⡀   ⢠⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⡄
   4.1 MB/s ┼    ⢀⣴⣿⣿⣷⡄⢀⣼⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣦⡀
            │  ⢀⣴⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣦⡀
     0 B/s  ┴─────────────────────────────────────────────────────────────
```

- **Color Differentiation**: Download (**RX**) is green, upload (**TX**) is red.
- **Dynamic Y-Axis Scaling**: the scale follows recent peak rates, shifting between `B/s`, `KB/s`, `MB/s` and `GB/s`.
- **Time-Based Window**: the chart covers the last 60 seconds by sample timestamps. A missed frame shows as a gap instead of stretching the whole window.

---

## Metrics & Diagnostics Cards

### Live Throughput

- **Current Download Rate** and **Current Upload Rate** are bytes per second computed by the core over the real elapsed interval, not a raw difference between ticks.
- **Peak Speeds** are the highest rates seen in the current connection session. They reset when the connection is re-established.

### Session Data Totals

- **Proxy Download / Upload**: cumulative wire bytes through the proxy outbounds since the session started. Proxy bytes include the tunnel protocol's framing and encryption overhead, so they can be slightly larger than the payload an application sent or received.
- **Direct**: cumulative bytes through Xray's `freedom` outbounds (traffic matching `direct` routing rules). This works in both proxy and TUN mode.
- **Bypass**: cumulative bytes that left through the physical gateway without Xray, measured from firewall counters. These are Layer 3 bytes (IP headers included), so they are not directly comparable with proxy or direct bytes. Bypass is counted for split-tunnel `exclude` apps and for replies to externally initiated connections. It is unavailable in `include` mode, where the tab reports partial coverage.
- **Profile / Status**: current connection state and profile name.

The core owns the session totals and the session epoch. Reconnecting, switching profiles, reloading routing or toggling TUN starts a new epoch and resets the chart, totals and peaks.

### Interface & TUN Footer

- **TUN State**: `TUN: active (<name>)` or `TUN: disabled`.
- **Direct Bypass**: live and cumulative bytes for direct and bypass traffic combined.
- The firewall backend, MTU and packet counters are not part of the Traffic tab; use `whoisthat doctor` or `nft`/`ip` for those.

---

## What is not counted

DNS queries handled internally by Xray, blocked (`blackhole`) destinations and the traffic of the statistics service itself are excluded. Traffic forwarded by containers or virtual machines is not attributed to the VPN session.

## See also

- [IPC & Wire Protocol](/reference/protocol)
- [Split Tunneling](/usage/split-tunnel)
- [Routing Rules](/networking/routing-rules)
- [Status JSON](/reference/status-json)
