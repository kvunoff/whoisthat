# CLI Subcommands & Flags

WhoisThat exposes the whole client through a `noun verb` command tree. The interactive TUI is still
the default when no arguments are given; every other invocation is head-less and script-friendly.

```bash
whoisthat                              # TUI
whoisthat <noun> <verb> [selector] [flags]
```

The Go daemon is unchanged: the CLI talks to the same IPC socket as the TUI.

---

## Global options

These work before or after the subcommand:

| Option | Description |
| :--- | :--- |
| `-j`, `--json` | Machine-readable output on stdout: one JSON value (object or array), except documented NDJSON streams (`status --watch`, `logs -f`). |
| `-q`, `--quiet` | Suppress human progress notes on stderr. |
| `--no-color` | Disable ANSI colours (also honours `NO_COLOR`). |
| `--socket <PATH>` | Override the core Unix socket (conflicts with `--tcp`). |
| `--tcp <HOST:PORT>` | Reach the core over loopback TCP (`tcp-enabled` must be on). Remote TCP is not supported by the daemon. |
| `--timeout <SECS>` | Reply timeout. Command defaults: 10 s, 60 s for subscriptions, 120 s for batch tests. |
| `--no-spawn` | Never start `whoisthat-core` automatically. |
| `-v`, `--version` | Print the TUI and core versions. |
| `-h`, `--help` | Help for any level, e.g. `whoisthat route add --help`. |

Exit codes:

| Code | Meaning |
| :---: | :--- |
| `0` | Success |
| `1` | Runtime failure (core warning, timeout, I/O) |
| `2` | Invalid arguments / unknown selector |
| `3` | Core is unreachable and spawning is disabled or failed |

`stdout` carries data only. Progress and warnings go to `stderr`, so `--json` stays parseable.

---

## Selectors

Most commands accept either a numeric id or a name:

* `42` — profile/group id;
* `12:6` — `group_id:profile_id`;
* `"Vienna"` — exact name first, then case-insensitive substring;
* `--group <selector>` restricts profile lookup to one group.

Ambiguous matches are an error with a candidate list; they are never resolved silently.

---

## Connection and mode

```bash
whoisthat status                  # human summary
whoisthat status --short          # one line for status bars
whoisthat status --json           # stable machine contract
whoisthat status --watch --json   # NDJSON stream

whoisthat connect                 # last used profile, else first available
whoisthat connect 12:6            # group:profile
whoisthat connect "Vienna" --wait # wait for the result
whoisthat disconnect --wait
whoisthat toggle                  # smart toggle

whoisthat mode                    # current mode
whoisthat mode status --json
whoisthat mode tun                # enable system-wide TUN
whoisthat mode proxy              # disable TUN
whoisthat mode toggle
```

`connect --wait` fails immediately with the daemon's `connect-failed` reason instead of waiting for
the timeout. A non-fatal warning such as “kill-switch not armed: server IP unknown” is printed as a
note and does not turn a successful connection into an error.

---

## Profiles

```bash
whoisthat profile list
whoisthat profile list --group Work --json
whoisthat profile show 12:6
whoisthat profile show 12:6 --reveal       # show the raw URI (contains credentials)

whoisthat profile add 'vless://...' --group Work
cat uris.txt | whoisthat profile add -     # one URI per line from stdin

whoisthat profile rm 12:6 --yes
whoisthat profile rename 12:6 "Frankfurt"
whoisthat profile move 12:6 --to Personal
whoisthat profile reorder Work 12:6 12:7 12:8
whoisthat profile reorder Work --order 12:7,12:6,12:8
```

By default `profile list`/`show` omit the `uri` field; use `--reveal` when you really need it.
`profile add --group New` creates an empty group if it does not exist.

---

## Subscriptions and groups

`sub` is the primary noun; `group` is an alias.

```bash
whoisthat sub list
whoisthat sub list --json            # flat group list with profiles_count
whoisthat sub show 12
whoisthat sub add 'https://provider/sub/token' --name "Provider"
whoisthat sub add 'https://provider/sub/token' "Provider"   # legacy positional name
whoisthat sub refresh 12
whoisthat sub refresh --all
whoisthat sub refresh 12 --no-wait
whoisthat sub rename 12 "New name"
whoisthat sub url 12
whoisthat sub url 12 --set 'https://provider/sub/new-token'
whoisthat sub rm 12 --yes
whoisthat sub reorder 12 2
```

Subscription URLs are masked in output (`https://provider/…`); `--reveal` prints the token.
`sub add`/`sub refresh` wait for `subscription-updated` unless `--no-wait` is given.

---

## Routing

```bash
whoisthat route list                       # global rules (group 0)
whoisthat route list --group Work
whoisthat route add --type geosite --value geosite:openai --outbound proxy
whoisthat route add --type domain --value example.com --outbound direct --group Work
whoisthat route rm 3 --group Work
whoisthat route enable 2
whoisthat route disable 2
whoisthat route move 4 1
whoisthat route mode Work custom           # inherit|custom
whoisthat route mode Work subscription     # auto|manual|subscription

whoisthat route profile list --group Work
whoisthat route profile activate "Provider Full Bypass" --group Work
whoisthat route profile off --group Work
whoisthat route profile rm <routing-profile-id> --group Work

whoisthat route import 'happ://routing/add/...' --group Work
whoisthat route preset list
whoisthat route preset apply proxy_ai --group Work
whoisthat route preset remove proxy_ai --group Work
```

Notes:

* `route add --type` is the *user-facing* match type. Stored rules always use `type = "field"`;
  `geoip`/`geosite` values go into the `ip`/`domain` fields, matching the TUI.
* Rule indexes are 1-based and refer to the effective config; indexes shift after `rm`/`move`.
* Editing rules of an inheriting group implicitly switches it to `custom`; the CLI prints a note.
* `inherit`/`custom` cannot be applied to global rules (group `0`).
* Read-modify-write preserves the Happ DNS section.

---

## Tests

```bash
whoisthat test profile 12:6
whoisthat test profile 12:6 --wait
whoisthat test group Work --wait
whoisthat test all --wait
whoisthat test cancel
whoisthat test config
whoisthat test config --concurrency 16 --test-timeout 5 --samples 3
whoisthat test config --endpoint 'https://cp.cloudflare.com/generate_204'
whoisthat test config --auto-on-subscribe off
```

`--wait` for a group finishes when `test-progress` reports `tested >= total`. If all profiles are
already being tested, the command reports that instead of hanging. `test config` mirrors the
daemon's sanitized values after applying changes.

`test config` uses `--test-timeout` for the core's per-test timeout; the global `--timeout` is the
CLI reply timeout and is not the same setting.

---

## Settings

```bash
whoisthat settings list
whoisthat settings list --json
whoisthat settings get kill-switch
whoisthat settings set split-tunnel=exclude kill-switch=true
whoisthat settings --set theme=tokyo-night show-ip=off log-level=debug
whoisthat settings toggle kill-switch
```

Core-authoritative keys:

| Key | Values | Backend |
| :--- | :--- | :--- |
| `kill-switch` | `true`/`false` | core |
| `split-tunnel` | `off`/`exclude`/`include` | core |
| `autoconnect` | `true`/`false` | core |
| `autoconnect-profile` | selector | core (+ `config.json` for reading the target) |
| `autoconnect-mode` | `proxy`/`tun` | core |
| `tun-name` | `[a-zA-Z][a-zA-Z0-9_-]{0,14}` | core |
| `hwid-enabled` | `true`/`false` | core |
| `user-agent` | string | core |
| `test.concurrency` | `1..64` | core |
| `test.timeout` | `1..120` | core |
| `test.samples` | `1..10` | core |
| `test.endpoint` | http(s) URL | core |
| `test.auto-on-subscribe` | `true`/`false` | core |

Local keys (`config.toml`):

| Key | Values |
| :--- | :--- |
| `theme` | theme id from the TUI |
| `show-ip` | `true`/`false` |
| `log-enabled` | `true`/`false` |
| `log-level` | `error`/`warn`/`info`/`debug`/`trace` |
| `test-method` | `tcp`/`http-get`/`http-head` |
| `core.host` | host |
| `core.port` | `1..65535` |
| `core.use-tcp` | `true`/`false` |
| `core.socket` | socket path (empty = default) |

`settings set` validates every pair before touching the daemon. Core settings are applied first;
local `config.toml` is written last from a fresh read, so a running TUI is not clobbered.

When the daemon is offline, `settings list` still prints local keys and marks core keys as
`unknown`; `settings get <core-key>` exits with code `3`.

---

## HWID

```bash
whoisthat hwid status
whoisthat hwid enable
whoisthat hwid disable
whoisthat hwid reset
whoisthat hwid user-agent 'WhoisThat/1.0'
```

---

## Logs

```bash
whoisthat logs                        # last 100 core log lines
whoisthat logs --lines 500
whoisthat logs --level error
whoisthat logs --follow               # tail -f
whoisthat logs --json                 # JSON array
whoisthat logs -f --json              # NDJSON stream
whoisthat logs --tui                  # ~/.local/share/whoisthat/tui.log
```

The parser understands both the core format (`2026/10/29 12:00:00 [INFO] ...`) and the TUI format
(`12:00:00 INFO  ...`). Rotation is detected by inode.

---

## Core lifecycle

```bash
whoisthat core status
whoisthat core status --json
whoisthat core start
whoisthat core stop
whoisthat core restart
whoisthat core version
```

* Read-only commands never spawn the daemon. `core status` reports `running: false` offline.
* Mutating commands (`connect`, `sub *`, `settings set` for core keys, …) start the daemon unless
  `--no-spawn` is set.
* `core restart` uses `systemctl --user restart whoisthat-core.service` when the unit is enabled;
  otherwise it sends `die` and waits for the single-instance lock to be released.
* An open TUI can respawn a stopped core automatically; stop the TUI first if that matters.
* `--tcp`/`--socket` never trigger a spawn, because the new daemon would listen on the default UDS.

---

## systemd, links and utilities

```bash
whoisthat systemd status
whoisthat systemd enable
whoisthat systemd disable
whoisthat systemd toggle

whoisthat link handle 'wit://toggle'
whoisthat link handle 'wit://sub?url=...' --group Work
whoisthat link register

whoisthat doctor
whoisthat doctor --json
whoisthat ip
whoisthat ip --json
whoisthat version --json

whoisthat run -d spotify        # split-tunnel launcher (unchanged)
```

`link handle` and `link register` print textual progress and currently do not support `--json`
(they warn on stderr if it is requested). Everything else honours the global flag.

`doctor` exits `1` when the report contains failures.

---

## Split-tunnel launcher

`whoisthat run` launches an application inside the `whoisthat_split.slice` cgroup so the core's
split-tunnel rules can route it separately:

```bash
whoisthat run spotify
whoisthat run spotify --minimized
whoisthat run firefox -P work https://example.com
whoisthat run -d spotify                 # detached transient service
whoisthat run -- /opt/app/-weird-name    # app name that starts with a dash
```

- Everything **before** the application name belongs to WhoisThat (`-d`, `--detach`, `-b`,
  `--background`, plus global options).
- Everything **after** the application name is passed through unchanged, even if it looks like a
  WhoisThat flag. In `whoisthat run spotify --minimized`, `--minimized` is Spotify's flag.
- `-d` after the application name is an application argument, not detach:
  `whoisthat run spotify -d` launches `spotify -d`.
- The application's exit code is propagated.
- `whoisthat run --help` and `whoisthat help run` document the command.

---

## Legacy flags

Old flags are hidden aliases and keep working:

| Legacy | New |
| :--- | :--- |
| `-t`, `--toggle` | `toggle` |
| `-c`, `--start [target]` | `connect [target]` |
| `-d`, `--stop` | `disconnect` |
| `-mt`, `--mode-toggle`, `toggle-tun` | `mode toggle` |
| `--mode-tun`, `--tun` | `mode tun` |
| `--mode-proxy`, `--proxy` | `mode proxy` |
| `-st`, `--systemd-*` | `systemd *` |
| `-kt`, `--killswitch-*` | `settings toggle/set kill-switch` |
| `-p`, `--profiles` | `sub list` |
| `add-sub <url> [name]` | `sub add <url> [name]` |
| `handle-url <url> [--group]` | `link handle <url> [--group]` |
| `register-deeplink` | `link register` |
| `status -s`, `-s` | `status --short` |
| `status -w -j`, `-j` | `status --watch --json` |
| `-v`, `--version` | `version` |

---

## JSON output

`status --json` is unchanged (Waybar/Polybar/GNOME contract). New commands use stable objects:

```json
{"connected":true,"mode":"tun","tun_enabled":true,"split_tunnel":"exclude"}
```

```json
[{"id":6,"group_id":12,"group_name":"tuqise","name":"Vienna","protocol":"vless",
  "address":"198.51.100.1","host":"vpn.example.com","test_result":318,
  "loss_pct":0,"jitter_ms":12,"tested_at":1712345678,"active":true,"uri":null}]
```

`profile list --json` and `sub list --json` omit credentials unless `--reveal` is passed.

---

## Scripting examples

```bash
# Toggle from a window manager
bind = $mainMod, V, exec, whoisthat -t

# Status bar
whoisthat status --short
whoisthat status --watch --json

# Pick the fastest profile and connect
whoisthat test group Work --wait --json \
  | jq -r 'sort_by(.test_result) | .[0] | "\(.id)"' \
  | xargs -r whoisthat connect

# Refresh every subscription from a timer
whoisthat sub refresh --all

# Read errors only, as JSON lines
whoisthat logs --level error --json
```

---

## Not covered yet

* Downloading/updating the pinned Xray runtime (no IPC command; needs a core change).
* Managing split-tunnel application lists (apps are launched through `whoisthat run`).
* Changing the TUN interface name on a live connection (`set-tun-name` persists it; reconnect to apply).
* Remote TCP: the daemon binds loopback only.
