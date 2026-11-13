---
title: "Security & Cryptography"
description: "Encryption, capabilities, DNS handling, and the trust boundaries of WhoisThat."
audience: "developer"
type: "explanation"
---

# Security & Cryptography

Security in WhoisThat is designed around the principle of **least privilege**: minimizing root exposure, encrypting persistent state at rest, preventing traffic and DNS leaks, and isolating IPC channels.

---

## Encrypted database at rest (AES-256-GCM)

All subscription credentials, server hostnames, tokens, and routing configurations are stored locally on disk under:

```text
~/.local/share/whoisthat/db/
```

### Encryption Architecture

1. **Master Key Generation**: On first launch, the daemon generates a 32-byte cryptographically secure random key using Linux kernel entropy (`crypto/rand`).
2. **File Permissions**: The key is written to `~/.config/whoisthat/db.key` (or `$XDG_CONFIG_HOME/whoisthat/db.key`) with strict Unix permissions `0600`; the database directory is `0700`. Keeping the key outside the data directory means a backup of `~/.local/share/whoisthat/db/` does not also contain the material needed to decrypt it. Legacy installations are migrated automatically on first start, and a corrupted key is never silently overwritten.
3. **Atomic Writes**: Database files, the key and core `config.json` are written via a temporary file in the same directory, `fsync`, `rename`, and a directory `fsync`. A crash or power loss leaves either the previous or the new file, never a truncated one.
4. **AEAD Cipher**: Data is encrypted using **AES-256-GCM** (Galois/Counter Mode), providing both confidentiality and cryptographic integrity verification.
5. **Encrypted JSON Storage**: Files on disk are stored with an encrypted wrapper:
   ```json
   {
     "ciphertext": "<base64-encoded nonce + ciphertext + GCM tag>"
   }
   ```
6. **Tamper Detection**: If any encrypted file is modified externally or corrupted, decryption fails cleanly with an integrity authentication error, preventing invalid state injection.

---

## Rootless network management (capabilities)

Traditionally, Linux VPN clients require `sudo`, setuid binaries, or a persistent root daemon. WhoisThat eliminates root privilege escalation entirely through **Linux File Capabilities**:

```text
cap_net_admin,cap_net_raw,cap_setpcap=+ep
```

### How Ambient Capabilities Work:

```text
[whoisthat-core binary]  (Has +ep capabilities on disk)
       │
       ▼
  capset(2)              (Adds CAP_NET_ADMIN / CAP_NET_RAW to inheritable set)
       │
       ▼
  prctl(PR_CAP_AMBIENT_RAISE)  (Promotes caps to Ambient set)
       │
       ▼
  fork() & execve()      (Child processes automatically inherit caps)
       │
       ├─► ip tuntap add dev whoisthattun mode tun
       ├─► nft / iptables rules
       └─► xray-core (raw socket binding)
```

* **No setuid**: No binary runs with effective UID 0 (`root`).
* **Inherited Subprocesses**: The core process runs as your normal unprivileged user while granting child network utilities just enough authority to manage network interfaces.
* **Functional Verification**: The daemon verifies real capability functioning by creating and destroying a transient test adapter (`wt-capcheck`) before TUN setup, guaranteeing capabilities work before initiating connections. The TUI itself only inspects the binary with `getcap`.

---

## Leak prevention architecture

### System-wide DNS hijacking
In TUN mode, misconfigured DNS servers or local resolvers can lead to DNS leaks. WhoisThat prevents this via kernel-level DNAT rules:
* All outbound UDP and TCP traffic on destination port 53 is intercepted.
* **Legacy mode** (manual routing, or profiles without DNS fields): queries are DNATed to the configured `dns-servers` and leave through the physical gateway.
* **Happ DNS mode**: queries are redirected to the sink addresses `198.18.0.53` / `fd00::53` and then resolved by Xray's tagged remote/domestic resolvers, so proxied domains are resolved through the VPN outbound.
* Split `include` disables the Happ hijack; the legacy hijack remains active for non-excluded traffic.

The full flow, FakeDNS rules and known limits are in [DNS & FakeDNS](/networking/dns).

### Independent kill-switch (`whoisthat_ks`)
When the Kill-Switch setting is enabled:
* A dedicated nftables table (`inet whoisthat_ks`) is installed.
* All non-VPN traffic is unconditionally blocked on connection drops.
* Works independently of TUN routing rules.

### IPv6 leak prevention (Happy Eyeballs)
Modern operating systems and browsers implement the Happy Eyeballs algorithm (RFC 8305), attempting dual-stack IPv4 and IPv6 connections simultaneously.
* If a physical LAN lacks an IPv6 gateway but the VPN tunnel provides IPv6, applications in Split Tunnel `exclude` mode could inadvertently leak traffic through the tunnel's IPv6 default gateway.
* When no physical IPv6 gateway is detected, WhoisThat installs an **unreachable route** for IPv6 in Table 100 (`ip -6 route replace unreachable default table 100`). This immediately fails marked IPv6 attempts, forcing instant fallback to physical IPv4. If a physical IPv6 gateway exists, the mark is routed to that gateway instead.

---

## Threat model and limitations

WhoisThat is a local VPN client, not a sandbox. This section states what the
security measures protect against, what they assume, and where the boundaries
are.

### Trust boundaries

| Boundary | Assumption |
| :--- | :--- |
| Same Unix user | **Trusted.** Any process running as your UID can read your config/key and control the daemon. |
| `root` | **Trusted.** Root can read everything and change any rule. |
| Other local users | **Untrusted.** Protected by `0600` sockets/files and the TCP token. |
| Remote network | **Untrusted.** That is what the proxy/VPN is for. |
| Subscription providers | **Untrusted input.** URLs, headers, names and routing profiles are parsed defensively. |
| Xray release chain | **Trusted.** The pinned archive is checksum-verified; the checksum is fetched only from GitHub, never from the mirror. |

### What is protected

- **State at rest.** Profiles, subscription tokens and routing data are stored
  AES-256-GCM encrypted in `~/.local/share/whoisthat/db/`. The key lives outside
  that directory in `~/.config/whoisthat/db.key` (`0600`), so copying the data
  directory does not copy the key.
- **IPC.** The Unix socket is `0600` and restricted to your user. The opt-in TCP
  listener is loopback-only and requires a token from `ipc.token` (`0600`) plus a
  protocol match, with a 5-second handshake deadline.
- **Least privilege.** No setuid binary and no root daemon. The core runs as your
  user with `cap_net_admin,cap_net_raw,cap_setpcap`; capabilities are raised only
  when TUN needs them, and the TUI never runs with them.
- **Leak prevention.** TUN DNS hijack, `SO_MARK`/policy routing, the independent
  kill-switch and the IPv6 unreachable route are designed to keep traffic from
  escaping outside the intended path.
- **Provider input.** Happ payloads are size-capped (1 MiB) and geo downloads are
  HTTPS-only, size-capped and validated before use. Provider labels are stripped
  of control characters before they reach the terminal.

### What it does not protect

**Same-user compromise.** The Unix socket relies entirely on filesystem
permissions. Malware, a malicious script or a compromised editor running as your
user can connect to `core.sock` and control the VPN, read `db.key` and decrypt
the database, or read logs that may contain subscription URLs. There is no
per-command authorization or user confirmation.

**Encryption is only at rest.** The database key is stored on the same machine,
protected only by file mode. There is no passphrase, KDF or hardware-backed key.
Full-disk encryption and OS account security remain your responsibility. While
the core runs, decrypted credentials and the generated Xray configuration exist
in process memory and are passed to Xray over stdin.

**Logs and URLs.** Both the core and the TUI redact userinfo and query strings
before logging. A token embedded in the URL path is not masked. Log files are
`0600`, but treat `whoisthat logs`, `doctor --json` and `core.log` as sensitive
when pasting them into an issue.

**Update checks.** When `check_updates` is enabled (the default), the TUI
requests `github.com/kvunoff/whoisthat/releases/latest` at most once every 24
hours. The request carries a static `whoisthat-update-check` User-Agent and no
account, device or profile identifiers. It follows `HTTPS_PROXY` when set.
Disable it in Settings or with `WHOISTHAT_NO_UPDATE_CHECK=1`.

**DNS and routing limits.**

- Only destination port 53 is hijacked. DoH/DoT to another port, or applications
  with their own resolver, are not intercepted.
- `include` split mode deliberately leaves everything outside the split slice
  unprotected.
- `exclude` split apps are allowed through the kill-switch by design (`fwmark 1`).
- QUIC/HTTP-3 is not in the TUN sniffing list, so it can be routed by IP rather
  than by domain.
- FakeDNS does not map ICMP; a reply to `ping <fake-ip>` does not prove
  reachability.
- The kill-switch is an outbound firewall policy, not a guarantee against every
  leak. It filters locally-generated traffic only: forwarded traffic (Docker,
  VMs, NAT on this host) is not covered. `kill-switch-enabled` stores the intent;
  `kill_switch_armed` reports whether the rules are actually installed, and a
  failed arm is surfaced in the TUI and `whoisthat doctor`.

**Capabilities are broad.** `cap_net_admin` and `cap_net_raw` let the core — and
its `ip`, `nft` and Xray children — alter interfaces, routes and firewall rules.
Compromising the core is equivalent to compromising the host's network
configuration. The daemon does not drop or further restrict these capabilities
at runtime.

**Remote control.** The TCP IPC transport binds loopback only. WhoisThat does not
expose a remote management API; do not tunnel port 4897 to another machine.

### Hardening checklist

- Keep `db.key`, `config.json` and the database at `0600`/`0700`; back up the key
  separately from the data directory.
- Use full-disk encryption and a dedicated desktop user for VPN work.
- Leave `tcp-enabled` off unless you need it.
- Keep subscription URLs and `--reveal` output out of screenshots, tickets and
  shell history.
- Run `whoisthat doctor` after upgrades to confirm capabilities and socket
  permissions.
- Enable the kill-switch if you need fail-closed behaviour, and remember it is
  incompatible with `split-tunnel: include`.
- Treat provider Happ/JSON routing as code-like policy: it can direct or block
  your traffic.

For reporting a suspected vulnerability, see
[Reporting a bug](/developers/#reporting-a-bug) and avoid attaching keys, raw
profile URIs or subscription tokens.
