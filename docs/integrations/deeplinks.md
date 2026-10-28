# Deep Links & Browser Integration (`wit://`)

WhoisThat registers the custom desktop URL scheme `wit://`. This enables web dashboards, VPN provider portals, and personal websites to import subscriptions, profiles, and routing policies with a single click in any web browser (Chrome, Firefox, Brave, Edge).

---

## Protocol Registration

When you launch the WhoisThat TUI, it verifies and re-registers the desktop scheme handler in the background (idempotent, not just on first run).

To register or refresh the handler manually from the CLI:

```bash
whoisthat register-deeplink
```

This writes the desktop launcher to `~/.local/share/applications/whoisthat.desktop` and binds the scheme via:
```bash
xdg-mime default whoisthat.desktop x-scheme-handler/wit
```

---

## Supported URL Schemes & Syntax

### 1. Subscription Ingestion (`sub` / `subscription`)
Creates a new subscription group and immediately triggers a background profile fetch and latency test:

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

### 2. Single Profile Import (`import`)
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

### 3. Happ Routing Ingestion
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

### 4. Direct Browser Actions
Control connection state directly from bookmarks or shell scripts. `connect` takes no query parameters: it connects to the last used profile.

* `wit://connect`
* `wit://disconnect`
* `wit://toggle`

---

## Testing Deep Links Locally

Simulate clicking a browser link using the system's `xdg-open` tool:

```bash
# Test subscription import
xdg-open "wit://sub?url=https%3A%2F%2Fexample.com%2Fsub&name=TestGroup"

# Test action toggle
xdg-open "wit://toggle"
```
