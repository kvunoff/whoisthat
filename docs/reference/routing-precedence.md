---
title: "Routing Precedence"
description: "The canonical first-match order for manual, Happ, and JSON routing."
audience: "developer"
type: "reference"
---

# Routing Precedence

Xray evaluates routing rules **top to bottom; the first match wins**. WhoisThat builds that list from structural rules, manual rules, a Happ profile, or a provider JSON baseline depending on the group and its mode. This page is the canonical order; editing UI and presets are covered in [Routing Rules & Presets](/networking/routing-rules).

---

## 1. Structural Rules

These are emitted by the core, not by the provider, and always come first:

| Order | Rule | When |
| :---: | :--- | :--- |
| 1 | Stats API traffic (`inboundTag: api → api`) | Stats enabled |
| 2 | TUN DNS capture (`tun-in`, port 53, tcp+udp → `dns-out`) | TUN + Happ DNS |
| 3 | Tagged resolvers (`domestic-dns → direct`, `remote-dns → proxy`) | TUN + Happ DNS |
| 4 | Legacy DNS (`port 53, udp → proxy`) | No Happ DNS section |

The legacy rule exists so DNS queries never reach user domain rules. With Happ DNS, rules 2–3 replace it.

---

## 2. Then the Effective Routing Source

For a group, `ResolveRouting` picks one source:

| Group mode | Manual source | Active Happ profile usable? | Effective source |
| :--- | :--- | :---: | :--- |
| `manual` | global or custom | — | **manual** |
| `auto` *(default)* | global or custom | yes | **Happ** |
| `auto` | global or custom | no | **manual** (global or custom) |
| `subscription` | any | yes | **Happ** |
| `subscription` | any | no | **LAN-only fail-safe** |

A Happ profile is usable when it is active, its compiled rule list is non-empty, and every referenced geo file exists on disk. The profile's `state` string is not the gate — a profile whose files were deleted after a successful download falls back.

- **Global rules** (group `0`) are always manual and cannot be switched to inherit/custom.
- **`manual_routing_mode`** is `inherit` (default, also stored as empty) or `custom`. `custom` uses the group's own rule file instead of Global.
- Editing a rule on an inheriting group switches it to `custom`, seeded from the stored override or from Global.
- `subscription` never falls back to manual rules; without a usable profile it installs `LAN → direct` and nothing else.

---

## 3. Manual and Happ Rule Order

For a normal (share-link) profile, the structural rules are followed by the rules of the **effective source** — manual or Happ, never both:

- **Manual source**: the global or custom rules in stored order.
- **Happ source**: the compiled profile rules:
   1. LAN / loopback (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `::1/128`, `fc00::/7`, `fe80::/10 → direct`);
   2. `BlockSites → block`;
   3. `BlockIp → block`;
   4. `DirectSites → direct`;
   5. `DirectIp → direct`;
   6. `ProxySites → proxy`;
   7. `ProxyIp → proxy`;
   8. catch-all `network: tcp,udp → direct` when `GlobalProxy=false`.

With `GlobalProxy=true` there is no terminal catch-all: unmatched traffic falls through to the default outbound (the active proxy).

Manual rules use the six match types described in [Routing](/usage/routing#supported-rule-match-types): domain, IP, protocol, port, geoip and geosite. GeoIP/GeoSite values are stored in the `ip`/`domain` fields.

---

## 4. JSON Subscription Groups

JSON subscriptions keep the provider's own routing as a baseline, so they bypass the Happ/manual selection above:

1. structural rules (API, TUN DNS capture);
2. **manual rules** from the Routing tab;
3. **provider JSON rules** and balancers.

Consequences:

- manual overrides always win over the provider policy;
- `auto`/`manual`/`subscription` and Happ profile activation are disabled for these groups;
- bundled Happ profiles are used for their geo assets only;
- manual `proxy` rules are mapped to the provider's proxy outbound, its single proxy outbound, or the target of the provider's first catch-all rule (usually the main balancer);
- `whoisthat-user-proxy` is generated only as a fallback when no usable provider catch-all exists, excluding chained outbounds that use `dialerProxy`.

See [JSON Subscriptions](/networking/json-subscriptions) for the format and runtime adaptations.

---

## 5. Domain Strategy and Fallback

The final `domainStrategy` is `IPIfNonMatch` unless the profile or manual config sets another value. FakeDNS forces `IPIfNonMatch` even if the profile asked for `AsIs`, otherwise IP rules would see fake addresses.

Changing rules, activating a Happ profile, or switching modes triggers a routing reload. The order above is rebuilt from scratch on every reload; the TUN device and system routes stay up.
