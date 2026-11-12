---
title: "Quick Start"
description: "Launch WhoisThat, import a profile or subscription, test latency, and connect."
audience: "user"
type: "tutorial"
---

# Quick Start

This gets you from a fresh install to a working connection.

## Before you start

- Install WhoisThat first: [Installation](/guide/installation).
- Run `whoisthat doctor` once. It checks the binaries, capabilities, IPC socket
  and storage.
- TUN mode needs the core capabilities; proxy mode does not. If TUN is not
  available, the TUI reports the missing capability and points to `setcap`.

## Launch

```bash
whoisthat
```

The TUI attaches to a running `whoisthat-core` or starts one in the background
and connects over the Unix socket.

## Import a profile or subscription

### Subscription

1. Press <kbd>A</kbd> or <kbd>n</kbd>.
2. Paste the subscription URL.
3. Press <kbd>Enter</kbd>.

The core fetches the subscription, imports the supported profiles, fills in
provider metadata, and tests the new servers. The group is named after
`profile-title`, or the URL host when the provider sends none. Rename it with
<kbd>e</kbd>.

### Single profile

1. Copy a supported URI (`vless://`, `vmess://`, `trojan://`, `ss://`,
   `socks://`, `hysteria2://`, `hy2://`).
2. Press <kbd>a</kbd>. With text in the clipboard, WhoisThat imports it
   immediately; otherwise the import popup opens.
3. Paste with <kbd>Ctrl+V</kbd> and press <kbd>Enter</kbd>.

### Deep link

Clicking a `wit://` link (or running `whoisthat "wit://sub?url=..."`) imports
the profile or subscription without opening the TUI first. See
[Deep Links](/integrations/deeplinks).

## Test latency

| Key | Action |
| :---: | :--- |
| <kbd>t</kbd> | Test the focused profile or group |
| <kbd>T</kbd> | Test all profiles |
| <kbd>C</kbd> | Cancel queued tests |

Pending tests show `…`; results show the median round-trip time plus jitter and
loss in the details pane. The test method and sample count live in Settings.

## Connect

1. Move with <kbd>j</kbd>/<kbd>k</kbd> or the arrow keys.
2. Press <kbd>c</kbd> or <kbd>Enter</kbd>.
3. The header changes to `● Connected`.

## Choose a mode

**Proxy mode** is the default. Applications use the local SOCKS5/HTTP proxy:

- SOCKS5: `127.0.0.1:3090`
- HTTP: `127.0.0.1:3091`

**TUN mode** routes the whole system through `whoisthattun`. Press <kbd>v</kbd>
or click `TUN` in the footer. TUN captures DNS and routes it through the
resolvers described in [DNS & FakeDNS](/networking/dns); it is not a promise of
zero leaks for applications that use their own resolver or DoH.

See [TUN Mode & Capabilities](/networking/tun-mode) for requirements and limits.

## Detach or quit

- <kbd>q</kbd> detaches: the TUI exits and the VPN keeps running. Reopen with
  `whoisthat`.
- <kbd>Q</kbd> or <kbd>Ctrl+C</kbd> exits the client and shuts down the current
  core session. Autostart controls what happens on the next login or boot.

## CLI quick toggles

The noun/verb form is the stable interface:

```bash
whoisthat status
whoisthat toggle
whoisthat mode status
whoisthat mode toggle
whoisthat sub list
whoisthat sub refresh --all
whoisthat profile list
```

The legacy flags `-t` (toggle connection), `-mt` (toggle mode) and
`whoisthat status -s` (one-line status) still work. See the
[CLI Reference](/reference/cli).

## See also

- [Installation](/guide/installation)
- [Profiles & Subscriptions](/usage/profiles)
- [TUN Mode & Capabilities](/networking/tun-mode)
- [Configuration](/reference/configuration)
- [Troubleshooting & FAQ](/reference/troubleshooting)
