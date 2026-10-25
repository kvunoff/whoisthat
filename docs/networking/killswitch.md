# Kill-Switch Firewall Protection

The **Kill-Switch** prevents accidental data and IP leaks by automatically blocking all outbound network traffic if the VPN connection drops unexpectedly.

---

## How It Works

When Kill-Switch is active, WhoisThat installs dedicated kernel firewall tables:

* **IPv4 Table**: `inet whoisthat_ks`
* **IPv6 Table**: `ip6 whoisthat_ks`

```
                    Outbound Packet
                          │
                          ▼
            Is traffic to Local Loopback (lo)?
                          ├─► YES: Allow
                          │
            Is traffic to RFC1918 Private LAN?
                          ├─► YES: Allow
                          │
            Is traffic destined to active VPN Server IP?
                          ├─► YES: Allow (needed for handshake/reconnect)
                          │
            Is traffic traversing the TUN adapter?
                          ├─► YES: Allow
                          │
                          ▼
                Drop Packet (Unconditionally Blocked)
```

---

## Key Security Characteristics

### 1. Independent Table Isolation
Unlike traditional scripts that append rules to your existing firewall chains, WhoisThat uses **dedicated nftables tables**. This ensures:
* It will never overwrite, conflict with, or break your existing `ufw`, `firewalld`, or custom Docker firewall rules.
* Works seamlessly in both **Proxy mode** and **TUN mode**.

### 2. Whitelisted Traffic
To prevent breaking local machine communication, the Kill-Switch automatically permits:
* **Loopback (`127.0.0.0/8`, `::1`)**: Local inter-process communication.
* **LAN Subnets (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`)**: Access to local printers, routers, and NAS devices.
* **VPN Server IP**: The exact IP and port of the active proxy server, allowing automatic reconnection attempts.

---

## Toggling the Kill-Switch

### In the TUI
1. Open the Settings tab (<kbd>5</kbd>).
2. Navigate to **Kill Switch** and press <kbd>Enter</kbd> or <kbd>Space</kbd> to toggle `on` / `off`.

### Via CLI
```bash
# Toggle Kill-Switch state
whoisthat -kt

# Explicitly enable Kill-Switch
whoisthat --killswitch-on

# Explicitly disable Kill-Switch
whoisthat --killswitch-off
```

---

## Self-Healing & Orphaned Rules Cleanup

If your machine experiences a sudden power loss, kernel panic, or hard reboot while the Kill-Switch is active, the firewall block would normally persist, leaving your internet disconnected.

WhoisThat handles this automatically:
* **On Every Startup**: The daemon inspects whether another core process is running. If not, it runs `tunmode.ReconcileOrphanedRules()` to cleanly tear down lingering `whoisthat_ks` tables.
* **Manual Recovery (if ever needed)**:
  ```bash
  sudo nft delete table inet whoisthat_ks 2>/dev/null
  ```
