---
title: "For Developers"
description: "Build, test, debug, and contribute to WhoisThat."
audience: "developer"
type: "how-to"
---

# For Developers

WhoisThat is a Rust client and a Go core. This section explains how the pieces
fit together, how to build and test them, and how to change them safely.

## Start here

- [Documentation Guide](/developers/documentation) — how documentation is
  organized, written, and kept current.
- [System Architecture Overview](/architecture/overview) — processes, IPC, and
  the packet path.
- [Build & Run](/developers/build) — the local build and test workflow.

Build, repository, testing, debugging, and release guides are listed in the
sidebar. The Extending guide lands with the developer rewrite.

## Repository at a glance

```text
src/                 Rust TUI, CLI, IPC client, runner, doctor
core/core/           Go daemon, encrypted DB, routing, TUN, Xray manager
docs/                VitePress documentation (this site)
scripts/             Checks, packaging, root helpers, release tools
pkg/, debian/        Distribution packaging
.github/workflows/   CI, release, docs deploy, core mirror
```

## Before you change code

- Read the page that owns the behavior you are changing. If there is no owner,
  create one as part of the change.
- Keep the IPC contract in `core/core/structs/` and
  `src/core_client/protocol.rs` in sync. The contract snapshot fails CI when the
  two sides drift.
- Update documentation in the same pull request as the behavior change.
- Run the checks in [Documentation Guide](/developers/documentation) for docs
  changes, and the relevant `make` targets for code changes.

## Reporting a bug

A good report lets a maintainer reproduce the problem without guessing. Collect
the built-in diagnostic report before opening an issue.

```bash
whoisthat doctor --json > whoisthat-doctor.json
whoisthat version --json
whoisthat core status --json
whoisthat doctor --logs --json > whoisthat-logs.json
```

Include the relevant environment block:

```bash
uname -a
cat /etc/os-release
ip -V
nft --version
systemctl --version | head -1
```

Also state how WhoisThat was installed and the exact release, the connection
mode (proxy or TUN), split-tunnel mode, whether the kill-switch was armed, the
firewall backend, steps to reproduce, expected and actual result, and whether
the problem survives `whoisthat core restart`.

::: danger Redact secrets
Logs, `doctor` output and profile listings can contain subscription URLs, server
addresses, HWIDs and tokens.

- Never attach `~/.config/whoisthat/db.key` or the encrypted database.
- Do not paste raw `vless://`, `vmess://`, `trojan://`, `ss://` or
  `hysteria2://` URIs.
- Mask subscription tokens: `whoisthat profile list` and `sub list` hide them
  unless `--reveal` is used.
- Review `whoisthat logs` output before posting; credentials in a URL path are
  not masked automatically.
:::

File bugs and feature requests at
[github.com/kvunoff/whoisthat/issues](https://github.com/kvunoff/whoisthat/issues).
Documentation problems go to the same tracker with the page URL. Sensitive
reports (credentials, privilege escalation, firewall bypass) follow
[SECURITY.md](https://github.com/kvunoff/whoisthat/security/policy) and GitHub's
private vulnerability reporting; do not open a public issue with the details.

## Getting help

For runtime problems, use `whoisthat doctor` and the troubleshooting guide
before filing a bug. Open an issue with the `docs` label for documentation gaps.

## See also

- [Documentation Guide](/developers/documentation)
- [Build & Run](/developers/build)
- [IPC & Event Flow](/architecture/ipc-flow)
- [Go Core Daemon](/architecture/go-core)
