---
title: "Routing Engine"
description: "How profiles, Happ and JSON subscriptions become Xray routing, and how live reload works."
audience: "developer"
type: "explanation"
---

# Routing Engine

Routing is assembled in memory from three inputs: manual rules stored in the
encrypted database, an optional Happ routing profile, and an optional provider
JSON configuration. The result is one Xray config that is validated before it
replaces a live process. The user-facing side is in [Routing](/usage/routing);
the exact first-match order is in
[Routing Precedence](/reference/routing-precedence).

## Pipeline

```text
share link / JSON subscription
        │
        ▼
lib/parser or lib/xrayjson          parse and normalize
        │
        ▼
db                                  store profile/rules encrypted
        │
        ▼
mainproxy.ResolveRouting            merge precedence
        │
        ▼
lib/xrayjson                        build routing, DNS, outbounds, inbounds
        │
        ▼
xray run -test                      preflight the generated config
        │
        ▼
mainproxy.ReloadRouting             stop old Xray, wait ~30 ms, start new one
```

The TUN device and system policy routing are not recreated for a routing-only
change. The old Xray process is stopped, the generated config is validated, and
a new process starts after a short gap. A failed preflight leaves the old
session in place and surfaces a warning instead of silently reporting success.

## Manual rules

Rules are stored per group plus a global group. Each rule has a matcher
(domain, IP, CIDR, port, process, protocol, GeoIP/GeoSite) and an action
(`proxy`, `direct`, `block`). The resolver emits structural rules first, then
manual rules, then provider rules, then the catch-all. Manual rules keep
priority over provider rules.

`SetActiveRoutingProfile` stores the selected profile for a group. A failed
database write must not update the in-memory group state; otherwise the UI shows
a profile that disappears after restart.

## Happ profiles

A Happ profile is a provider-owned routing document delivered as a
`happ://routing/...` link or via `wit://`. The ingestion path:

1. Decode the tolerant Base64 payload from the link.
2. Validate size and structure; provider labels are stripped of control
   characters.
3. Upsert the profile into the encrypted database, keyed by group and name.
4. Compile the provider rules into Xray rules.
5. Download provider Geo assets into an isolated per-profile directory, verify
   them, and reload the live config when the active profile changes.

Provider rules are immutable in the editor. A group can run in `auto`, `manual`,
or `subscription` mode; the manual layer still takes priority. The 7-day geo
refresh cooldown and the 100 MiB cap are enforced by the downloader. The format
contract is in [Happ Format Reference](/reference/happ-format).

## JSON subscriptions

A JSON subscription is detected before the URI parsers see the body. Each server
entry is stored as a managed profile with the provider's raw Xray config. The
provider owns routing, DNS, balancers, and outbounds; the daemon adapts only the
parts that must be local:

- `ext-ip:` / `ext-domain:` placeholders are resolved;
- `dialerProxy` and balancer references are mapped to the local outbounds;
- statistics and inbound settings are injected;
- the config is preflighted like a manual one.

Managed profiles are marked `Managed` and cannot be edited in the same way as
share-link profiles. A refresh replaces the provider payload without touching
manual rules. See [JSON Subscriptions](/networking/json-subscriptions).

## DNS assembly

The routing builder also emits the DNS block:

- legacy resolver list for direct queries;
- tagged `domestic-dns` and `remote-dns` servers for Happ mode;
- `dns-out` and the port-53 hijack;
- FakeDNS allocation when TUN, global proxy, and split mode allow it.

The resolver matrix belongs to [DNS & FakeDNS](/networking/dns); the kernel
side is in [Kernel Networking](/architecture/networking).

## Failure modes

- Invalid provider payload: rejected before it reaches the database.
- Failed `xray run -test`: the old session is kept and a warning is emitted.
- Failed database write: in-memory state must not diverge from disk.
- Missing split cgroup: TUN start fails with an actionable error rather than
  running without the requested split.
- Live reload during a subscription update: provider rules are swapped only
  after a successful preflight.

## Source map

| Area | Files |
| :--- | :--- |
| URI parsers | `core/core/lib/parser/` |
| Xray config assembly | `core/core/lib/xrayjson/` |
| Happ ingestion | `core/core/lib/happ/` |
| Happ DNS and FakeDNS | `core/core/lib/happdns/` |
| Resolution and reload | `core/core/lib/proxy/mainproxy/{routing,mainproxy}.go` |
| Database | `core/core/db/` |
| Precedence contract | `docs/reference/routing-precedence.md` |

## See also

- [Routing](/usage/routing)
- [Routing Precedence](/reference/routing-precedence)
- [Happ Format Reference](/reference/happ-format)
- [JSON Subscriptions](/networking/json-subscriptions)
- [Kernel Networking](/architecture/networking)
