---
title: "Kernel Networking"
description: "How WhoisThat implements TUN routing, split tunneling, DNS hijacking, and the kill-switch."
audience: "developer"
type: "explanation"
---

# Kernel Networking

WhoisThat uses Linux kernel primitives instead of a root daemon or a network
namespace. This page describes the moving parts. For the user-facing behavior
see [TUN mode](/networking/tun-mode), [DNS](/networking/dns), [split
tunneling](/usage/split-tunnel), and the [kill-switch](/usage/kill-switch).

## TUN and policy routing

TUN mode creates `whoisthattun` (name configurable). The core adds the interface,
assigns `198.18.0.1` / `fd00::1`, and installs policy routing that sends normal
traffic into the TUN default route.

The direct outbound and split-exclude apps must not loop back into the tunnel.
They set `SO_MARK = 1`, and the kernel rule

```text
ip rule fwmark 1 table 100
```

routes marked packets through the physical gateway. Table 200 is the include
path: marked sockets (`fwmark 2`) are sent to the TUN interface. Postrouting NAT
masquerades the source address on physical egress so LAN replies survive.

When the physical interface has no IPv6 gateway, Table 100 installs an
`unreachable default` IPv6 route. Marked IPv6 attempts fail immediately and
fall back to IPv4 instead of leaking through the tunnel's IPv6 default.

The core learns the default interface and addresses at TUN start, then clears
and re-applies the rules on every reconnect. `TunModeManager` serializes
start, stop, DNS changes, and split re-application with one mutex.

## Split tunneling

Split mode places launched processes in a cgroup and marks their sockets:

| Host | cgroup path | nft match |
| :--- | :--- | :--- |
| systemd | `user.slice/user-<uid>.slice/user@<uid>.service/whoisthat_split.slice` | `socket cgroupv2 level 4` |
| non-systemd | `/sys/fs/cgroup/whoisthat-<uid>` | `socket cgroupv2 level 1` |

`whoisthat run` starts the app with `systemd-run --user` on systemd hosts, or
asks `whoisthat-runner` to fork it inside the per-user cgroup elsewhere. The
`inet whoisthat_split` table (output hook priority −150) matches the cgroup and
sets `fwmark 1` in `exclude` mode or `fwmark 2` in `include` mode. Policy routing
then selects Table 100 or Table 200.

Before adding a rule, the core checks that the cgroup directory exists. A rule
for a missing cgroup breaks the whole nft transaction, so an unavailable split
cgroup fails the TUN start with an actionable error instead of silently
disabling split. Changing the mode while connected re-applies the tables,
cgroup filters, nft rules, and DNS hijack without reconnecting.

If `whoisthat run` is executed under `sudo`/`pkexec`, the command refuses: the
application must run as the target user or the cgroup/UID assumptions break.

## DNS hijack

DNS interception depends on the routing mode:

- **Legacy mode** (manual routing or profiles without DNS fields): outbound
  port 53 is DNATed to the configured `dns-servers` and leaves through the
  physical gateway.
- **Happ mode**: port 53 is DNATed to the reserved sink `198.18.0.53` /
  `fd00::53`, which is not assigned to any interface and therefore follows the
  TUN default route. Xray receives the query on `tun-in`, routes it to `dns-out`,
  and the tagged domestic/remote resolvers decide the path.
- **FakeDNS** is enabled only when TUN and a global proxy are active and the
  split mode is not `include`. It maps domains to synthetic addresses from the
  reserved pool and never maps ICMP.
- **Split modes**: `exclude` keeps the Happ hijack; marked sockets return early
  and resolve directly. `include` disables the Happ hijack and FakeDNS because
  the system resolver lives outside the split slice. `off` keeps the normal
  TUN/Happ path.

Early returns in the nft rules keep the hijack away from `fwmark 1` sockets, the
sink itself, and the VPN server addresses. `resolvectl` points
systemd-resolved at the same sink. The full resolver matrix is in
[DNS & FakeDNS](/networking/dns).

## Kill-switch

The kill-switch is an independent outbound policy, not a TUN rule. It uses
`inet whoisthat_ks` (or `WHOISTHAT_KS` chains on the iptables fallback) and
allows only:

- loopback (`127.0.0.0/8`, `::1`);
- private LAN, link-local, and multicast ranges;
- configured DNS servers and the TUN DNS sink;
- sockets marked `fwmark 1` (direct outbound, split-exclude, daemon downloads);
- traffic leaving through the TUN adapter;
- the active VPN server IPs.

Everything else is dropped. The allowlist is rebuilt in one transaction right
before Xray starts, using the exact server IPs Xray is about to use. The block is
not removed for the duration of a connect, so there is no unprotected window
while a profile is re-established.

`kill-switch-enabled` is the intent; `kill_switch_armed` reports whether the
rules are installed. The block survives disconnect, an Xray crash, and
`whoisthat core stop`; it is removed only by `kill-switch=false`. Runtime, geo,
and subscription downloads use `SO_MARK=1`, so they keep working while the block
is armed.

On startup, if no other core is live, `ReconcileOrphanedRules()` removes
lingering `whoisthat_*` tables and rules, then re-arms the kill-switch if the
intent is still enabled. It also removes stale policy rules and routes left by a
crash.

## Incoming connections and conntrack

For inbound connections to a host behind the tunnel (for example a game server
or a container), reply packets would normally leave through the physical
interface and bypass the tunnel. The core uses conntrack reply marking: packets
belonging to an established inbound flow are marked and routed through Table
100, with postrouting NAT matching the physical address. This is why server
hosting and Docker publishing work without manual routing.

## Capabilities and verification

The core runs as the user with `cap_net_admin,cap_net_raw,cap_setpcap`. Ambient
capabilities are raised only when needed, and child `ip`, `nft`, and Xray
processes inherit them. Before TUN setup the core creates and destroys a
transient `wt-capcheck` adapter to prove the capabilities actually work; `doctor`
inspects the binary with `getcap` and reports a missing or stale capability set.

## Source map

| Area | Files |
| :--- | :--- |
| TUN lifecycle, routes, reconcile | `core/core/lib/proxy/tun/{tun,routes,reconcile,rpfilter}.go` |
| Split cgroup and nft rules | `core/core/lib/proxy/tun/{split,utils}.go` |
| DNS hijack and FakeDNS | `core/core/lib/proxy/tun/{dns_hijack,resolved}.go`, `lib/happdns/` |
| Kill-switch | `core/core/lib/proxy/tun/killswitch.go` |
| Direct socket marking | `core/core/lib/netmark/` |
| Xray TUN inbound config | `core/core/lib/xrayjson/` |

## See also

- [TUN Mode & Capabilities](/networking/tun-mode)
- [DNS & FakeDNS](/networking/dns)
- [Split Tunneling](/usage/split-tunnel)
- [Kill-Switch](/usage/kill-switch)
- [Go Core Daemon](/architecture/go-core)
