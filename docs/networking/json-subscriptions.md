# JSON Subscriptions

Some providers ship a second subscription format where every server is a complete Xray client configuration instead of a share link. Happ calls these **JSON subscriptions** (`content-type: application/json`). WhoisThat imports them and keeps the provider's per-server routing intact.

## How the format looks

The body is a JSON array; each element is a full Xray config with its own `outbounds`, `routing`, `dns` and a display name in `remarks`:

```json
[
  {
    "remarks": "Germany Auto",
    "dns": { "servers": ["https://8.8.8.8/dns-query"] },
    "routing": {
      "rules": [
        { "type": "field", "domain": ["geosite:category-ru"], "outboundTag": "direct" },
        { "type": "field", "domain": ["geosite:youtube"], "balancerTag": "Germany_Balancer" }
      ],
      "balancers": [
        { "tag": "Germany_Balancer", "selector": ["proxy-1", "proxy-2"], "strategy": { "type": "leastPing" } }
      ]
    },
    "inbounds": [
      { "tag": "socks", "protocol": "socks", "port": 10808 },
      { "tag": "http", "protocol": "http", "port": 10809 }
    ],
    "outbounds": [
      { "tag": "proxy-1", "protocol": "vless", "settings": { "vnext": [{ "address": "de1.example.com", "port": 443 }] } },
      { "tag": "proxy-2", "protocol": "trojan", "settings": { "servers": [{ "address": "de2.example.com", "port": 443 }] } },
      { "tag": "direct", "protocol": "freedom" },
      { "tag": "block", "protocol": "blackhole" }
    ]
  }
]
```

A single configuration object is accepted too, but per-server routing is the interesting case.

## Delivery

On every refresh (`u`) WhoisThat probes `<subscription-url>/json` first:

- if it returns a usable array of Xray configs, that body is imported and the group is marked `format: json`;
- otherwise the normal share-link list is fetched, so providers without the JSON endpoint keep working;
- a URL that already ends in `/json` is fetched directly.

The provider's `routing` response header is still imported. JSON configs often reference custom `geosite:` / `geoip:` codes (for example `geosite:win-spy`), and that header carries the matching `Geoipurl` / `Geositeurl` assets.

## Immutability and update semantics

The embedded provider configuration — outbounds, routing rules, balancers, DNS — is **read-only**. WhoisThat never edits it in the UI or CLI.

On subscription refresh the configuration is replaced wholesale. Profiles are matched by their `remarks` value, so ids and latency results survive reordering and display-name renames. Removed entries delete their stored config; a provider-side change is visible after the next `u`.

## Rule priority

Manual rules from the Routing tab still apply and are checked first:

```
stats/api rule
TUN DNS capture rule
manual rules (Routing tab)
provider JSON rules
```

So a user override always wins, while anything not overridden falls through to the provider's policy. Provider `domainStrategy` and balancers are preserved. Manual rules that target `proxy` are mapped to the provider's `proxy` outbound, its single proxy outbound, or the target of the provider's first catch-all rule (usually the main balancer). A generated `whoisthat-user-proxy` balancer is used only as a fallback when the provider has no usable catch-all; it excludes chained outbounds that use `dialerProxy`.

Happ profiles delivered by the same subscription are used for their geo assets only; their rules are not stacked on top of the JSON routing. Importing, activating or deleting Happ profiles and changing the routing mode is disabled for JSON groups.

## Runtime adaptations

Only the structural pieces are changed:

- provider socks/http inbounds are moved to the ports from `config.json`; the native TUN inbound is added when TUN is active;
- `freedom` outbounds get `sockopt.mark = 1` so direct traffic bypasses the tunnel;
- `stats`, the StatsService API inbound and the api rule are injected (provider `policy` levels are kept);
- a `port 53 -> dns-out` rule is added in TUN mode, and DNS hijack uses the tunnel sink like a Happ profile;
- when the provider brings custom geo assets, the global `geoip.dat`/`geosite.dat` are exposed alongside them as `geoip-global.dat`/`geosite-global.dat`; manual `geoip:`/`geosite:` rules are compiled to Xray's `ext-ip:`/`ext-domain:` references so both datasets resolve in one Xray process;
- every outbound server address is resolved before TUN starts, added to `dns.hosts` and to the kill-switch allow-list — one JSON profile may contain several servers and balancers;
- traffic counters are summed across all proxy outbounds.

Latency tests run the provider config with a temporary SOCKS inbound, so balancers and per-server rules are exercised the same way as a real connection.
