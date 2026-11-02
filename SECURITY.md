# Security Policy

WhoisThat handles VPN credentials, subscription tokens and privileged network configuration. Please report suspected vulnerabilities privately and do not open a public issue with exploit details.

## Supported Versions

Security fixes are applied to the latest release of each component:

| Component | Version source |
| :--- | :--- |
| `whoisthat` (TUI/CLI) | latest `v*` tag |
| `whoisthat-core` (daemon) | latest `core-v*` tag |
| Pinned Xray runtime | `xray/v*` release directory |

Older releases do not receive backports. If you are unsure, run `whoisthat version` and `whoisthat doctor` first.

## Reporting a Vulnerability

Use **GitHub's private vulnerability reporting** for this repository: **Security → Report a vulnerability**. If that option is unavailable, open a minimal public issue asking for a private channel — without reproduction details, credentials or logs.

Please include:

- affected component and version (`whoisthat version`, `whoisthat core version`);
- distribution and kernel (`uname -a`, `/etc/os-release`);
- connection mode (proxy/TUN), split-tunnel mode and whether the kill-switch was armed;
- minimal reproduction steps and expected/actual behaviour;
- relevant, redacted log lines (`whoisthat logs --level error --lines 200`).

## Never Attach

- `~/.config/whoisthat/db.key` or the encrypted database;
- raw subscription URLs, profile URIs or `--reveal` output;
- HWID values or provider tokens;
- unredacted `core.log`/`tui.log` without reviewing them first.

## Scope

In scope: the Rust client, the Go daemon, `install.sh`, packaging and documentation.

Out of scope:

- vulnerabilities in the upstream Xray-core runtime or third-party Go modules — report them upstream, but tell us if WhoisThat's defaults make them reachable;
- an attacker who already runs code as the same Unix user (they can read the key and control the IPC socket by design — see [Threat Model](/reference/threat-model));
- development-only tooling advisories in `package-lock.json` that are not part of the shipped binaries.

Reports are handled on a best-effort basis. We will confirm receipt, assess impact, and coordinate disclosure after a fix is available. Please give us a reasonable window before publishing.
