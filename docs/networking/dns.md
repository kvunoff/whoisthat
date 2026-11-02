# DNS & FakeDNS

WhoisThat intercepts DNS in TUN mode so queries cannot leave through the physical interface behind the VPN's back. Which resolver answers depends on the active profile: the legacy path uses your configured `dns-servers`, while a Happ profile can supply its own domestic/remote resolvers and FakeDNS.

---

## 1. Modes at a Glance

| Mode | When it is used | Resolver |
| :--- | :--- | :--- |
| **Legacy** | Manual routing, or a Happ profile without a DNS section | `dns-servers` from `config.json`, reached through the physical gateway |
| **Happ DNS** | The active Happ profile has `RemoteDNS*` / `DomesticDNS*` | Xray's tagged `remote-dns` (through the proxy) and `domestic-dns` (direct) |
| **FakeDNS** | Happ DNS plus TUN, `GlobalProxy=true`, split mode `off`/`exclude` | Fake pool first, then the real Happ resolvers |

The hijack rules are installed only while TUN is active. Proxy mode has no kernel-level DNS interception; applications must use the SOCKS5/HTTP inbound.

---

## 2. Legacy Resolver Hijack

With the legacy path:

1. `nat OUTPUT` and `nat PREROUTING` DNAT outbound UDP **and** TCP port 53 to the first IPv4/IPv6 address in `dns-servers`.
2. The DNATed query is routed through the physical gateway and masqueraded there, so it does not enter the TUN default route.
3. Xray's `freedom` outbound uses `dns-servers` for direct destinations.

`dns-servers` are also used to resolve the proxy server's own hostname: entries are queried sequentially and A/AAAA answers are merged (no parallel race or "fastest IP" selection).

The legacy hijack is independent of the split-tunnel mode: when the legacy path is active it stays installed in `off`, `exclude` and `include`.

---

## 3. Happ DNS

A Happ profile can replace Xray's DNS section entirely. WhoisThat routes queries through two tagged resolvers:

- **`remote-dns`** — resolved through the proxy outbound;
- **`domestic-dns`** — resolved directly.

The kernel side uses two **sink addresses** that are not assigned to any interface:

- `198.18.0.53` (IPv4)
- `fd00::53` (IPv6)

Queries are DNATed to a sink, survive the DNAT, follow the TUN default route and are read by Xray's TUN inbound. Xray then matches `tun-in :53 → dns-out`. Sink packets must never be delivered locally: if one of these addresses is assigned to an interface, the hijack stops working.

The hijack installs early returns so it never captures:

- sockets marked `1` (Xray's `freedom` outbound and split-`exclude` apps);
- traffic already addressed to the sink;
- the active VPN server IPs (a server listening on port 53 stays reachable).

`resolvectl dns <tun> 198.18.0.53` + `resolvectl domain <tun> "~."` points systemd-resolved at the same sink.

### Resolver selection and bootstrap

- `GlobalProxy = true`: `domestic-dns` is emitted only for the profile's direct domains (with `skipFallback`); `remote-dns` is the fallback for everything else.
- `GlobalProxy = false`: `domestic-dns` is the primary fallback; `remote-dns` is used only for the profile's proxy domains (with `skipFallback`).
- If the profile has no usable resolvers, `dns-servers` is used as a fallback.

`dns.hosts` is merged in this order (first value wins):

1. `DnsHosts` from the profile;
2. the DoH/DoU hostname mapped to `RemoteDNSIP`;
3. the domestic DoH/DoU hostname mapped to `DomesticDNSIP`;
4. the resolved IPs of the VPN server itself.

The last entry is required in TUN mode: when Xray reconnects, the system resolver is already captured, so the server domain must be resolvable without asking the tunnel it is about to establish.

---

## 4. FakeDNS

FakeDNS is enabled only when **all** of these are true:

- TUN mode is active;
- the active source is a Happ profile with a DNS section;
- the profile sets `FakeDNS: "true"`;
- `GlobalProxy == true`;
- split mode is `off` or `exclude` (not `include`).

When active, WhoisThat:

- adds `198.19.0.0/16` and `fc00::/18` fake pools to Xray;
- prepends a tagged `fakedns` server to `dns.servers`;
- adds `fakedns` to `destOverride` on `tun-in` only;
- keeps `routeOnly: false` and forces `IPIfNonMatch` if the profile asked for `AsIs`.

The pools do not overlap the TUN address (`198.18.0.1`), the DNS sink (`198.18.0.53`) or their IPv6 counterparts.

Limitations:

- ICMP is not mapped back to the real host. The TUN stack may answer `ping <fake-ip>` locally, so a successful ping does not prove reachability. Use TCP/HTTP.
- QUIC/HTTP-3 is not in `destOverride`, so HTTP/3 to a fake domain may bypass domain sniffing.
- Fake allocations are per Xray process: a reload invalidates old fake addresses.

---

## 5. Split Tunneling and the Kill-Switch

| Split mode | Happ hijack | FakeDNS | Resolution of split apps |
| :--- | :---: | :---: | :--- |
| `off` | on | per profile | — |
| `exclude` | on | per profile | marked `1`, resolve directly |
| `include` | **off** | **off** | leave through the physical resolver |

In `include` mode only explicitly launched apps use the tunnel; the rest of the system keeps its resolver. Hijacking port 53 system-wide would break every non-included application, so the Happ hijack and `resolvectl` changes are disabled. The legacy hijack is independent and remains installed.

With the kill-switch armed, its nftables table must allow the configured resolvers and the DNS sink (`198.18.0.53`, `fd00::53`); `whoisthat doctor` checks this.

---

## 6. Diagnostics

```bash
whoisthat doctor                 # DNS sink, fake pools, hijack mode, safety returns
nft list table ip whoisthat      # IPv4 hijack rules
nft list table ip6 whoisthat     # IPv6 hijack rules
resolvectl dns <tun-name>        # what systemd-resolved was pointed at
```

`doctor` reports the active mode as Happ sink, legacy resolver, disabled in `include`, or TUN inactive. If it cannot read firewall state without root, it prints the exact `sudo` command instead of failing.

See [Troubleshooting](/reference/troubleshooting) for leak symptoms and the HTTP/3 caveat.
