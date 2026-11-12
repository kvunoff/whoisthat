---
title: "Repair & Recovery"
description: "Back up, repair, and restore WhoisThat configuration, database, and network rules."
audience: "user"
type: "how-to"
---

# Repair & Recovery

WhoisThat keeps last-known-good copies of its settings and daily snapshots of the encrypted database. If a file is damaged or unreadable, the daemon does **not** overwrite it with defaults; it reports the problem and keeps the damaged copy for recovery.

---

## 1. What Gets Backed Up

| Path | Contents | Kept |
| :--- | :--- | :--- |
| `~/.config/whoisthat/config.json.bak` | Last known good daemon settings | One generation (+ `.prev`) |
| `~/.local/share/whoisthat/backups/db-YYYYMMDD/` | Encrypted database snapshot | Last three days |
| `~/.config/whoisthat/db.key` | AES-256-GCM master key | **Not** backed up automatically |

Snapshots are encrypted copies of the database, not plaintext exports. They can only be restored with the same `db.key`. A snapshot is skipped when the current key cannot open the database, so it cannot silently become a useless copy.

---

## 2. The “Repair WhoisThat” Prompt

When the TUI connects and `application-state` reports `config_status` or `db_status` as unreadable, it shows **Repair WhoisThat**:

- <kbd>Enter</kbd> restores settings from `config.json.bak` and/or the newest database snapshot;
- <kbd>Esc</kbd> postpones the repair and leaves the damaged files untouched for manual inspection.

The prompt is TUI-only: the CLI has no `repair`/`restore` command. Both operations are also available as IPC commands (`repair-config`, `restore-db`) if you are writing your own client — see [Wire Protocol](/reference/protocol).

Status values to look for in `application-state` / `whoisthat doctor`:

- `config_status`: `ok`, `missing`, `corrupt`, `repaired`;
- `db_status`: `ok` or `read-only` (with `snapshot_at` when a snapshot exists).

---

## 3. Manual Settings Restore

Stop the core/TUI first, then copy the backup back:

```bash
systemctl --user stop whoisthat-core 2>/dev/null || pkill -TERM -x whoisthat-core
cp ~/.config/whoisthat/config.json.bak ~/.config/whoisthat/config.json
chmod 0600 ~/.config/whoisthat/config.json
```

If the newest backup is unusable, try `config.json.bak.prev`. A damaged file that was set aside during a write is kept as `config.json.corrupt-<timestamp>`; inspect it before deleting anything.

---

## 4. Manual Database Restore

Prefer the in-app repair. If you need to do it by hand, keep the current directory:

```bash
systemctl --user stop whoisthat-core 2>/dev/null || pkill -TERM -x whoisthat-core
mv ~/.local/share/whoisthat/db ~/.local/share/whoisthat/db.broken
cp -a ~/.local/share/whoisthat/backups/db-YYYYMMDD ~/.local/share/whoisthat/db
```

Keep `~/.config/whoisthat/db.key` in place — the snapshot is encrypted with it. The in-app restore does the same and automatically renames the old database to `db.broken-<timestamp>`.

---

## 5. Lost or Corrupted Key

`db.key` is a 32-byte random key with mode `0600`. There is no passphrase or recovery phrase.

- If you have a backup of the key, restore it before starting the core and keep the mode `0600`.
- If the key is gone but the encrypted database remains, the profiles cannot be decrypted. Either restore both the key and the database from a backup, or move the old database aside and let the core generate a new key and an empty database:

```bash
systemctl --user stop whoisthat-core 2>/dev/null || pkill -TERM -x whoisthat-core
mv ~/.local/share/whoisthat ~/.local/share/whoisthat.old
# next start generates a new key and empty db
```

Re-add subscriptions afterwards. Never delete `db.key` while copying the database — you would lose the only way to read it.

---

## 6. Leftover Firewall / Routing Rules

If the daemon crashed or the machine lost power during TUN or kill-switch operation, rules can be left behind. The next core start runs `ReconcileOrphanedRules()` when no other core is live and then re-arms the kill-switch if `kill-switch-enabled` is still true, so restarting is usually enough:

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

See [Kill-Switch](/networking/killswitch#self-healing-orphaned-rules-cleanup) for the policy details.

---

## 7. Diagnose Before Deleting

```bash
whoisthat doctor --logs
whoisthat logs --level error --lines 200
whoisthat logs --source tui --lines 200
```

`doctor --logs` reports log paths, modes, rotation, recent errors and whether `core-crash.log` contains a previous panic. Collect that output before wiping anything.
