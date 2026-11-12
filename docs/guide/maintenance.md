---
title: "Maintenance, Upgrading & Recovery"
description: "Upgrade, downgrade, back up, repair, and uninstall WhoisThat without losing data."
audience: "user"
type: "how-to"
---

# Maintenance, Upgrading & Recovery

WhoisThat ships as two independently versioned binaries — the Rust TUI/CLI
(`whoisthat`) and the Go daemon (`whoisthat-core`) — plus a pinned Xray runtime.
Upgrading replaces the binaries; your configuration and encrypted database are
kept.

WhoisThat also keeps last-known-good copies of its settings and daily snapshots
of the encrypted database. If a file is damaged or unreadable, the daemon does
**not** overwrite it with defaults; it reports the problem and keeps the damaged
copy for recovery.

## Check what you have

```bash
whoisthat version          # TUI + core versions
whoisthat core version     # daemon version only
whoisthat doctor           # versions, capabilities, IPC, storage
```

The doctor header also shows the **IPC protocol version**. TUI and core must
agree on it:

- over the Unix socket, a mismatched core is stopped and respawned;
- over TCP, the `hello` handshake rejects the connection.

`whoisthat-core` and the pinned Xray runtime are matched to the release that
installed them. Mixing an old `whoisthat-core` with a new TUI is what the
protocol check is for.

## Upgrade

### Installer / updater

Re-run the same command you installed with:

```bash
curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash
```

By default it downloads the **latest stable release bundle**, verifies its
`SHA256SUMS`, replaces both binaries, refreshes the Xray runtime, reinstalls to
`/usr/bin`, reapplies file capabilities and removes legacy copies from
`/usr/local/bin`. No Rust or Go toolchain is required.

Useful flags:

| Flag | Effect |
| :--- | :--- |
| `--yes` | Accepted for compatibility; the installer is non-interactive |
| `--release vX.Y.Z` | Install a specific release instead of the latest one |
| `--from-source` | Build from source instead of downloading the release bundle |
| `--branch vX.Y.Z` | Build a specific tag/branch (implies `--from-source`) |
| `--local` | Build from the current checkout (implies `--from-source`) |
| `--insecure-skip-verify` | Skip release and Xray checksums (dangerous, not recommended) |
| `--uninstall` | Remove unowned binaries, unit, desktop entries and icon (data is kept) |

### Arch Linux (AUR)

The prebuilt package is recommended:

```bash
paru -Syu whoisthat-bin
# or
yay -Syu whoisthat-bin
```

To build from source instead, use `whoisthat`. The two packages conflict, so
pacman will prompt to remove the other when you switch.

If `whoisthat` was installed with `install.sh`, AUR packages refuse to install
over the files pacman does not own. Remove the script install first with
`install.sh --uninstall`, then install from the AUR.

### Ubuntu PPA

```bash
sudo apt update
sudo apt upgrade whoisthat
```

The PPA is refreshed for every release on Ubuntu 24.04 LTS (Noble). For
Ubuntu 26.04 (Resolute), enable the Resolute series in the PPA settings first.

If you installed WhoisThat as a `.deb` or `.rpm`, upgrade it with the package
manager (`apt upgrade`, `dnf upgrade`) rather than the installer.

### Manual build

```bash
make build
sudo make install
```

`make install` installs to `/usr/bin` and runs `setcap`. If you build by hand,
remember that a new `go build` output has a new inode and **loses its
capabilities**:

```bash
sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /usr/bin/whoisthat-core
whoisthat doctor
```

See [Build & Run](/developers/build) for the full loop.

### Update notification in the TUI

The TUI checks `github.com/kvunoff/whoisthat/releases/latest` at most once every
24 hours and shows a popup when a newer stable release exists. The last version
seen is cached, so the popup still appears on later launches without another
network request.

- Only stable TUI releases (`vX.Y.Z`) are considered; `core-v*` tags are ignored.
- The install source is detected automatically: pacman/dpkg/rpm package,
  `install.sh`, a manual copy, or a development build.
- The popup shows the update command for that source and copies it with
  <kbd>Enter</kbd>/<kbd>u</kbd>; nothing is executed from the TUI. `.deb`/`.rpm`
  commands also verify the release `SHA256SUMS`.
- <kbd>s</kbd> skips that version; <kbd>Esc</kbd> postpones until the next launch.
- Development builds (`cargo run`, `make build`) and `WHOISTHAT_TEST_MODE=1` are
  never checked.

Disable the check with `check_updates = false`,
`whoisthat settings set check-updates=off`, or `WHOISTHAT_NO_UPDATE_CHECK=1`.
If GitHub is unreachable, no new request is made and any previously cached
version is still shown.

## After upgrading

```bash
whoisthat doctor
whoisthat logs --level error --lines 200
```

- The encrypted database and `config.toml` are migrated automatically when
  needed; nothing is rewritten until it parses.
- A new pinned Xray release gets its own directory under
  `~/.local/share/whoisthat/runtimes/xray/`. Old version directories are not
  deleted automatically and can be removed once no longer referenced.
- Package installs upgrade the binaries through dpkg/rpm/pacman; the encrypted
  data and runtimes live in the user's home and are untouched.
- If a profile suddenly fails, check that no stale `whoisthat-core` shadows the
  new one: `which -a whoisthat-core` should not list `/usr/local/bin`.

## Downgrade and rollback

Install the older release explicitly:

```bash
curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash -s -- --release vX.Y.Z
```

AUR users can install an older package version with their helper's
downgrade/cache support (`whoisthat-bin` or `whoisthat`). PPA users can pin an
older version explicitly:

```bash
sudo apt install whoisthat=<version>-1~ubuntu24.04.1
```

When TUI and core versions no longer match the same protocol, the TUI respawns
the daemon instead of attaching to it.

## What gets backed up

| Path | Contents | Kept |
| :--- | :--- | :--- |
| `~/.config/whoisthat/config.json.bak` | Last known good daemon settings | One generation (+ `.prev`) |
| `~/.local/share/whoisthat/backups/db-YYYYMMDD/` | Encrypted database snapshot | Last three days |
| `~/.config/whoisthat/db.key` | AES-256-GCM master key | **Not** backed up automatically |

Snapshots are encrypted copies of the database, not plaintext exports. They can
only be restored with the same `db.key`. A snapshot is skipped when the current
key cannot open the database, so it cannot silently become a useless copy.

## The Repair WhoisThat prompt

When the TUI connects and `application-state` reports `config_status` or
`db_status` as unreadable, it shows **Repair WhoisThat**:

- <kbd>Enter</kbd> restores settings from `config.json.bak` and/or the newest
  database snapshot;
- <kbd>Esc</kbd> postpones the repair and leaves the damaged files untouched for
  manual inspection.

The prompt is TUI-only: the CLI has no `repair`/`restore` command. Both
operations are also available as IPC commands (`repair-config`, `restore-db`)
if you are writing your own client — see [Wire Protocol](/reference/protocol).

Status values to look for in `application-state` / `whoisthat doctor`:

- `config_status`: `ok`, `missing`, `corrupt`, `repaired`;
- `db_status`: `ok` or `read-only` (with `snapshot_at` when a snapshot exists).

## Manual settings restore

Stop the core/TUI first, then copy the backup back. On non-systemd hosts use
`whoisthat core stop` instead of `systemctl --user stop`:

```bash
systemctl --user stop whoisthat-core 2>/dev/null || pkill -TERM -x whoisthat-core
cp ~/.config/whoisthat/config.json.bak ~/.config/whoisthat/config.json
chmod 0600 ~/.config/whoisthat/config.json
```

If the newest backup is unusable, try `config.json.bak.prev`. A damaged file
that was set aside during a write is kept as
`config.json.corrupt-<timestamp>`; inspect it before deleting anything.

## Manual database restore

Prefer the in-app repair. If you need to do it by hand, keep the current
directory:

```bash
systemctl --user stop whoisthat-core 2>/dev/null || pkill -TERM -x whoisthat-core
mv ~/.local/share/whoisthat/db ~/.local/share/whoisthat/db.broken
cp -a ~/.local/share/whoisthat/backups/db-YYYYMMDD ~/.local/share/whoisthat/db
```

Keep `~/.config/whoisthat/db.key` in place — the snapshot is encrypted with it.
The in-app restore does the same and automatically renames the old database to
`db.broken-<timestamp>`.

## Lost or corrupted key

`db.key` is a 32-byte random key with mode `0600`. There is no passphrase or
recovery phrase.

- If you have a backup of the key, restore it before starting the core and keep
  the mode `0600`.
- If the key is gone but the encrypted database remains, the profiles cannot be
  decrypted. Either restore both the key and the database from a backup, or move
  the old database aside and let the core generate a new key and an empty
  database:

```bash
systemctl --user stop whoisthat-core 2>/dev/null || whoisthat core stop
mv ~/.local/share/whoisthat ~/.local/share/whoisthat.old
mv ~/.config/whoisthat/db.key ~/.config/whoisthat/db.key.old
# next start generates a new key and an empty database
```

The key lives in `~/.config/whoisthat`, not in the data directory, so moving the
database alone is not enough. Re-add subscriptions afterwards. Never delete
`db.key` while copying the database — you would lose the only way to read it.

## Leftover firewall and routing rules

If the daemon crashed or the machine lost power during TUN or kill-switch
operation, rules can be left behind. The next core start runs
`ReconcileOrphanedRules()` when no other core is live and then re-arms the
kill-switch if `kill-switch-enabled` is still true, so restarting is usually
enough:

```bash
whoisthat core restart
```

Manual cleanup, if the daemon cannot start:

```bash
sudo nft delete table inet whoisthat_ks 2>/dev/null
sudo nft delete table inet whoisthat_split 2>/dev/null
sudo nft delete table ip whoisthat 2>/dev/null
sudo nft delete table ip6 whoisthat 2>/dev/null
sudo nft delete table ip whoisthat_mangle 2>/dev/null
sudo nft delete table ip6 whoisthat_mangle 2>/dev/null
ip rule show | grep -E 'table (100|200)'   # inspect before deleting
```

See [Kill-Switch](/usage/kill-switch#self-healing-and-orphaned-rules) for the
policy details.

## Diagnose before deleting

```bash
whoisthat doctor --logs
whoisthat logs --level error --lines 200
whoisthat logs --source tui --lines 200
```

`doctor --logs` reports log paths, modes, rotation, recent errors and whether
`core-crash.log` contains a previous panic. Collect that output before wiping
anything.

## Uninstall

For a script install:

```bash
curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash -s -- --uninstall
```

For a package install:

```bash
sudo pacman -Rns whoisthat-bin      # or whoisthat
sudo apt remove whoisthat           # Debian / Ubuntu
sudo dnf remove whoisthat           # Fedora / RHEL
```

If you installed from the PPA, remove the repository as well:

```bash
sudo add-apt-repository --remove ppa:kvunoff/whoisthat
```

The script installer stops the user service and removes unowned binaries, the
systemd unit, and the desktop entries. It deliberately leaves files owned by
dpkg/rpm in place. Package removal only deletes package-owned files; stop the
user service first if it is running. **Configuration and data are preserved**
either way:

- `~/.config/whoisthat/` — settings, key, logs, geo assets
- `~/.local/share/whoisthat/` — encrypted database, backups, Xray runtimes

To remove everything, including profiles and the encryption key:

```bash
rm -rf ~/.config/whoisthat ~/.local/share/whoisthat
```

::: danger Irreversible
Deleting `~/.config/whoisthat/db.key` makes the encrypted database unreadable.
Back it up before wiping the config directory.
:::

## See also

- [Platforms & Service Management](/guide/platforms)
- [Configuration Files Reference](/reference/configuration)
- [Doctor Diagnostics](/reference/doctor)
- [Troubleshooting & FAQ](/reference/troubleshooting)
