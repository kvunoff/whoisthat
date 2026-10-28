# Security & Cryptography

Security in WhoisThat is designed around the principle of **least privilege**: minimizing root exposure, encrypting persistent state at rest, preventing traffic and DNS leaks, and isolating IPC channels.

---

## 1. Encrypted Database at Rest (AES-256-GCM)

All subscription credentials, server hostnames, tokens, and routing configurations are stored locally on disk under:

```
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

## 2. Rootless Network Management (Capabilities)

Traditionally, Linux VPN clients require `sudo`, setuid binaries, or a persistent root daemon. WhoisThat eliminates root privilege escalation entirely through **Linux File Capabilities**:

```
cap_net_admin,cap_net_raw,cap_setpcap=+ep
```

### How Ambient Capabilities Work:

```
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

## 3. Leak Prevention Architecture

### A. System-Wide DNS Hijacking
In TUN mode, misconfigured DNS servers or local resolvers can lead to DNS leaks. WhoisThat prevents this via kernel-level DNAT rules:
* All outbound UDP and TCP traffic on destination port 53 is intercepted.
* **Legacy mode** (manual routing, or profiles without DNS fields): queries are DNATed to the configured `dns-servers` and leave through the physical gateway.
* **Happ DNS mode**: queries are redirected to the sink addresses `198.18.0.53` / `fd00::53` and then resolved by Xray's tagged remote/domestic resolvers, so proxied domains are resolved through the VPN outbound.
* Split `include` disables the Happ hijack; the legacy hijack remains active for non-excluded traffic.

### B. Independent Kill-Switch (`whoisthat_ks`)
When the Kill-Switch setting is enabled:
* A dedicated nftables table (`inet whoisthat_ks`) is installed.
* All non-VPN traffic is unconditionally blocked on connection drops.
* Works independently of TUN routing rules.

### C. IPv6 Leak Prevention (Happy Eyeballs)
Modern operating systems and browsers implement the Happy Eyeballs algorithm (RFC 8305), attempting dual-stack IPv4 and IPv6 connections simultaneously.
* If a physical LAN lacks an IPv6 gateway but the VPN tunnel provides IPv6, applications in Split Tunnel `exclude` mode could inadvertently leak traffic through the tunnel's IPv6 default gateway.
* When no physical IPv6 gateway is detected, WhoisThat installs an **unreachable route** for IPv6 in Table 100 (`ip -6 route replace unreachable default table 100`). This immediately fails marked IPv6 attempts, forcing instant fallback to physical IPv4. If a physical IPv6 gateway exists, the mark is routed to that gateway instead.
