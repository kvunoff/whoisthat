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
│  1. Binaries & Runtimes          (whoisthat, core, xray)│
│  2. Linux Capabilities & Kernel  (caps, /dev/net, fw)  │
│  3. Polkit & Permissions         (polkit, pkexec, linger)│
│  4. IPC & Daemon                 (UDS socket, handshake)│
│  5. DNS & Routing               (sink, pools, hijack, KS)│
│  6. Configuration & Storage      (DB key, configs, geo) │
└────────────────────────────────────────────────────────┘
```

### 1. Binaries & Runtimes
* **`whoisthat` (Client)**: Resolves the active binary path (`std::env::current_exe()`) and confirms it matches the compiled crate version (`CARGO_PKG_VERSION`).
* **`whoisthat-core` (Daemon)**: Searches `PATH`, `./whoisthat-core`, `./core/core/whoisthat-core`, `./bin/whoisthat-core`, `/usr/local/bin`, and `/usr/bin` (in that order) for the daemon binary.
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
* **Daemon Handshake**: Transmits an IPC `get-application-state` probe to verify the daemon is responsive. The reported `core_version`/`protocol_version` are shown in the report header but are not interpreted as a pass/fail gate.
* **Systemd User Service**: Reports `systemctl --user is-active` / `is-enabled` for `whoisthat-core.service` as informational.
* **Legacy TCP Port**: If TCP is enabled, reports the configured `host:port`.

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

### 6. Configuration & Storage
* **TUI Config**: Confirms `~/.config/whoisthat/config.toml` exists and echoes the active theme/test method. It does not re-parse the file (the TUI already does that with a backup/fallback on error).
* **Core Config**: Parses `~/.config/whoisthat/config.json` as JSON and reports the SOCKS/HTTP ports; invalid JSON is a warning because the core recreates defaults.
* **Encrypted DB**: Checks that `~/.local/share/whoisthat/db/` exists and inspects the key (`~/.config/whoisthat/db.key`, legacy `db/.key`): exactly 32 bytes and mode `0600`. The DB directory mode itself is not asserted.
* **Geo Assets**: Confirms `geoip.dat` and `geosite.dat` exist in `~/.config/whoisthat/geo/` and warns when `geoip.dat` is under 10 MiB. `geosite.dat` size is reported but not thresholded (the core's own floor is 1 MiB).

---

## Machine-Readable JSON Output Format

When running `whoisthat doctor --json`:

```json
{
  "version": "0.14.5",
  "core_version": "0.3.7",
  "core_protocol_version": 3,
  "categories": [
    {
      "title": "Binaries & Runtimes",
      "items": [
        {
          "name": "whoisthat",
          "status": "pass",
          "message": "/usr/local/bin/whoisthat (v0.14.5)"
        },
        {
          "name": "whoisthat-core",
          "status": "pass",
          "message": "/usr/local/bin/whoisthat-core"
        },
        {
          "name": "xray-core",
          "status": "pass",
          "message": "~/.local/share/whoisthat/runtimes/xray/v26.9.9/xray (v26.9.9)"
        }
      ]
    },
    {
      "title": "Linux Capabilities & Kernel",
      "items": [
        {
          "name": "whoisthat-core caps",
          "status": "warn",
          "message": "missing required network capabilities for rootless TUN mode",
          "fix_hint": "sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /usr/local/bin/whoisthat-core"
        }
      ]
    }
  ],
  "summary": {
    "passed": 18,
    "warnings": 1,
    "failed": 0,
    "info": 2,
    "is_healthy": true
  }
}
```

`fix_hint` is omitted entirely when there is no suggested fix. `summary.is_healthy` is `true` when `failed == 0`.

---

## Remediation Guide for Failures

| Failure / Warning | Technical Cause | Immediate Fix Command |
| :--- | :--- | :--- |
| **`daemon binary not found in PATH or standard directories`** | Daemon binary missing from standard PATH directories. | `sudo install -Dm755 core/core/whoisthat-core /usr/local/bin/whoisthat-core` |
| **`missing required network capabilities for rootless TUN mode`** | Rebuilt daemon lost file capabilities on new inode. | `sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep $(which whoisthat-core)` |
| **`virtual device /dev/net/tun not found`** | Linux kernel `tun` module is not loaded. | `sudo modprobe tun && echo "tun" \| sudo tee -a /etc/modules-load.d/tun.conf` |
| **`neither nftables nor iptables found`** | Host lacks a compatible firewall backend. | Arch: `sudo pacman -S nftables`<br>Debian/Ubuntu: `sudo apt install nftables` |
| **`systemd linger: disabled`** (Info) | User services will stop on logout; boot autostart will not work. | `sudo loginctl enable-linger $USER` |
| **`daemon connection: core is not running`** (Info) | Daemon is offline or crashed. | `systemctl --user start whoisthat-core` or launch `whoisthat` |
| **`key file ... has mode ... (expected 0600)`** | Master key permissions are broader than user-only. | `chmod 0600 ~/.config/whoisthat/db.key` |
| **`geoip.dat (... is under 10MB)`** | Global GeoIP database is truncated or stale. | Remove damaged files and restart the client: `rm -rf ~/.config/whoisthat/geo/*` |
