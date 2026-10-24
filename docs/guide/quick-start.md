# Quick Start

Get up and running with WhoisThat in less than two minutes.

---

## 1. Launching the Interface

Run the `whoisthat` binary from your terminal:

```bash
whoisthat
```

When started, WhoisThat checks whether the background daemon (`whoisthat-core`) is already running. If it is offline, the TUI automatically spawns it in the background, establishes an authenticated Unix domain socket connection, and renders the interface.

---

## 2. Adding VPN Profiles

There are three ways to import VPN profiles:

### Method A: Subscription URL (Recommended)

1. Press <kbd>U</kbd> in the TUI to open the **Add Group** modal.
2. Enter a descriptive group name (e.g., `Personal VPN` or `Mullvad`).
3. Paste your subscription URL into the **Subscription URL** field.
4. Press <kbd>Enter</kbd> to save.

::: tip Automatic Ingestion & Testing
The core daemon immediately fetches the subscription, parses all supported proxy URIs, populates metadata (traffic usage, plan expiry), and automatically triggers latency tests for newly added servers.
:::

### Method B: Single Profile URI (Clipboard or Manual)

1. Copy any supported proxy URI to your system clipboard (`vless://`, `vmess://`, `trojan://`, `ss://`, `socks://`, `hysteria2://`, `hy2://`).
2. Press <kbd>a</kbd> in the TUI to open the **Import Profile** popup.
3. Press <kbd>Ctrl+V</kbd> or paste the URI and hit <kbd>Enter</kbd>.

### Method C: One-Click Browser Deep Link

If you click a `wit://` link on any web page (or run `whoisthat "wit://sub?url=..."`), WhoisThat creates the group and downloads the subscription headlessly without requiring you to open the TUI first.

---

## 3. Testing Latency & Server Speed

WhoisThat features a multi-sample latency testing engine:

| Key | Action | Details |
| :---: | :--- | :--- |
| <kbd>t</kbd> | **Test All** | Sequentially tests all profiles across all groups starting from the cursor. Deduplicates identical addresses. |
| <kbd>T</kbd> | **Test Focused** | Tests only the highlighted profile, or all profiles within the highlighted group. |
| <kbd>C</kbd> | **Cancel Tests** | Immediately halts all in-flight and queued latency tests. |

Pending tests display `…`. Finished profiles show median round-trip time in milliseconds (e.g. `42ms`), along with jitter and packet loss percentages in the details panel.

---

## 4. Connecting to a Server

1. Use <kbd>j</kbd> / <kbd>k</kbd> or the arrow keys <kbd>↓</kbd> / <kbd>↑</kbd> to move the selection bar.
2. Press <kbd>c</kbd> or <kbd>Enter</kbd> to connect to the highlighted profile.
3. The status indicator in the top header will transition to `[Connected]`.

---

## 5. Choosing Your Connection Mode

WhoisThat offers two distinct operating modes:

### Proxy Mode (Default)
Traffic is made available on local proxy ports:
* **SOCKS5**: `127.0.0.1:3090`
* **HTTP**: `127.0.0.1:3091`

Configure your browser or application proxy settings to use these ports.

### TUN Mode (System-Wide VPN)
Press <kbd>v</kbd> on your keyboard (or click `TUN` in the bottom status bar) to toggle TUN mode.

* Creates a virtual network adapter (`whoisthattun`).
* Intercepts and tunnels all TCP and UDP traffic from all system applications.
* Performs DNS hijacking to ensure zero DNS leaks.

---

## 6. Detach vs Quit

* **Detach (<kbd>q</kbd>)**: Exits the TUI interface, but **leaves the VPN running in the background**. You can reopen the interface at any time with `whoisthat` to reattach to the running session.
* **Full Quit (<kbd>Q</kbd> or <kbd>Ctrl+C</kbd>)**: Shuts down the VPN tunnel, restores system routing rules, and terminates the daemon.

---

## 7. Headless CLI Quick Toggles

You don't need to open the TUI just to toggle your VPN:

```bash
# Toggle connection (reconnects to last profile or disconnects)
whoisthat -t

# Toggle between TUN mode and Proxy mode
whoisthat -mt

# View one-line status
whoisthat status -s
# Example output: [Connected] [TUN] ↓ 2.4 MB/s ↑ 410.0 KB/s
```
