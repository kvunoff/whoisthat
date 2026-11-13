---
title: "Profiles & Subscription Groups"
description: "Manage subscription groups, profiles, metadata, and latency tests in the TUI."
audience: "user"
type: "how-to"
---

# Profiles & Subscription Groups

WhoisThat organizes VPN servers into a flexible hierarchical tree consisting of **Subscription Groups** and individual **Profiles**.

CLI equivalents:

```bash
whoisthat sub list
whoisthat sub refresh --all
whoisthat profile list
whoisthat profile rename 12:6 "Frankfurt"
whoisthat profile move 12:6 --to Personal
```

---

## Group Operations

Groups allow you to categorize profiles (e.g. by provider, protocol, or geographic location) and associate subscription update URLs.

| Action | Shortcut | Description |
| :--- | :---: | :--- |
| **Add Group** | <kbd>A</kbd> / <kbd>n</kbd> | Opens a modal asking only for the subscription URL. The group name comes from the provider (`profile-title` response header) or, if absent, from the URL host. |
| **Edit Group** | <kbd>e</kbd> | Edit group name, subscription URL, auto-update toggle and manual interval. Updating the URL automatically triggers an immediate profile fetch! |
| **Reset Group Name** | <kbd>Ctrl+R</kbd> (in Edit Group) | Restores the provider's name. Manual renames are never overwritten by subscription refreshes. |
| **Delete Group** | <kbd>X</kbd> | Prompts confirmation to delete the entire group and all contained profiles. |
| **Refresh Group** | <kbd>u</kbd> / <kbd>r</kbd> | Fetches the latest server list from the subscription provider, updating metadata and profiles. |
| **Move Group** | <kbd>J</kbd> / <kbd>K</kbd> | Move group up or down in the tree. Order is saved persistently to disk. |
| **Fold / Unfold** | <kbd>Space</kbd> / <kbd>Enter</kbd> | Toggle collapsing or expanding the group tree node (<kbd>h</kbd> to collapse, <kbd>l</kbd> to expand). |

---

## Subscription Metadata

When a subscription URL is fetched, the core daemon inspects HTTP response headers (`subscription-userinfo`, `profile-title`, `profile-update-interval`):

```text
upload=1048576; download=52428800; total=107374182400; expire=1720000000
```

The group is named after the provider's `profile-title` header (both plain and `base64:` values are supported) and falls back to the subscription URL host when the header is missing. Renaming the group manually pins your name; use <kbd>Ctrl+R</kbd> in the Edit Group modal to go back to the provider name.

### Automatic subscription refresh

The core reads the standard `profile-update-interval` response header (hours) and also accepts a `#profile-update-interval: N` directive in the body (plain or base64). Auto-update is enabled by default for every group. The Edit Group modal (<kbd>e</kbd>) exposes:

* **Auto-update** — on/off per subscription.
* **Interval, hours** — optional manual override; blank means "use the provider value". Enter `0` or clear the field to remove the override.

A manual value always wins over the provider value. If the provider stops sending the header, the stored interval is cleared and the schedule waits for a manual value.

The daemon checks subscriptions every 10 minutes. On startup, after autoconnect, it performs one probe for groups whose interval is not known yet, so missed updates after a long shutdown are picked up automatically. Background refreshes download and store new profiles but do not run latency tests or reconfigure a live DNS/routing/TUN session; the new configuration applies on the next connect or a manual refresh (<kbd>u</kbd>). A failed background refresh keeps the old profiles and retries with exponential backoff (30m → 1h → 2h → 4h → 6h).

The Details panel dynamically displays:
* **Used Bandwidth**: Total data consumed (Upload + Download) formatted in human-readable units (e.g., `51.0 MB`).
* **Data Limit**: Total quota allocated to the account (e.g., `100.0 GB`).
* **Expiration Date**: Calendar timestamp when the subscription expires.
* **Last Updated**: Relative timestamp since the last successful sync.
* **Auto-update**: Whether the background refresh is on, and whether the interval came from the provider or a manual override.

---

## Profile Operations

| Action | Shortcut | Description |
| :--- | :---: | :--- |
| **Import Profile** | <kbd>a</kbd> | Paste one or more proxy URIs (`vless://`, `vmess://`, `trojan://`, `ss://`, `socks://`, `hysteria2://`, `hy2://`). |
| **Switch Focus** | <kbd>Tab</kbd> | Toggle focus between Profile Tree and Server Details pane. |
| **Rename Profile** | <kbd>e</kbd> | Rename selected profile alias. |
| **Copy URI** | <kbd>y</kbd> | Copies the selected profile's raw URI to the system clipboard (Wayland / X11). |
| **Delete Profile** | <kbd>x</kbd> | Delete the selected profile. |
| **Reorder Profile** | <kbd>J</kbd> / <kbd>K</kbd> | Move profile up or down within its group (<kbd>Alt+↓</kbd> / <kbd>Alt+↑</kbd>). |
| **Page Scroll** | <kbd>Ctrl+D</kbd> / <kbd>Ctrl+U</kbd> | Fast half-page scroll down / up through long server lists. |

---

## Latency Testing Engine

WhoisThat includes a high-precision multi-sample latency testing subsystem:

### Test Controls
* <kbd>t</kbd>: **Test Focused** — Tests only the selected profile, or all profiles within the selected group (using the optimized `test-group` command).
* <kbd>T</kbd>: **Scan All** — Queues every profile across all groups (the focused profile/group first), deduplicated by group/profile ID.
* <kbd>C</kbd>: **Cancel Tests** — Drops queued tests via the internal epoch counter. A test already in flight finishes its current sample or times out; it is not killed mid-connection.

### How results are measured

Tests simulate real traffic rather than ICMP. The median round-trip time is shown
in the tree; jitter and loss are shown in the details pane. Method, sample count,
concurrency, timeout and endpoint are configured in Settings. With **Auto-test on
refresh** enabled, refreshing a group schedules tests for the imported profiles;
groups larger than 50 profiles are skipped.

The test pipeline, methods and warning throttling are described in
[Testing](/developers/testing#latency-tester-internals).
