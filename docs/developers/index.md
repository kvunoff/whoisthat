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
- [Development & Testing Guide](/reference/development) — the current build,
  test, and release workflow.

The dedicated build, repository map, testing, debugging, extending, and release
pages are part of the documentation restructuring. Until they land, use the
development guide above and the source tree as the reference.

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

## Getting help

Open an issue with the `docs` label for documentation gaps. For runtime
problems, use `whoisthat doctor` and the troubleshooting guide before filing a
bug.

## See also

- [Documentation Guide](/developers/documentation)
- [Development & Testing Guide](/reference/development)
- [IPC Transport & Framing](/architecture/ipc-flow)
- [Go Core Daemon](/architecture/go-core)
