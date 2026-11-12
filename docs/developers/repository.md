---
title: "Repository Map"
description: "Where the Rust client, Go core, documentation, scripts, and packaging live."
audience: "developer"
type: "how-to"
---

# Repository Map

Use this map to find the package that owns a behavior before changing it.

## File structure

```text
whoisthat/
├── .cargo/config.toml  ← static musl linker and rustflags for release builds
├── Cargo.toml          ← Rust project manifest (TUI & CLI dependencies)
├── README.md           ← Quick overview and reference guide
├── debian/             ← Ubuntu PPA packaging (control, rules, preinst, postinst)
├── install.sh          ← Release downloader / source installer / updater
├── pkg/
│   ├── whoisthat.desktop ← Desktop launcher entry (opens the TUI in a terminal)
│   ├── whoisthat-url.desktop ← Hidden x-scheme-handler/wit deep link handler
│   ├── nfpm.yaml         ← deb/rpm package definition (binaries, entries, icon)
│   ├── aur/              ← source AUR package (PKGBUILD + .SRCINFO + whoisthat.install)
│   └── aur-bin/          ← prebuilt AUR package (PKGBUILD + whoisthat-bin.install)
├── src/                ← Rust TUI & CLI source code
│   ├── main.rs         ← Entry point: CLI routing, core daemon spawn/re-attach, terminal lifecycle
│   ├── cli/            ← CLI: clap grammar, IPC session, selectors, output, command modules
│   │   ├── args.rs     ← clap definitions and hidden legacy aliases
│   │   ├── client.rs   ← Session (request-id correlation, spawn policy, timeouts)
│   │   ├── resolve.rs  ← group/profile/routing-profile selectors
│   │   ├── output.rs   ← JSON/human output, masking, tables
│   │   └── commands/   ← status, connection, core, profiles, subs, routing, tests, settings, logs, hwid, autostart, links, doctor
│   ├── runner/         ← per-user fork server: frame protocol, socket server, PTY helpers
│   ├── bin/whoisthat-runner.rs ← runner entry point (separate binary)
│   ├── doctor/         ← System & capability diagnostics (`whoisthat doctor`)
│   │   ├── mod.rs      ← Module entry point
│   │   ├── types.rs    ← Diagnostic models, pass/warn/fail statuses, reports
│   │   └── checks/     ← Diagnostic checks: binaries, capabilities, polkit, IPC, DNS, config/storage, logs
│   ├── event_loop.rs   ← AppEvent enum + run_loop (tokio::select! core_rx ↔ input_rx ↔ ticks)
│   ├── core_events.rs  ← handle_core_event — dispatches all CoreEvent broadcasts
│   ├── input/          ← Keyboard & mouse dispatch (mod.rs + navigation, profiles, routing, settings, mouse)
│   ├── popups/         ← Modal dialogs & routing popup key handlers (general, editors, routing)
│   ├── testing.rs      ← build_test_list, run_test_batch, persist_and_sync_test_config
│   ├── text_edit.rs    ← edit_text_field line editor + clipboard paste + UTF-8/Cyrillic support
│   ├── logger.rs       ← FileLogger (custom) + init/configure logger
│   ├── core_spawn.rs   ← spawn_core, find_core_binary, ensure_core_caps (pkexec fallback)
│   ├── autostart.rs    ← systemd / init / desktop autostart backends (status, enable, disable)
│   ├── lifecycle.rs    ← backend selection + session-scoped core markers (core-start/stop/restart/run)
│   ├── platform.rs     ← init detection, init-service state, privilege-wrapper check
│   ├── systemd.rs      ← Unit-file generation, linger, enable/disable service
│   ├── net_info.rs     ← fetch_public_ip / fetch_public_ipv6 (HTTPS via ureq to ipify)
│   ├── update_check.rs ← GitHub release check, install-source detection, update command
│   ├── deeplink/       ← wit:// parser, imports, and desktop entry/icon registration (parse, actions, desktop)
│   ├── launcher.rs     ← `whoisthat run` — systemd-run or whoisthat-runner, foreground/background
│   ├── terminal.rs     ← Re-exec the TUI in a terminal when launched without a tty
│   ├── config.rs       ← Config loader (~/.config/whoisthat/config.toml)
│   ├── core_client/    ← IPC client for the Go core (Unix domain socket / TCP)
│   │   ├── protocol.rs ← All serde types mirroring Go structs
│   │   ├── connection.rs ← UDS/TCP transport + 4-byte length framing
│   │   ├── dispatch.rs ← Read loop → typed event channel
│   │   └── commands.rs ← High-level async send functions
│   └── ui/             ← Ratatui UI components
│       ├── app/        ← Main app state & rendering (state, tree, details, popups, helpers)
│       ├── layout.rs   ← Responsive layout geometry engine (width/height tiers, adaptive splits)
│       ├── theme.rs    ← Curated color palettes (4 themes: Graphite Dark, Graphite Light, Tokyo Night, Dracula)
│       ├── settings.rs ← Settings screen
│       ├── routing/    ← Routing tab, presets catalog, rule form & popup rendering
│       ├── traffic.rs  ← Real-time traffic monitoring tab (Braille charts + live transfer stats)
│       ├── logs.rs     ← Log viewer (live tail + auto-scroll + level filter)
│       └── uri.rs      ← URI detail parser (VLESS/VMess/Trojan/SS/SOCKS/Hysteria2)
├── core/               ← WhoisThat Core (Go VPN engine)
│   └── core/
│       ├── main.go     ← Daemon entry point; RaiseAmbientCaps() before everything
│       ├── go.mod
│       ├── commands/   ← IPC command handlers (connect, test, groups, subscription, split, ...)
│       ├── structs/    ← Shared data types (mirror of src/core_client/protocol.rs)
│       ├── db/         ← Encrypted JSON profile DB (AES-256-GCM) + auto-migration
│       ├── utils/      ← caps.go (RaiseAmbientCaps, CanTun), user.go (UIDs), resolve.go
│       └── lib/        ← Core libraries
│           ├── parser/      ← Native in-memory URI parser & Xray JSON config generator
│           ├── xraymgr/     ← Pinned Xray-core runtime manager (v26.10.31 download & checksum verification)
│           ├── logger/      ← Level-filtered log wrapper (rotation configured in main.go)
│           ├── TCPServer/   ← IPC server + dispatcher (server, framing, auth, command dispatch)
│           ├── AppConfig/   ← Core configuration, defaults, HWID gen (config, storage, platform, apply)
│           ├── PortPool/    ← Dynamic port allocator for test/profile instances
│           ├── crypto/      ← AES-256-GCM encrypt/decrypt for DB files
│           ├── geo/         ← Auto-download + verify geoip.dat / geosite.dat from v2fly
│           ├── happ/        ← Happ routing: link parsing, payload decoder, geo sandbox
│           ├── happdns/     ← Happ DNS / FakeDNS compilation
│           ├── xrayjson/    ← Xray JSON assembly: routing, DNS, JSON subscriptions
│           ├── version/     ← CoreVersion and protocol.txt
│           └── proxy/       ← mainproxy (connect/stop/status, routing+DNS, tester+probes), xray wrapper, TUN manager + scripts
└── .github/workflows/
    ├── ci.yml          ← Automated CI: Rust, Go, shell, docs, versions and static musl build
    ├── release.yml     ← Builds release assets, publishes drafts and AUR/PPA packages
    ├── core-sync.yml   ← Mirrors core/core into kvunoff/whoisthat-core
    └── deploy-docs.yml ← Automated documentation build and deploy to GitHub Pages
```

---

## See also

- [Build & Run](/developers/build)
- [Architecture Overview](/architecture/overview)
