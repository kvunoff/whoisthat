# Upgrading, Downgrading & Uninstalling

WhoisThat ships as two independently versioned binaries — the Rust TUI/CLI (`whoisthat`) and the Go daemon (`whoisthat-core`) — plus a pinned Xray runtime. Upgrading replaces the binaries; your configuration and encrypted database are kept.

---

## 1. Check What You Have

```bash
whoisthat version          # TUI + core versions
whoisthat core version     # daemon version only
whoisthat doctor           # versions, capabilities, IPC, storage
```

The doctor header also shows the **IPC protocol version**. TUI and core must agree on it:

- over the Unix socket, a mismatched core is stopped and respawned;
- over TCP, the `hello` handshake rejects the connection.

`whoisthat-core` and the pinned Xray runtime are matched to the release that installed them. Mixing an old `whoisthat-core` with a new TUI is what the protocol check is for.

---

## 2. Upgrade

### Installer / updater

Re-run the same command you installed with:

```bash
curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash
```

The script is fetched from `main`, but by default it builds the **latest stable release tag**. It rebuilds both binaries, refreshes the Xray runtime, reinstalls to `/usr/bin`, reapplies file capabilities and removes legacy copies from `/usr/local/bin`.

Useful flags:

| Flag | Effect |
| :--- | :--- |
| `--yes` | Accepted for compatibility; the installer is non-interactive |
| `--branch vX.Y.Z` | Build a specific tag/branch instead of the latest release |
| `--local` | Build from the current checkout instead of cloning |
| `--insecure-skip-verify` | Skip the Xray checksum (dangerous, not recommended) |
| `--uninstall` | Remove binaries, unit, desktop entry (data is kept) |

### Arch Linux (AUR)

```bash
paru -Syu whoisthat
# or
yay -Syu whoisthat
```

### Manual build

```bash
make build
sudo make install
```

`make install` installs to `/usr/bin` and runs `setcap`. If you build by hand, remember that a new `go build` output has a new inode and **loses its capabilities**:

```bash
sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /usr/bin/whoisthat-core
whoisthat doctor
```

See [Development & Testing](/reference/development#local-development-loop) for the full loop.

---

## 3. After Upgrading

```bash
whoisthat doctor
whoisthat logs --level error --lines 200
```

- The encrypted database and `config.toml` are migrated automatically when needed; nothing is rewritten until it parses.
- A new pinned Xray release gets its own directory under `~/.local/share/whoisthat/runtimes/xray/`. Old version directories are not deleted automatically and can be removed once no longer referenced.
- If a profile suddenly fails, check that no stale `whoisthat-core` shadows the new one: `which -a whoisthat-core` should not list `/usr/local/bin`.

---

## 4. Downgrade / Rollback

Install the older release explicitly:

```bash
curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash -s -- --branch vX.Y.Z
```

AUR users can install an older package version with their helper's downgrade/cache support. When TUI and core versions no longer match the same protocol, the TUI respawns the daemon instead of attaching to it.

---

## 5. Uninstall

```bash
curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash -s -- --uninstall
```

This stops the user service, removes `/usr/bin/whoisthat`, `/usr/bin/whoisthat-core` (and the legacy `/usr/local/bin` copies), the systemd unit, and the desktop entry. **Configuration and data are preserved**:

- `~/.config/whoisthat/` — settings, key, logs, geo assets
- `~/.local/share/whoisthat/` — encrypted database, backups, Xray runtimes

To remove everything, including profiles and the encryption key:

```bash
rm -rf ~/.config/whoisthat ~/.local/share/whoisthat
```

::: danger Irreversible
Deleting `~/.config/whoisthat/db.key` makes the encrypted database unreadable. Back it up before wiping the config directory. See [Repair & Recovery](/guide/recovery).
:::
