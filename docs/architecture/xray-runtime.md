---
title: "Xray Runtime"
description: "How the pinned Xray-core runtime is downloaded, verified, adopted, and used."
audience: "developer"
type: "explanation"
---

# Xray Runtime

WhoisThat does not ship Xray inside its own binary. `xraymgr` manages a pinned
release, verifies it, and exposes the binary to the proxy manager.

## Pin and layout

The pin is `core/core/lib/xraymgr/version.go::PinnedVersion` (`v26.10.31` for
this client release). The runtime lives under:

```text
~/.local/share/whoisthat/runtimes/xray/v26.10.31/
```

`Manager.Status` exposes `TargetVersion`, `Status`, `Version`, `Progress`,
`BytesDownloaded`, `TotalBytes` and `Error`. The status is one of `missing`,
`downloading`, `ready`, or `error`.

## Init sequence

`Manager.Init`:

1. Checks the managed binary and its version.
2. If it is missing, checks `/usr/local/bin/xray` and `/usr/bin/xray`.
3. Adopts a system binary only when its version **exactly** matches the pin;
   a different version is logged and ignored.
4. Sets `ready` or leaves `missing`.

A second core does not download into the same directory: the single-instance
lock and the runtime directory layout prevent a race.

## Download and verification

`downloadAndInstall`:

1. Fetches the official `.dgst` file **only from GitHub**.
2. Downloads the archive from GitHub, falling back to the `ghfast.top` mirror.
3. Verifies SHA-256 before unpacking.
4. Unpacks into a temporary directory next to the runtime.
5. Runs the binary's version check and compares it with the pin.
6. Renames the temporary directory into place.

A missing or mismatched checksum aborts the install. Progress is streamed as
`xray-progress` notifications; a failed download becomes `error` with the
underlying message.

## Use by the proxy manager

`GetXrayBin` returns the managed path only when the status is `ready`; while
`downloading` it returns a clear error. The proxy manager:

- generates the Xray JSON in memory (see [Routing Engine](/architecture/routing-engine));
- runs `xray run -test` as a preflight;
- pipes the config to `xray run` over stdin;
- reads stats and exposes latency-test results.

Temporary test instances lease ports from `lib/PortPool` (default range
`3095`–`30120`); a port is returned as soon as the sample finishes or times out.

## Missing runtime

If the runtime is absent and cannot be downloaded (offline install, blocked
network), the core emits a high-priority `warn` with an actionable hint. A
connect attempt fails with the underlying error; the TUI does not pretend the
VPN is up.

## Source map

| Area | Files |
| :--- | :--- |
| Pin and manager | `core/core/lib/xraymgr/{version,xraymgr}.go` |
| Download, digest, adoption | `core/core/lib/xraymgr/download.go` |
| Port leasing | `core/core/lib/PortPool/port-pool.go` |
| Preflight and start | `core/core/lib/proxy/mainproxy/{preflight,mainproxy}.go` |
| Stats and tester | `core/core/lib/proxy/mainproxy/{stats,tester}.go` |
| Progress event | `lib/TCPServer`, `core_events` |

## See also

- [Routing Engine](/architecture/routing-engine)
- [Go Core Daemon](/architecture/go-core)
- [Configuration](/reference/configuration#xray-runtime)
- [Troubleshooting & FAQ](/reference/troubleshooting)
