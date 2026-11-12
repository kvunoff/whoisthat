---
title: "Glossary"
description: "Canonical WhoisThat terminology and spelling used across the documentation."
audience: "user"
type: "reference"
---

# Glossary

Use these terms consistently in the TUI, CLI, code, and documentation.

| Term | Meaning |
| :--- | :--- |
| **WhoisThat** | The product: a Rust TUI/CLI and a Go core. Written as one word. |
| **core**, `whoisthat-core` | The Go daemon that owns state, routing, TUN, the encrypted database, and Xray. |
| **client** | The Rust binary that provides the TUI and CLI. |
| **TUI** | The interactive terminal interface. |
| **runner**, `whoisthat-runner` | The per-user helper that launches split-tunnel applications on systems without systemd. |
| **profile** | One server entry inside a group. |
| **group** | A subscription or a manual collection of profiles. |
| **subscription** | A provider URL that fills a group with profiles. |
| **routing profile** | A Happ or JSON routing configuration attached to a group. |
| **TUN mode** | System-wide routing through the `whoisthattun` virtual interface. |
| **proxy mode** | Local SOCKS5/HTTP proxying without system-wide routing. |
| **split tunneling** | Routing selected applications inside or outside the VPN. Two words. |
| **exclude mode** | Listed applications bypass the VPN. |
| **include mode** | Only listed applications use the VPN. |
| **kill-switch** | The independent nftables firewall that blocks traffic when the VPN drops. Hyphenated. |
| **armed** | The kill-switch is actually installed in the kernel. |
| **enabled** | The user intent to use the kill-switch; it may not be armed yet. |
| **Happ** | A provider routing format and the profiles built from it. |
| **FakeDNS** | Xray's fake-IP DNS mode. One word, capital D. |
| **routing precedence** | The first-match order for manual, Happ, and JSON routing rules. |
| **manual routing mode** | The per-group choice between inheriting global rules and using custom rules. |
| **managed JSON profile** | A profile imported from an Xray JSON subscription and owned by the provider. |
| **HWID** | The device fingerprint sent to subscription providers. |
| **doctor** | The `whoisthat doctor` system and runtime diagnostics suite. |
| **deep link** | A `wit://` URL that imports a profile or subscription. |
| **IPC** | The length-prefixed JSON protocol between the client and the core. |
| **protocol version** | The IPC compatibility number (`5` in the current release). |
| **Xray-core** | The pinned proxy runtime managed by the core. |
| **status JSON** | The stable `whoisthat status --json` and NDJSON contract. |
| **warning key** | A machine-readable `warn` reason such as `connect-failed`. |
| **request id** | The optional `id` echoed on replies and request-scoped notifications. |
| **managed profile** | A profile owned by a JSON subscription; provider routing stays immutable. |
| **split slice** | The cgroup that `whoisthat run` places applications in. |
| **direct mark** | `SO_MARK=1`, used for direct traffic and split-exclude apps. |

## Spelling and style

- "WhoisThat", not "WhoIsThat" or "Who is that".
- "split tunneling", not "split tunnelling".
- "kill-switch", not "kill switch" or "killswitch".
- "FakeDNS", not "fake DNS" or "fake-ip" in prose.
- "core" for the daemon; "client" for the Rust process.
- "TUN mode" and "proxy mode" in titles; lowercase in running text.

## See also

- [Documentation Guide](/developers/documentation)
- [Configuration Files Reference](/reference/configuration)
- [TCP & IPC Wire Protocol Reference](/reference/protocol)
