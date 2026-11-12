---
title: Introduction
description: Official documentation for WhoisThat - Modern Linux terminal-based VPN client with Rust TUI and Go core daemon.
audience: user
type: landing
---

<div style="display: flex; align-items: center; gap: 1.25rem; margin-top: 1rem; margin-bottom: 2rem;">
  <img src="/logo.svg" alt="WhoisThat Logo" width="72" height="72" style="border-radius: 16px; border: 1px solid var(--vp-c-border); background-color: var(--vp-c-bg-soft);" />
  <div>
    <h1 style="margin: 0; padding: 0; font-size: 2.2rem; font-weight: 800; border: none;">WhoisThat Documentation</h1>
    <p style="margin: 0.35rem 0 0; color: var(--vp-c-text-2); font-size: 1.05rem;">
      Modern terminal-based VPN client for Linux. Rust TUI and CLI, a Go core, and a pinned Xray-core runtime.
    </p>
  </div>
</div>

## What is WhoisThat?

WhoisThat is a Linux VPN client with a terminal interface and a long-running
core daemon. The Rust client owns the TUI and CLI; the Go core owns profiles,
subscriptions, the encrypted database, routing, TUN, the firewall and Xray. The
two processes talk over a length-prefixed JSON socket.

It is built for people who want a real TUN client without an Electron app, and
for scripts that need the same state through a stable CLI.

## Start here

- **[Installation](/guide/installation)** — script, packages, AUR, source.
- **[Quick Start](/guide/quick-start)** — launch, import, test, connect.
- **[CLI Reference](/reference/cli)** — every command, selector and exit code.
- **[Architecture](/architecture/overview)** — processes, IPC and the packet path.
- **[Troubleshooting](/reference/troubleshooting)** — symptoms and fixes.

The essential keys:

| Key | Action |
| :---: | :--- |
| <kbd>j</kbd> / <kbd>k</kbd> | Move selection |
| <kbd>Tab</kbd> | Switch tree/details focus |
| <kbd>c</kbd> / <kbd>Enter</kbd> | Connect |
| <kbd>d</kbd> | Disconnect |
| <kbd>v</kbd> | Toggle TUN/proxy |
| <kbd>t</kbd> / <kbd>T</kbd> | Test focused / all |
| <kbd>?</kbd> | Help |
| <kbd>q</kbd> / <kbd>Q</kbd> | Detach / quit |

## What it provides

- **Rootless TUN by default.** File capabilities
  (`cap_net_admin,cap_net_raw,cap_setpcap`) let the core manage the interface and
  routing without a setuid binary. A privileged fallback exists for hosts where
  capabilities are not available; it is documented in
  [TUN Mode & Capabilities](/networking/tun-mode).
- **One proxy engine.** VLESS (Reality, Vision, xHTTP, gRPC, WebSocket), VMess,
  Trojan, Shadowsocks (2022), SOCKS5 and Hysteria2 run on pinned Xray-core
  `v26.10.31`. See [Protocols](/networking/protocols).
- **Per-application split tunneling.** `whoisthat run` places apps in a cgroup
  and routes them inside or outside the tunnel. Set Split Tunnel to `exclude` or
  `include` first; with the default `off`, the command changes no routing. See
  [Split Tunneling](/usage/split-tunnel).
- **Routing and provider profiles.** Manual rules, Happ routing profiles and
  managed JSON subscriptions share one first-match engine. See
  [Routing](/usage/routing), [Happ](/networking/happ-routing) and
  [JSON Subscriptions](/networking/json-subscriptions).
- **Encrypted state at rest.** Profiles, tokens and routing data are stored with
  AES-256-GCM under `~/.local/share/whoisthat/db/`; the key lives separately in
  `~/.config/whoisthat/db.key` (`0600`). See
  [Security](/architecture/security).
- **Desktop and status integration.** `wit://` imports, Waybar/Polybar NDJSON,
  and the GNOME extension. See [Integrations](/integrations/desktop-widgets).

## Visual preview

<div style="display: flex; flex-direction: column; gap: 1.5rem; margin: 1.5rem 0;">
  <div>
    <h4 style="margin-bottom: 0.5rem; color: var(--vp-c-text-2);">Profiles, tree navigation and live details</h4>
    <img src="/screen1.png" alt="WhoisThat profiles, tree and details" style="border-radius: 8px; border: 1px solid var(--vp-c-divider);" />
  </div>
  <div>
    <h4 style="margin-bottom: 0.5rem; color: var(--vp-c-text-2);">Real-time traffic monitoring</h4>
    <img src="/screen2.png" alt="WhoisThat traffic dashboard" style="border-radius: 8px; border: 1px solid var(--vp-c-divider);" />
  </div>
</div>

::: info
The TUN interface is `whoisthattun` by default. The name is configurable in
Settings; the screenshots use a custom `tun0`.
:::

## Support the project

WhoisThat is built in spare time. Donations are optional:

| Network | Address |
| :--- | :--- |
| **Bitcoin (BTC)** | `bc1qutg32qasdyuuc03l9rtm3uxwncfjafqnhpnnec` |
| **Ethereum (ETH / ERC-20)** | `0x7C643C5b704ADD1888a0F68457cf6b0c5c3508C6` |
| **Tron (TRX / TRC-20)** | `TWSvpXfJsYcDVB9PEwVAgCUXT5RSdtxaYL` |
| **TON (GRAM / jettons)** | `UQAablx88aAbnjXTnU2t6LYoEHIEQeCv6wMqYZaqeoNjSb_x` |
| **Solana (SOL / SPL)** | `DckqFaa2PUikF3eQCt79vzGrg1xCM1j3uYyUNDSsswoX` |

Each address accepts the native coin and, where supported, stablecoins
(USDT/USDC). The Bitcoin address accepts BTC only. TON's native coin is Gram
(GRAM), formerly Toncoin (TON); the address is unchanged.

## See also

- [Installation](/guide/installation)
- [Quick Start](/guide/quick-start)
- [CLI Reference](/reference/cli)
- [Architecture Overview](/architecture/overview)
