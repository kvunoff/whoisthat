---
title: Introduction
description: Official documentation for WhoisThat - Modern Linux terminal-based VPN client with Rust TUI and Go core daemon.
---

<div style="display: flex; align-items: center; gap: 1.25rem; margin-top: 1rem; margin-bottom: 2rem;">
  <img src="/logo.svg" alt="WhoisThat Logo" width="72" height="72" style="border-radius: 16px; border: 1px solid var(--vp-c-border); background-color: var(--vp-c-bg-soft);" />
  <div>
    <h1 style="margin: 0; padding: 0; font-size: 2.2rem; font-weight: 800; border: none;">WhoisThat Documentation</h1>
    <p style="margin: 0.35rem 0 0; color: var(--vp-c-text-2); font-size: 1.05rem;">
      Modern terminal-based VPN client for Linux. Rust TUI frontend paired with an autonomous Go engine backed by Xray-core.
    </p>
  </div>
</div>

---

## What is WhoisThat?

**WhoisThat** is a modern, high-performance VPN client built specifically for Linux desktop and server environments. It bridges the gap between bulky Electron applications and complex bare command-line proxies by providing an ergonomic terminal user interface (TUI) backed by a long-running system daemon.

```
┌────────────────────────────────────────────────────────┐
│                      User Space                        │
│                                                        │
│  ┌────────────────────────┐    ┌────────────────────┐  │
│  │  whoisthat (Rust TUI)  │    │  CLI & Status API  │  │
│  │  ⋅ Ratatui Interface   │    │  ⋅ Waybar / i3     │  │
│  │  ⋅ Responsive Layout   │    │  ⋅ GNOME Extension │  │
│  └───────────┬────────────┘    └─────────┬──────────┘  │
│              │                           │             │
│              └─────────────┬─────────────┘             │
│                            │                           │
│           Unix Domain Socket / TCP Wire Protocol       │
│           $XDG_RUNTIME_DIR/whoisthat/core.sock         │
│                            │                           │
│                            ▼                           │
│  ┌──────────────────────────────────────────────────┐  │
│  │          whoisthat-core (Go Daemon)              │  │
│  │  ⋅ Encrypted DB (AES-256-GCM)                    │  │
│  │  ⋅ Subscription & HWID Engine                 │  │
│  │  ⋅ TUN & Route Orchestrator                      │  │
│  │  ⋅ Pinned Xray-core Manager (v26.9.9)            │  │
│  └─────────────────────────┬────────────────────────┘  │
└────────────────────────────┼───────────────────────────┘
                             │ Ambient File Capabilities
                             ▼
┌────────────────────────────────────────────────────────┐
│                     Linux Kernel                       │
│  ⋅ Virtual TUN Interface (`whoisthattun`)              │
│  ⋅ nftables / iptables DNAT & MASQUERADE               │
│  ⋅ cgroup v2 Socket Classification (Split Tunnel)      │
│  ⋅ Policy Routing Tables (Table 100 / Table 200)       │
└────────────────────────────────────────────────────────┘
```

---

## Core Pillars & Features

### 1. Rootless Least-Privilege Operation
WhoisThat creates and manages virtual TUN network adapters and policy routing tables without running as `root` or relying on dangerous setuid binaries. Using Linux file capabilities (`cap_net_admin,cap_net_raw,cap_setpcap=+ep`), ambient capabilities are promoted at runtime, keeping user credentials and proxy logic secure.

### 2. Unified Protocol Engine
All modern circumvention and proxy protocols are supported natively through a pinned build of **Xray-core (v26.9.9)**:
* **VLESS**: Reality (TLS certificate camouflage), Vision flow, xHTTP (SplitHTTP), gRPC, and WebSocket transports.
* **Hysteria2**: Native QUIC UDP protocol with Salamander obfuscation and UDP port hopping.
* **VMess**, **Trojan**, **Shadowsocks** (2022 specs), and **SOCKS5**.

### 3. Application-Based Split Tunneling
Bypass the VPN tunnel for gaming, domestic banking, or local network services using pure Linux kernel primitives (cgroup v2 and nftables policy routing). Set **Split tunnel** to `exclude` (apps bypass the VPN) or `include` (only these apps use it) in Settings first — with the default `off`, the commands below do not change routing:
```bash
whoisthat run -d spotify           # Spotify bypasses the VPN tunnel in background
whoisthat run %command%            # In Steam launch options: zero-ping direct gaming
whoisthat run curl 2ip.io          # Runs directly attached in terminal
```
See the [Split Tunneling & Steam Guide](/networking/split-tunnel#gaming-steam-integration) for technical details and setup.

### 4. Rich Routing & Happ Profiles
Define fine-grained routing policies based on domain names, IP subnets, protocols, GeoIP, and GeoSite databases. Ingest provider **Happ routing profiles** (`happ://routing/...`) with per-group routing modes (`auto`, `manual`, `subscription`).

### 5. Encrypted State at Rest
All subscription tokens, server configs, and routing rules are saved locally under `~/.local/share/whoisthat/db/` encrypted with **AES-256-GCM**. The master encryption key is generated automatically with permissions `0600`.

---

## Visual Preview

::: info
These screenshots were captured from an earlier release and show the legacy TUN name `tun0`; the current default is `whoisthattun` and the layout has evolved.
:::

<div style="display: flex; flex-direction: column; gap: 1.5rem; margin: 1.5rem 0;">
  <div>
    <h4 style="margin-bottom: 0.5rem; color: var(--vp-c-text-2);">Profiles, Tree Navigation & Live Details</h4>
    <img src="/screen1.png" alt="WhoisThat Profiles & Details" style="border-radius: 8px; border: 1px solid var(--vp-c-divider);" />
  </div>

  <div>
    <h4 style="margin-bottom: 0.5rem; color: var(--vp-c-text-2);">Real-time Traffic Monitoring & Braille Charts</h4>
    <img src="/screen2.png" alt="WhoisThat Traffic Dashboard" style="border-radius: 8px; border: 1px solid var(--vp-c-divider);" />
  </div>
</div>

---

## Documentation Roadmap

Get started exploring the documentation:

* **[Installation Guide](/guide/installation)** — Install via quick curl script, Arch Linux AUR (`paru`/`yay`), or build from source.
* **[Quick Start Tutorial](/guide/quick-start)** — First launch, importing subscriptions, latency testing, and connecting.
* **[Architecture Deep-Dive](/architecture/overview)** — Explore the Go daemon, IPC wire framing, and encryption layer.
* **[TUN Mode & Networking](/networking/tun-mode)** — Learn how rootless TUN, DNS hijacking, and Docker reply conntrack work.
* **[Split Tunneling](/networking/split-tunnel)** — Complete manual on running apps inside cgroup v2 transient slices.
* **[CLI & Desktop Widgets](/integrations/cli)** — Command-line flags, Waybar, Polybar, and the GNOME Shell extension.
* **[Wire Protocol Reference](/reference/protocol)** — Complete specification of IPC commands and streaming notifications.
