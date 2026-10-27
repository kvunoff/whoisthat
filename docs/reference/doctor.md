# Doctor Diagnostics (`whoisthat doctor`)

WhoisThat features a built-in diagnostic and health inspection suite accessible via the command line:

```bash
whoisthat doctor
```

For programmatic monitoring, desktop widget checks, or CI pipelines, output machine-readable JSON:

```bash
whoisthat doctor --json
```

---

## The 6 Diagnostic Categories

The diagnostic engine performs comprehensive, non-destructive system inspections divided into six categories implemented in `src/doctor/checks.rs`:

```
┌────────────────────────────────────────────────────────┐
│                  whoisthat doctor                      │
├────────────────────────────────────────────────────────┤
│  1. Binaries & Runtimes     (whoisthat, core, xray)    │
│  2. Linux Capabilities      (cap_net_admin, /dev/net)  │
│  3. Polkit & Permissions    (polkit, pkexec, linger)   │
│  4. IPC & Daemon Runtime    (UDS socket, handshake)    │
│  5. DNS & Routing           (sink, pools, hijack, KS)  │
│  6. Storage & Cryptography  (DB 0700, .key, geoip)     │
└────────────────────────────────────────────────────────┘
```

### 1. Binaries & Runtimes
* **`whoisthat` (Client)**: Resolves the active binary path (`std::env::current_exe()`) and confirms it matches the compiled crate version (`CARGO_PKG_VERSION`).
* **`whoisthat-core` (Daemon)**: Searches `PATH`, current executable directory, `./core/core/`, `/usr/local/bin`, and `/usr/bin` for the daemon binary.
* **`xray-core` (Proxy Runtime)**: Probes for available Xray-core runtimes in `~/.local/share/whoisthat/runtimes/xray/` and system `PATH`. Checks if version matches the pinned **v26.9.9** release.
* **System CLI Tools**: Validates the presence of essential Linux networking utilities: `ip` (`iproute2`), `getcap` & `setcap` (`libcap`), and `pkexec` (`polkit`).

### 2. Linux Capabilities & Kernel
* **`whoisthat-core caps`**: Runs `getcap` against the daemon binary to verify all three required capabilities are granted:
  * `cap_net_admin`: Required for creating virtual TUN adapters and manipulating routing tables.
  * `cap_net_raw`: Required for raw socket binding and packet marking.
  * `cap_setpcap`: Required for elevating capabilities into the ambient set.
* **`/dev/net/tun` Device**: Confirms `/dev/net/tun` exists and is a valid character device with read/write access.
* **Firewall Backend**: Detects whether modern **nftables** or legacy **iptables** is active. Warns if neither is available.
* **cgroup v2 Controller**: Checks for `/sys/fs/cgroup/cgroup.controllers`. Confirms the unified cgroup v2 hierarchy is active for application split tunneling (`whoisthat run`).

### 3. Polkit & User Permissions
* **Polkit Daemon**: Checks if `polkit.service` is active via `systemctl is-active`.
* **`pkexec` Helper**: Confirms `pkexec` binary is installed to allow graphical authentication dialogs during first-time TUN setup.
* **Systemd User Linger**: Runs `loginctl show-user $USER --property=Linger` to check if user services can start at boot before interactive login.

### 4. IPC & Daemon Runtime Status
* **`XDG_RUNTIME_DIR`**: Validates the user runtime directory environment variable and permissions.
* **Unix Domain Socket**: Resolves `$XDG_RUNTIME_DIR/whoisthat/core.sock`, verifying that file mode is `0600` and owned by the current user.
* **Daemon Handshake**: Transmits an IPC `get-application-state` probe to verify the daemon is responsive and reports wire protocol version `3`.
* **Legacy TCP Port**: If TCP is enabled, checks if `127.0.0.1:4897` is accessible.

### 5. DNS & Routing
* **DNS sink address**: Verifies that `198.18.0.53` / `fd00::53` are **not** assigned to any interface. If they are, kernel local delivery bypasses the TUN inbound and DNS hijack stops working.
* **FakeDNS pools**: Checks the TUN interface addresses against `198.19.0.0/16` and `fc00::/18`; an overlap means real traffic could collide with fake addresses.
* **DNS hijack rules**: Inspects the `ip whoisthat` / `ip6 whoisthat` nftables tables (or the iptables `WHOISTHAT_DNS_*` chains) and reports the active mode: Happ sink, legacy resolver, disabled in `include`, or TUN inactive.
* **DNS safety returns**: In Happ mode, verifies that the `mark 1` and sink return rules are present. Missing returns can loop Xray's direct DNS or capture split-excluded apps.
* **systemd-resolved**: If `resolvectl` is installed, verifies that the TUN interface points at the Happ sink. A missing or inactive systemd-resolved is informational, not a warning or failure — kernel DNAT still works.
* **Kill-switch DNS allowlist**: When the kill-switch is armed, checks that its nftables table still allows the TUN interface, the DNS sink, and `mark 1`.

::: tip Root permissions
Reading nftables/iptables state usually requires root. When `whoisthat doctor` runs unprivileged, firewall items are reported as `info` with the exact inspection command (`sudo nft list table ip whoisthat`) instead of failing.
:::

### 6. Storage, Database & Cryptography
* **Database Directory**: Validates that `~/.local/share/whoisthat/db/` exists and has secure Unix permissions (`0700`).
* **Master Encryption Key**: Inspects `~/.config/whoisthat/db.key`:
  * Ensures permissions are strictly `0600`.
  * Verifies key length is exactly 32 bytes (256 bits).
* **Configuration Syntax**: Parses `~/.config/whoisthat/config.toml` (TUI) and `~/.config/whoisthat/config.json` (Core) for JSON/TOML syntax validity.
* **Geo Assets Verification**: Confirms `geoip.dat` and `geosite.dat` exist in `~/.config/whoisthat/geo/` and meet the minimum 10 MB size threshold.

---

## Machine-Readable JSON Output Format

When running `whoisthat doctor --json`:

```json
{
  "client_version": "0.14.3",
  "core_version": "0.3.3",
  "core_protocol_version": 3,
  "categories": [
    {
      "name": "Binaries & Runtimes",
      "items": [
        {
          "name": "whoisthat",
          "status": "pass",
          "message": "/usr/local/bin/whoisthat (v0.14.3)",
          "fix": null
        },
        {
          "name": "whoisthat-core",
          "status": "pass",
          "message": "/usr/local/bin/whoisthat-core",
          "fix": null
        },
        {
          "name": "xray-core",
          "status": "pass",
          "message": "~/.local/share/whoisthat/runtimes/xray/xray (v26.9.9)",
          "fix": null
        }
      ]
    },
    {
      "name": "Linux Capabilities & Kernel",
      "items": [
        {
          "name": "whoisthat-core caps",
          "status": "pass",
          "message": "/usr/local/bin/whoisthat-core (=ep)",
          "fix": null
        },
        {
          "name": "/dev/net/tun",
          "status": "pass",
          "message": "available (char device, mode 0666)",
          "fix": null
        }
      ]
    }
  ],
  "summary": {
    "total": 18,
    "passed": 18,
    "warnings": 0,
    "failures": 0
  }
}
```

---

## Remediation Guide for Failures

| Failure / Warning | Technical Cause | Immediate Fix Command |
| :--- | :--- | :--- |
| **`whoisthat-core not found`** | Daemon binary missing from standard PATH directories. | `sudo install -Dm755 core/core/whoisthat-core /usr/local/bin/whoisthat-core` |
| **`missing required network capabilities`** | Rebuilt daemon lost file capabilities on new inode. | `sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep $(which whoisthat-core)` |
| **`virtual device /dev/net/tun not found`** | Linux kernel `tun` module is not loaded. | `sudo modprobe tun && echo "tun" \| sudo tee -a /etc/modules-load.d/tun.conf` |
| **`neither nftables nor iptables found`** | Host lacks a compatible firewall backend. | Arch: `sudo pacman -S nftables`<br>Debian/Ubuntu: `sudo apt install nftables` |
| **`systemd linger disabled`** | User services will terminate upon graphical logout. | `sudo loginctl enable-linger $USER` |
| **`IPC connection refused`** | Daemon is offline or crashed. | `systemctl --user restart whoisthat-core` or `whoisthat-core &` |
| **`Invalid .key permissions`** | Master key permissions are broader than user-only. | `chmod 0600 ~/.config/whoisthat/db.key` |
| **`Geo assets missing or corrupt`** | `geoip.dat` or `geosite.dat` smaller than 10 MB. | Remove damaged files and restart client: `rm -rf ~/.config/whoisthat/geo/*` |
