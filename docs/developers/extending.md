---
title: "Extending WhoisThat"
description: "Step-by-step recipes for adding IPC commands, CLI commands, routing presets, doctor checks, and protocol parsers."
audience: "developer"
type: "how-to"
---

# Extending WhoisThat

This page follows one change through every layer it touches. Read
[Documentation Guide](/developers/documentation) first: a user-visible change
includes its docs, and a contract change includes the IPC snapshot.

## Add an IPC command

1. **Payload types** — add the request/reply structs to
   `core/core/structs/structs.go` with JSON tags matching the wire format.
2. **Core handler** — add a method on `cmd.Cmd` in `core/core/commands/`.
   Validate input at the boundary, write state, and emit the reply with
   `cmd.send(...)`; use `cmd.warn(key, message)` for recoverable failures.
3. **Dispatch** — add a `case "your-command":` to
   `core/core/lib/TCPServer/dispatch.go`, unmarshal `raw_tcp_message.Data`, and
   call the handler. Copy `RequestID` if the handler produces notifications.
4. **Rust types** — mirror the structs in `src/core_client/protocol.rs`.
5. **Rust command** — add a typed helper in `src/core_client/commands.rs` and a
   `CoreEvent` variant in `src/core_client/dispatch.rs` if the reply is a
   notification.
6. **Client surface** — wire the CLI (`src/cli/`) or TUI (`src/input/`) path.
7. **Contract snapshot** —
   `cd core/core && go test ./structs -update-contract` updates
   `core/core/testdata/ipc-contract.json`; the Rust test in
   `src/core_client/protocol.rs` deserializes it. Commit both sides together.
8. **Docs** — add the command to `reference/protocol.md` and the user-facing
   page that owns the feature.

Commands that start background work should use the `background` `WaitGroup` on
`cmd.Cmd` (see `AddGroup`) so tests can wait and a goroutine cannot outlive the
state it reads.

## Add a CLI command

1. Add the clap variant in `src/cli/args.rs`.
2. Add a module under `src/cli/commands/` and register it in
   `src/cli/commands/mod.rs`.
3. Dispatch it in `src/cli/mod.rs`; return `Exit(code)` or use the shared
   `Out` helper.
4. Use `Session::open_or_spawn` for mutating commands and `Session::open` for
   read-only commands. Never spawn for `--tcp`/`--socket`.
5. Add the command to `reference/cli.md`. `check-docs.sh` requires every
   top-level noun to appear as `whoisthat <noun>`.
6. Add a test or a self-check for the parsing and output path.

## Add a routing preset

1. Add the preset definition to the preset catalog under `src/ui/routing/`.
2. Map it to Xray rule fields in `core/core/lib/xrayjson/routing.go` if it
   needs a new matcher or outbound.
3. Add the user-facing description to `usage/routing.md`; precedence belongs
   to `reference/routing-precedence.md`.
4. Test the emitted rules, not only the TUI label.

## Add a doctor check

1. Add the check under `src/doctor/checks/`, using `CheckItem` and a
   `fix_hint` when there is an actionable fix.
2. Register the category with `CheckCategory::new(...)`; the name is part of the
   docs contract.
3. Add the category or item to `reference/doctor.md`.
4. Keep remediation steps in `reference/troubleshooting.md`; `doctor` reports,
   it does not repair.
5. Run `./scripts/check-docs.sh` — it checks every category name.

## Add a protocol parser

1. Add the URI parser under `core/core/lib/parser/` and a model in
   `models.go`.
2. Cover normal, missing, and malformed inputs with table tests.
3. Add the parser to the relevant fuzz target (`lib/parser/fuzz_test.go`).
4. Map the parsed profile to Xray JSON in `core/core/lib/xrayjson/` if a new
   transport or cipher is involved.
5. Add the protocol to `networking/protocols.md` and, if it has a format
   contract, a reference page.
6. Run `make test-fuzz` or the specific `-fuzz` target.

## Change the IPC contract

The Go structs and Rust types are a hand-maintained pair. The snapshot test
catches drift:

```bash
cd core/core && go test ./structs
cd ../.. && cargo test core_client::protocol
```

When the change is intentional, update the snapshot with
`go test ./structs -update-contract`, update the Rust types, and commit both.
Never edit `testdata/ipc-contract.json` by hand.

## Add or change docs

- New page: follow the templates in [Documentation Guide](/developers/documentation),
  add it to `docs/.vitepress/config.mts`, and link it from a hub.
- Moved page: use `git mv` and add a redirect in `docs/public/`.
- Removed page: redirect to the replacement or nearest hub.
- New CLI/config/IPC/doctor name: `check-docs.sh` will fail until the reference
  page mentions it.

## Verification

```bash
./scripts/check-docs.sh
./scripts/check-docs.test.sh
./scripts/check-versions.sh
npm run docs:build
```

For code changes, add the relevant `make` target (`test-go`, `test-rust`,
`test-e2e`, `integration`, `test-fuzz`).

## See also

- [Documentation Guide](/developers/documentation)
- [Repository Map](/developers/repository)
- [Testing](/developers/testing)
- [IPC & Wire Protocol](/reference/protocol)
