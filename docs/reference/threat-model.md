# Threat Model & Limitations

WhoisThat is a local VPN client, not a sandbox. This page states what the security measures protect against, what they assume, and where the boundaries are.

---

## 1. Trust Boundaries

| Boundary | Assumption |
| :--- | :--- |
| Same Unix user | **Trusted.** Any process running as your UID can read your config/key and control the daemon. |
| `root` | **Trusted.** Root can read everything and change any rule. |
| Other local users | **Untrusted.** Protected by `0600` sockets/files and the TCP token. |
| Remote network | **Untrusted.** That is what the proxy/VPN is for. |
| Subscription providers | **Untrusted input.** URLs, headers, names and routing profiles are parsed defensively. |
| Xray release chain | **Trusted.** The pinned archive is checksum-verified; the checksum is fetched only from GitHub, never from the mirror. |

---

## 2. What Is Protected

- **State at rest.** Profiles, subscription tokens and routing data are stored AES-256-GCM encrypted in `~/.local/share/whoisthat/db/`. The key lives outside that directory in `~/.config/whoisthat/db.key` (`0600`), so copying the data directory does not copy the key.
- **IPC.** The Unix socket is `0600` and restricted to your user. The opt-in TCP listener is loopback-only and requires a token from `ipc.token` (`0600`) plus a protocol match, with a 5-second handshake deadline.
- **Least privilege.** No setuid binary and no root daemon. The core runs as your user with `cap_net_admin,cap_net_raw,cap_setpcap`; capabilities are raised only when TUN needs them, and the TUI never runs with them.
- **Leak prevention.** TUN DNS hijack, `SO_MARK`/policy routing, the independent kill-switch and the IPv6 unreachable route are designed to keep traffic from escaping outside the intended path.
- **Provider input.** Happ payloads are size-capped (1 MiB) and geo downloads are HTTPS-only, size-capped and validated before use. Provider labels are stripped of control characters before they reach the terminal.

---

## 3. What It Does Not Protect

### Same-user compromise

The Unix socket relies entirely on filesystem permissions. Malware, a malicious script or a compromised editor running as your user can:

- connect to `core.sock` and connect/disconnect the VPN, switch TUN, change routing or read state over IPC;
- read `db.key` and decrypt the database;
- read logs that may contain subscription URLs.

There is no per-command authorization or user confirmation.

### Encryption is only at rest

The database key is stored on the same machine, protected only by file mode. There is no passphrase, KDF or hardware-backed key. Full-disk encryption and OS account security remain your responsibility. While the core runs, decrypted credentials and the generated Xray configuration exist in process memory (and are passed to Xray over stdin).

### Logs and URLs

Both the core and the TUI redact userinfo and query strings before logging. **A token embedded in the URL path is not masked.** Log files are `0600`, but treat `whoisthat logs`, `doctor --json` and `core.log` as sensitive when pasting them into an issue.

### DNS and routing limits

- Only destination port 53 is hijacked. DoH/DoT to another port, or applications with their own resolver, are not intercepted.
- `include` split mode deliberately leaves everything outside the split slice unprotected.
- `exclude` split apps are allowed through the kill-switch by design (`fwmark 1`).
- QUIC/HTTP-3 is not in the TUN sniffing list, so it can be routed by IP rather than by domain.
- FakeDNS does not map ICMP; a reply to `ping <fake-ip>` does not prove reachability.
- The kill-switch is an outbound firewall policy, not a guarantee against every leak (e.g. traffic explicitly allowed by the whitelist). It filters locally-generated traffic only: forwarded traffic (Docker, VMs, NAT on this host) is not covered. `kill-switch-enabled` stores the intent; `kill_switch_armed` reports whether the rules are actually installed, and a failed arm is surfaced in the TUI and `whoisthat doctor`.

### Capabilities are broad

`cap_net_admin` and `cap_net_raw` let the core — and its `ip`, `nft` and Xray children — alter interfaces, routes and firewall rules. Compromising the core is equivalent to compromising the host's network configuration. The daemon does not drop or further restrict these capabilities at runtime.

### Remote control

The TCP IPC transport binds loopback only. WhoisThat does not expose a remote management API; do not tunnel port 4897 to another machine.

---

## 4. Hardening Checklist

- Keep `db.key`, `config.json` and the database at `0600`/`0700`; back up the key separately from the data directory.
- Use full-disk encryption and a dedicated desktop user for VPN work.
- Leave `tcp-enabled` off unless you need it.
- Keep subscription URLs and `--reveal` output out of screenshots, tickets and shell history.
- Run `whoisthat doctor` after upgrades to confirm capabilities and socket permissions.
- Enable the kill-switch if you need fail-closed behaviour, and remember it is incompatible with `split-tunnel: include`.
- Treat provider Happ/JSON routing as code-like policy: it can direct or block your traffic.

---

For reporting a suspected vulnerability, see [Reporting a Bug](/reference/reporting) and avoid attaching keys, raw profile URIs or subscription tokens.
