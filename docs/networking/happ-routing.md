# Happ Routing Profiles

WhoisThat provides complete, native support for **Happ routing profiles** (`happ://routing/...`). Many modern VPN subscription providers and server panels bundle comprehensive routing policies (containing thousands of verified domains, CIDRs, and regional rules) directly in subscription responses or via external links.

---

## What is a Happ Routing Profile?

A Happ routing profile is an encoded URL structure:
* `happ://routing/add/{base64}`: Ingests and saves a routing profile.
* `happ://routing/onadd/{base64}`: Ingests and immediately activates the routing profile.
* `happ://routing/off`: Deactivates the active Happ profile for the group.

The base64 payload unpacks into JSON defining direct domains/IPs, blocked domains/IPs, proxied domains/IPs, external geo asset download URLs, and a `GlobalProxy` flag.

---

## Per-Group Routing Modes (<kbd>m</kbd>)

In WhoisThat, routing policies can be customized **per subscription group**. Press <kbd>m</kbd> on the Routing tab to cycle the mode for the highlighted group:

```
[auto] ──► [manual] ──► [subscription] ──► [auto]
```

### 1. `auto` Mode *(Default)*
* If an active Happ profile exists and its geo assets are compiled and ready, it is automatically used.
* If no Happ profile is attached, or if an asset download error occurs, WhoisThat **transparently falls back to your manual rules and presets**.

### 2. `manual` Mode
* Strictly enforces your manual routing rules and presets catalog.
* Any Happ profiles present in the subscription are ignored.

### 3. `subscription` Mode
* Strictly enforces the active Happ routing profile supplied by the provider.
* If the Happ profile is temporarily unavailable or downloading, traffic falls back safely to direct/LAN rather than exposing your manual rules.

---

## Rule Compilation & Precedence Order

When a Happ profile is active, WhoisThat compiles rules into Xray using a strict hierarchy:

1. **Private LAN Bypass (`direct`)**: Guaranteed first priority. RFC1918 subnets (`127.0.0.0/8`, `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `::1/128`, `fc00::/7`, `fe80::/10`) always exit via your local gateway.
2. **Block Rules (`block`)**: `BlockSites` and `BlockIp` are blackholed.
3. **Direct Rules (`direct`)**: `DirectSites` and `DirectIp` bypass the proxy.
4. **Proxy Rules (`proxy`)**: `ProxySites` and `ProxyIp` are forwarded through the VPN tunnel.
5. **Catch-All Fallback**:
   * If `GlobalProxy == false`: Unmatched traffic defaults to `direct` (`network: tcp,udp -> direct`).
   * If `GlobalProxy == true`: Unmatched traffic defaults to `proxy`.

---

## Isolated Per-Profile Geo Assets

Some Happ profiles link to specialized, provider-hosted `geoip.dat` and `geosite.dat` files. WhoisThat downloads and manages these assets in an isolated sandbox:

```
~/.local/share/whoisthat/geo/groups/<GID>/<PROFILE_ID>/v<VERSION>/
```

### Protection Guarantees:
* **HTTPS Enforcement**: Custom assets are only downloaded over secure TLS connections.
* **Size Cap**: Downloads exceeding 100 MiB are rejected to prevent disk-filling attacks.
* **Strict Timeout**: 3-minute hard ceiling per download attempt.
* **Non-Blocking Resilience**: An asset download failure **never drops an active VPN connection**. Previous verified assets or global system geo files are retained as fallbacks.

---

## Managing Happ Profiles

### In the TUI (Routing Tab)
* Press <kbd>H</kbd> to open the **Happ Routing Profiles** modal:
  * View installed profiles, rule counts, and compilation statuses.
  * Press <kbd>Enter</kbd> to activate a profile.
  * Press <kbd>o</kbd> to deactivate the active profile.
  * Press <kbd>d</kbd> or <kbd>x</kbd> to delete a profile.
* Press <kbd>i</kbd> to import a `happ://routing/...` link directly from your clipboard into the active group.

### Via CLI
Import any Happ link directly from your terminal or shell scripts:

```bash
whoisthat handle-url "happ://routing/add/<BASE64>" --group "Premium"
```
