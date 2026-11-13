---
title: "Deep Links & Browser Integration (`wit://`)"
description: "The `wit://` deep-link grammar and desktop handler registration."
audience: "integrator"
type: "reference"
---

# Deep Links & Browser Integration (`wit://`)

WhoisThat registers the custom desktop URL scheme `wit://`. This enables web
dashboards, VPN provider portals, and personal websites to import subscriptions,
profiles, and routing policies with a single click in any web browser (Chrome,
Firefox, Brave, Edge).

::: warning Tokens travel in the URL
A subscription link can contain a token. It may end up in browser history, shell
history, logs, or a shared screenshot. Prefer the provider's one-click import
over pasting the full link into a terminal, and avoid sharing `wit://` URLs.
:::

---

## Protocol Registration

`install.sh` runs the registration once as the installing user, and every TUI launch verifies and re-registers the desktop launcher, icon and scheme handler in the background (idempotent, not just on first run).

To register or refresh the handler manually from the CLI:

```bash
whoisthat link register
# legacy alias
whoisthat register-deeplink
```

This writes two entries to `~/.local/share/applications/` and the icon to `~/.local/share/icons/hicolor/scalable/apps/whoisthat.svg`:

* `whoisthat.desktop` — the application menu launcher; opens the TUI in the terminal configured by `terminal` in `config.toml` (or auto-detected).
* `whoisthat-url.desktop` — the deep link handler (`Terminal=false`, `NoDisplay=true`), so it stays out of the application list.

The scheme is bound to the hidden handler:

```bash
xdg-mime default whoisthat-url.desktop x-scheme-handler/wit
```

To remove the handler, delete the two `.desktop` files and clear the binding:

```bash
rm -f ~/.local/share/applications/whoisthat.desktop \
      ~/.local/share/applications/whoisthat-url.desktop
xdg-mime default '' x-scheme-handler/wit
```

---

## Supported URL Schemes & Syntax

### Subscription ingestion (`sub` / `subscription`)

Creates a subscription group and fetches it. Latency tests follow the normal
**Auto-test on refresh** policy; this is not a silent background connect.

* **Query Parameter Format**:
  ```
  wit://sub?url=https%3A%2F%2Fprovider.com%2Fapi%2Fsub%3Ftoken%3Dxyz&name=Premium
  ```
* **Path Format**:
  ```
  wit://sub/https://provider.com/api/sub?token=xyz
  ```
* **Direct HTTPS Prefix**:
  ```
  wit://https://provider.com/api/sub?token=xyz
  ```

---

### Single profile import (`import`)
Imports an individual server profile into a specified group:

* **Query Parameter Format**:
  ```
  wit://import?uri=vless%3A%2F%2F...&group=MyServers
  ```
  The optional `group` parameter selects the target group; a `name` parameter is accepted but ignored (the profile name comes from the URI remark).
* **Direct Proxy Protocol Prefix**:
  ```
  wit://vless://uuid@host:443?security=reality&...#Frankfurt-01
  wit://hy2://pass@host:443?sni=example.com#Hysteria2-DE
  wit://trojan://pass@host:443#Trojan-JP
  ```

---

### Happ routing ingestion
Imports a provider Happ routing profile into an existing or auto-detected group:

* **Wrapped Format**:
  ```
  wit://happ://routing/add/<BASE64_PAYLOAD>
  ```
* **Query Format**:
  ```
  wit://routing?link=happ%3A%2F%2Frouting%2Fadd%2F...&group=Premium
  ```

---

### Direct browser actions
Control connection state directly from bookmarks or shell scripts. `connect` takes no query parameters: it connects to the last used profile.

* `wit://connect`
* `wit://disconnect`
* `wit://toggle`

---

## URL encoding

Query values must be percent-encoded. In particular:

- `#` starts a fragment, so encode it as `%23`;
- `&` separates parameters, so encode it as `%26`;
- `:` and `/` in nested URLs are usually encoded as `%3A` and `%2F`.

The direct prefix forms (`wit://vless://...`, `wit://happ://...`) avoid the
wrapper encoding but still need encoding for characters that would break the
outer URL.

## Troubleshooting

- **Browser does nothing**: check `xdg-mime query default x-scheme-handler/wit`.
  If it is empty, run `whoisthat link register`.
- **Handler opens but import fails**: run the same URL through
  `xdg-open` and read the TUI/CLI error. A malformed `url` or `uri` parameter is
  rejected before the daemon sees it.
- **Wrong group**: pass `group=<name>` explicitly; otherwise the importer uses
  the provider title or URL host.
- **Token leaked**: rotate the subscription token with the provider and clear
  browser/shell history as needed.

## Testing Deep Links Locally

Simulate clicking a browser link using the system's `xdg-open` tool:

```bash
# Test subscription import
xdg-open "wit://sub?url=https%3A%2F%2Fexample.com%2Fsub&name=TestGroup"

# Test action toggle
xdg-open "wit://toggle"
```

## See also

- [CLI Reference](/reference/cli)
- [Profiles & Subscriptions](/usage/profiles)
- [Happ Routing Profiles](/networking/happ-routing)
