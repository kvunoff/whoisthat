---
title: "Kill-Switch Firewall Protection"
description: "How the independent nftables kill-switch blocks traffic when the VPN drops."
audience: "user"
type: "how-to"
---

# Kill-Switch Firewall Protection

The **Kill-Switch** prevents accidental data and IP leaks by automatically blocking all outbound network traffic if the VPN connection drops unexpectedly.

---

## How It Works

When Kill-Switch is active, WhoisThat installs dedicated kernel firewall rules:

* **nftables backend**: a single `inet whoisthat_ks` table covering IPv4 and IPv6.
* **iptables fallback**: `WHOISTHAT_KS` chains in the `ip` and `ip6` `filter` tables, inserted at the top of `OUTPUT`.

The allowlist is rebuilt in one transaction right before Xray starts, using the
exact server IPs that Xray is about to use. The block itself is never removed
for the duration of a connect, so there is no unprotected window while a
profile is (re)established.

```
                    Outbound Packet
                          │
                          ▼
            Loopback interface / 127.0.0.0/8 / ::1?
                          ├─► YES: Allow
            Private LAN, link-local, multicast?
                          ├─► YES: Allow
            Configured DNS servers / TUN DNS sink?
                          ├─► YES: Allow (resolution, tunnel DNS)
            Socket carries fwmark 1 (direct outbound, split-exclude,
            daemon-owned downloads)?
                          ├─► YES: Allow
            Leaving through the TUN adapter?
                          ├─► YES: Allow
            Destination is the active VPN Server IP?
                          ├─► YES: Allow (needed for handshake/reconnect)
                          │
                          ▼
                Drop Packet (Unconditionally Blocked)
```

### Lifecycle: enabled vs armed

`kill-switch-enabled` is the user's **intent**. The daemon keeps the block
installed while it is alive, and reports the actual kernel state separately:

* `kill_switch_armed: true` — the rules are installed and verified.
* `kill_switch_armed: false` — the switch is requested but the rules could not
  be installed (or could not be inspected). The TUI shows
  `Kill switch: ▲ on (not armed)` and `whoisthat doctor` reports it.

The block is installed at startup (after the orphaned-rule reconcile), when
the switch is enabled while disconnected, and refreshed before every Xray
start. It survives disconnect, an Xray crash and an explicit `core stop`; it is
removed only by `kill-switch=false`.

### Daemon-owned downloads

Runtime (Xray), geo databases and subscription updates use a dedicated HTTP
transport whose sockets carry `SO_MARK=1` (the same direct mark as
split-exclude). They keep working while the block is armed without opening
anything for user traffic. The TUI's "show public IP" check is not marked and
does not work while the block is armed.

### Incompatibility with include-mode split tunneling

Kill-Switch is **not compatible with `split-tunnel: include`**. Include mode deliberately routes only the apps in the split slice through the VPN and lets everything else use the physical interface; a system-wide kill-switch would block exactly that "everything else". The core rejects enabling either feature while the other is active (and normalizes a hand-edited config to `kill-switch: false`), so the UI cannot end up in a state where it reports a working split tunnel while all other traffic is silently dropped.

---

## Key Security Characteristics

### 1. Independent Table Isolation
Unlike traditional scripts that append rules to your existing firewall chains, WhoisThat uses **dedicated nftables tables**. This ensures:
* It will never overwrite, conflict with, or break your existing `ufw`, `firewalld`, or custom Docker firewall rules.
* Works seamlessly in both **Proxy mode** and **TUN mode**.

### 2. Whitelisted Traffic
To prevent breaking local machine communication, the Kill-Switch automatically permits:
* **Loopback (`127.0.0.0/8`, `::1`)**: Local inter-process communication.
* **LAN Subnets (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `169.254.0.0/16`, `fc00::/7`, `fe80::/10`)**: Access to local printers, routers, NAS devices, mDNS and NDP.
* **Multicast (`224.0.0.0/4`, `ff00::/8`)**: Local service discovery.
* **Configured DNS servers**: The resolvers from the app configuration, so name resolution keeps working while the tunnel is being established. The reserved TUN DNS sink (`198.18.0.53`, `fd00::53`) is allowed as well.
* **Direct-marked sockets (`fwmark 1`)**: Xray's `freedom` outbound for `DirectSites`/`DirectIp` and apps launched via `whoisthat run` in exclude mode. Setting `SO_MARK` requires `CAP_NET_ADMIN`, which only `whoisthat-core` holds in the rootless setup.
* **TUN Interface**: Traffic encapsulated by the tunnel. Once the interface is removed on disconnect the rule stops matching, so the fail-closed behaviour is preserved.
* **VPN Server IP**: The exact IPs of the active proxy server, allowing automatic reconnection attempts.

---

## Toggling the Kill-Switch

### In the TUI
1. Open the Settings tab (<kbd>5</kbd>).
2. Navigate to **Kill Switch** and press <kbd>Enter</kbd> or <kbd>Space</kbd> to toggle `on` / `off`.

Enabling the Kill-Switch while connected arms the firewall immediately; there is
no need to reconnect.

### Via CLI
```bash
# Toggle Kill-Switch state
whoisthat settings toggle kill-switch
whoisthat -kt                       # legacy alias

# Explicitly enable / disable
whoisthat settings set kill-switch=true
whoisthat settings set kill-switch=false

# Force-disable when the firewall state cannot be verified
# (broken nft, lost capabilities): removes the rules best-effort
whoisthat settings set kill-switch=false --force

# Read the persisted intent and the actual kernel state
whoisthat settings get kill-switch
whoisthat settings get kill-switch-armed

# Legacy aliases
whoisthat --killswitch-on
whoisthat --killswitch-off
```

`settings set` validates the value before contacting the daemon and reports the value the daemon
actually ended up with. The daemon replies with both `enabled` and `armed`;
`kill-switch=true armed=false` means the intent is stored but traffic is not
currently protected (see `core.log` and `whoisthat doctor`).

### Stopping the daemon

The rules deliberately survive `whoisthat core stop` and a systemd unit stop:
stopping the client must not silently expose the machine. Use
`whoisthat settings set kill-switch=false` before stopping, or remove the table
manually (see below). The uninstaller removes it as part of `install.sh --uninstall`.

**Not covered:** only locally-generated traffic (`OUTPUT`) is filtered. Forwarded
traffic — Docker containers, virtual machines, NAT on this host — is not
blocked by the kill-switch.

---

## Self-Healing & Orphaned Rules Cleanup

If your machine experiences a sudden power loss, kernel panic, or hard reboot while the Kill-Switch is active, the firewall block would normally persist, leaving your internet disconnected.

WhoisThat handles this automatically:
* **On Every Startup**: the daemon inspects whether another core process is running. If not, it runs `tunmode.ReconcileOrphanedRules()` to tear down lingering `whoisthat_*` tables, and then **re-arms the kill-switch** if `kill-switch-enabled` is still true.
* **Manual Recovery (if ever needed)**:
  ```bash
  sudo nft delete table inet whoisthat_ks 2>/dev/null
  ```
