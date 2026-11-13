---
title: "Data & Encryption"
description: "How WhoisThat stores profiles, routing, keys, snapshots, and recovery state."
audience: "developer"
type: "explanation"
---

# Data & Encryption

The core persists profiles, groups, routing and provider metadata under
`~/.local/share/whoisthat/db/`. Every file is encrypted with AES-256-GCM; the
key is stored outside that directory so copying the data directory does not
copy the key.

## Layout

```text
~/.local/share/whoisthat/
├── db/
│   ├── config.json                 # DB-level config
│   ├── routing.json                # global manual routing rules
│   └── groups/<group_id>/
│       ├── group_config.json
│       ├── <profile_id>.json
│       ├── xray/<profile_id>.json  # provider JSON config for managed profiles
│       └── routing/
│           ├── manual.json         # custom per-group rules
│           └── geo/<profile_id>/<version>/
└── backups/db-YYYYMMDD/            # last three daily snapshots

~/.config/whoisthat/
├── config.toml                     # client settings
├── config.json                     # daemon settings
├── config.json.bak                 # last known good daemon settings
└── db.key                          # 32-byte master key, mode 0600
```

The exact path list is also in [Configuration](/reference/configuration).

## Encryption

`lib/crypto` uses AES-256-GCM with a random nonce per write. Files are stored as:

```json
{
  "ciphertext": "<base64 nonce + ciphertext + GCM tag>"
}
```

The key is generated with `crypto/rand` on first start and written with mode
`0600`; the database directory is `0700`. Keeping `db.key` under
`~/.config/whoisthat/` means a backup of the data directory alone cannot decrypt
the database.

## Atomic writes and permissions

`utils.AtomicWriteFile` writes to a temporary file in the same directory,
`fsync`s it, renames it, and `fsync`s the directory. A crash leaves either the
old or the new file, never a truncated one. Directory modes are tightened on
write (`0700`); file modes are `0600`.

Legacy unencrypted files are detected and migrated on startup. The migration
keeps the old key handling compatible and never overwrites a file that cannot be
read.

## Read-only degradation

If a file cannot be decrypted, `db.markDegraded` switches the database to
read-only:

- `db_status.state` becomes `read-only` and carries the error;
- writes are refused with `database is read-only: ...`;
- existing files are left untouched, so a correct key or a backup can still
  recover them.

The TUI sees this in `application-state` and offers **Repair WhoisThat**.

## Snapshots and restore

`db.snapshotDaily` copies the database once per day to
`backups/db-YYYYMMDD/` and keeps the last three. A snapshot is skipped when the
current key cannot open the files, because the copy would be useless.

`RestoreSnapshot` replaces the database with the newest snapshot and renames the
unreadable copy to `db.broken-<timestamp>` for inspection. The in-app restore
uses the same path. `db.key` is not part of the snapshot; restore the key and
the database together.

## Repair paths

- `config.json` is restored from `config.json.bak` (and `.prev`).
- The database is restored from the newest daily snapshot.
- A damaged file set aside during a write is kept as
  `config.json.corrupt-<timestamp>`.
- A lost key cannot be recovered; move the database and key aside and let the
  core create a new empty set.

The user-facing procedure is in
[Maintenance, Upgrading & Recovery](/guide/maintenance).

## Source map

| Area | Files |
| :--- | :--- |
| Database and key | `core/core/db/{db,init}.go` |
| Snapshots and restore | `core/core/db/snapshot.go` |
| Groups, profiles, routing | `core/core/db/{groups,profiles,routing,group_sync}.go` |
| AES-256-GCM | `core/core/lib/crypto/` |
| Atomic writes and modes | `core/core/utils/fs.go` |
| Path resolution | `core/core/utils/paths.go`, `lib/AppConfig/storage.go` |

## See also

- [Security & Threat Model](/architecture/security)
- [Go Core Daemon](/architecture/go-core)
- [Maintenance, Upgrading & Recovery](/guide/maintenance)
- [Configuration](/reference/configuration)
