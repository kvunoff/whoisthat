---
title: "Kill-Switch"
description: "Block non-VPN traffic when the connection drops, and understand enabled vs armed."
audience: "user"
type: "how-to"
---

# Kill-Switch

The **Kill-Switch** prevents accidental data and IP leaks by automatically
blocking all outbound network traffic if the VPN connection drops unexpectedly.
It works in both proxy and TUN mode.

The switch installs a dedicated `inet whoisthat_ks` nftables table (or
`WHOISTHAT_KS` chains on the iptables fallback). It does not append rules to
your existing firewall, so it does not conflict with `ufw`, `firewalld`, Docker,
or custom rules. The packet flow and allowlist are described in
[Networking internals](/architecture/networking#kill-switch).

## Enabled vs armed

`kill-switch-enabled` is the user's **intent**. The daemon reports the actual
kernel state separately:

- `kill_switch_armed: true` — the rules are installed and verified.
- `kill_switch_armed: false` — the switch is requested but the rules could not
  be installed or inspected. The TUI shows `Kill switch: ▲ on (not armed)` and
  `whoisthat doctor` reports it.

The block is installed at startup, when the switch is enabled while
disconnected, and refreshed before every Xray start. It survives disconnect, an
Xray crash, and an explicit `whoisthat core stop`; it is removed only by
`kill-switch=false`.

## Include-mode incompatibility

Kill-Switch is **not compatible with `split-tunnel: include`**. Include mode
deliberately routes only the apps in the split slice through the VPN and lets
everything else use the physical interface; a system-wide kill-switch would
block exactly that "everything else". The core rejects enabling either feature
while the other is active and normalizes a hand-edited config to
`kill-switch: false`.

## Toggle in the TUI

1. Open Settings (<kbd>5</kbd>).
2. Navigate to **Kill Switch** and press <kbd>Enter</kbd> or <kbd>Space</kbd> to
   toggle `on` / `off`.

Enabling the Kill-Switch while connected arms the firewall immediately; there is
no need to reconnect.

## Toggle from the CLI

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

`settings set` validates the value before contacting the daemon and reports the
value the daemon actually ended up with. `kill-switch=true armed=false` means
the intent is stored but traffic is not currently protected; check `core.log`
and `whoisthat doctor`.

## Stopping the daemon

The rules deliberately survive `whoisthat core stop` and a systemd unit stop:
stopping the client must not silently expose the machine. Use
`whoisthat settings set kill-switch=false` before stopping, or remove the table
manually:

```bash
sudo nft delete table inet whoisthat_ks 2>/dev/null
```

The uninstaller removes it as part of `install.sh --uninstall`.

**Not covered:** only locally-generated traffic (`OUTPUT`) is filtered.
Forwarded traffic — Docker containers, virtual machines, NAT on this host — is
not blocked by the kill-switch.

## Self-healing and orphaned rules

If the machine loses power or reboots while the Kill-Switch is active, the
firewall block could persist and leave the internet disconnected. On every
startup the daemon checks whether another core is running. If not, it runs
`tunmode.ReconcileOrphanedRules()` to tear down lingering `whoisthat_*` tables,
then re-arms the kill-switch if `kill-switch-enabled` is still true.

Manual recovery, if ever needed:

```bash
whoisthat core restart
# or
sudo nft delete table inet whoisthat_ks 2>/dev/null
```

## See also

- [Split Tunneling](/usage/split-tunnel)
- [Settings Reference](/usage/settings)
- [Networking internals](/architecture/networking)
- [Troubleshooting & FAQ](/reference/troubleshooting)
