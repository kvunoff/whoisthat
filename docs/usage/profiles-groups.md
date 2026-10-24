# Profiles & Subscription Groups

WhoisThat organizes VPN servers into a flexible hierarchical tree consisting of **Subscription Groups** and individual **Profiles**.

---

## 1. Group Operations

Groups allow you to categorize profiles (e.g. by provider, protocol, or geographic location) and associate subscription update URLs.

| Action | Shortcut | Description |
| :--- | :---: | :--- |
| **Add Group** | <kbd>U</kbd> | Opens modal to create a new group with a custom name and optional subscription URL. |
| **Edit Group** | <kbd>e</kbd> | Edit group name or update subscription URL. Updating the URL automatically triggers an immediate profile fetch! |
| **Delete Group** | <kbd>X</kbd> | Prompts confirmation to delete the entire group and all contained profiles. |
| **Refresh Group** | <kbd>u</kbd> | Fetches the latest server list from the subscription provider, updating metadata and profiles. |
| **Move Group** | <kbd>J</kbd> / <kbd>K</kbd> | Move group up or down in the tree. Order is saved persistently to disk. |
| **Fold / Unfold** | <kbd>Space</kbd> / <kbd>Enter</kbd> | Toggle collapsing or expanding the group tree node (<kbd>h</kbd> to collapse, <kbd>l</kbd> to expand). |

---

## 2. Subscription Metadata

When a subscription URL is fetched, the core daemon inspects HTTP response headers (such as `subscription-userinfo`):

```
upload=1048576; download=52428800; total=107374182400; expire=1720000000
```

The Details panel dynamically displays:
* **Used Bandwidth**: Total data consumed (Upload + Download) formatted in human-readable units (e.g., `51.0 MB`).
* **Data Limit**: Total quota allocated to the account (e.g., `100.0 GB`).
* **Remaining Data & Percentage**: Color-coded progress bar showing remaining traffic.
* **Expiration Date**: Calendar timestamp when the subscription expires.
* **Last Updated**: Relative timestamp since the last successful sync.

---

## 3. Profile Operations

| Action | Shortcut | Description |
| :--- | :---: | :--- |
| **Import Profile** | <kbd>a</kbd> | Paste one or more proxy URIs (`vless://`, `vmess://`, `trojan://`, `ss://`, `socks://`, `hysteria2://`, `hy2://`). |
| **Rename Profile** | <kbd>e</kbd> | Rename selected profile alias. |
| **Copy URI** | <kbd>y</kbd> | Copies the selected profile's raw URI to the system clipboard (Wayland / X11). |
| **Delete Profile** | <kbd>x</kbd> | Delete the selected profile. |
| **Reorder Profile** | <kbd>J</kbd> / <kbd>K</kbd> | Move profile up or down within its group (instant & persistent). |

---

## 4. Latency Testing Engine

WhoisThat includes a high-precision multi-sample latency testing subsystem:

### Test Controls
* <kbd>t</kbd>: **Scan All** — Tests all profiles across all groups starting from the cursor, deduplicating identical IP/port endpoints.
* <kbd>T</kbd>: **Test Focused** — Tests only the selected profile, or all profiles within the selected group (using the optimized `test-group` command).
* <kbd>C</kbd>: **Cancel Tests** — Immediately halts all in-flight and queued latency tests using an internal epoch counter. Drops pending queue items before spawning Xray instances and terminates active test subprocesses without leaving orphans.

### Testing Architecture & Metrics
Latency tests are not simple ICMP pings (which many VPN firewalls block or prioritize). Instead, tests simulate real application traffic:

1. **Test Methods**:
   * **`tcp`**: Fast direct TCP handshake prefilter.
   * **`http-get`** *(Recommended)*: Spawns a temporary Xray instance and performs a full HTTP GET request to the test endpoint over SOCKS5.
   * **`http-head`**: Similar to HTTP GET but issues lightweight HTTP HEAD requests.
2. **Multi-Sample Precision**:
   Configurable in Settings (default: 3 samples). The median latency is displayed in the tree:
   * **Median Latency (ms)**: Real round-trip time, resilient to network spikes.
   * **Jitter (ms)**: Difference between maximum and minimum sample latency ($\Delta t = t_{\max} - t_{\min}$).
   * **Packet Loss (%)**: Percentage of failed sample requests.
3. **Progress Broadcasting & Visual Feedback**:
   * Pending profiles show `…` immediately when added to the test queue.
   * In-flight batches broadcast tested/total progress to the bottom status bar (`[Testing 12/48]`).
   * **Throttled Failure Notifications**: When test endpoints fail or time out, the core broadcasts a `warn` notification, throttled to 1 warning per error-reason per 5 seconds so users are not spammed with modal alerts.
4. **Auto-Test on Refresh**:
   When **Auto-test on refresh** is enabled in Settings, refreshing a subscription group (<kbd>u</kbd>) immediately schedules latency tests for all newly imported or updated servers.
