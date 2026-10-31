# Installation

WhoisThat can be installed via an automated installer script, from the Arch User Repository (AUR), or compiled manually from source.

---

## 1. Quick Install (Any Linux Distribution)

The automated installation script detects your CPU architecture (`x86_64` or `aarch64`), verifies or installs required build tools (Rust and Go from official upstream sources), builds the latest release binaries, and configures required network capabilities.

### Interactive Installation

```bash
curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash
```

### Unattended / Non-Interactive Installation

The installer never prompts, so it can run from CI or a provisioning script as is.
`--yes` is still accepted for compatibility:

```bash
curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash -s -- --yes
```

The script is always fetched from `main`, while the build uses the latest release
tag. To install a specific release, pass `--branch vX.Y.Z`, or download
`install.sh` from that tag.

**What the install script does:**
1. Installs distribution build and runtime prerequisites (C compiler/make, `git`, `curl`, `unzip`, `libcap`/`setcap`, `iproute2`, `nftables`). `pkexec`/`iptables` are optional and reported by `whoisthat doctor`.
2. Installs or adopts precompiled **Xray-core (pinned v26.9.9)** with SHA256 checksum verification. The installer **fails closed**: a missing or mismatched official checksum aborts the installation (override only with the explicit `--insecure-skip-verify` flag).
3. Builds `whoisthat-core` (Go) and `whoisthat` (Rust).
4. Installs binaries into `/usr/local/bin/`.
5. Grants `cap_net_admin,cap_net_raw,cap_setpcap=+ep` to `/usr/local/bin/whoisthat-core`.
6. Installs `whoisthat.desktop` into `/usr/share/applications` and refreshes the desktop database. The `wit://` scheme handler is registered per-user by the TUI on launch (`xdg-mime`), not by the installer.

---

## 2. Arch Linux (AUR)

WhoisThat is available on the Arch User Repository as [`whoisthat`](https://aur.archlinux.org/packages/whoisthat).

```bash
# Using paru
paru -S whoisthat

# Using yay
yay -S whoisthat
```

The AUR package automatically installs:
* `/usr/bin/whoisthat` (client TUI / CLI)
* `/usr/bin/whoisthat-core` (VPN daemon with capabilities pre-configured)
* `/usr/share/applications/whoisthat.desktop` (desktop entry with `wit://` deep link handler)

---

## 3. Manual Build From Source

### Prerequisites

Ensure you have the following installed on your machine:
* **Rust**: recent stable toolchain (edition 2021, [rustup.rs](https://rustup.rs/))
* **Go**: `1.26+` ([go.dev](https://go.dev/))
* **Git**, **curl**, and a C compiler (`gcc` or `clang`)
* **iproute2** and either **nftables** or **iptables**

### Step-by-Step Compilation

```bash
# 1. Clone the repository
git clone https://github.com/kvunoff/whoisthat.git
cd whoisthat

# 2. Build the Core Daemon (Go)
cd core/core
go build -o whoisthat-core
cd ../..

# 3. Build the TUI Client (Rust)
cargo build --release

# 4. Install binaries
sudo install -Dm755 target/release/whoisthat /usr/local/bin/whoisthat
sudo install -Dm755 core/core/whoisthat-core /usr/local/bin/whoisthat-core

# 5. Grant network capabilities to the core daemon
sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /usr/local/bin/whoisthat-core

# 6. Install desktop launcher and register wit:// deep link handler (optional)
cp pkg/whoisthat.desktop ~/.local/share/applications/
xdg-mime default whoisthat.desktop x-scheme-handler/wit
```

---

## 4. Xray-Core Runtime Management

WhoisThat relies on a unified proxy engine: **Xray-core v26.9.9**.

* **Automatic Download & Verification**: On startup, `whoisthat-core` inspects `~/.local/share/whoisthat/runtimes/xray/v26.9.9/`. If the pinned binary is not present, it downloads the official precompiled release archive (GitHub first, then the `ghfast.top` mirror), fetches the official `.dgst` file **only from GitHub**, verifies SHA-256, and only then unpacks the binary. A missing or mismatched checksum aborts the install.
* **System Binary Adoption**: If `/usr/local/bin/xray` or `/usr/bin/xray` already exists and exactly matches the pinned version, the core automatically adopts the system binary.
* **Native Protocols**: Because Hysteria2 protocol support is compiled directly into modern Xray-core, **no separate hysteria binary is required**.
* **Missing-Binary UX & Hints**: On startup, the core daemon proactively tests for Xray availability. If missing and automatic download cannot proceed (e.g. offline setup), the core unicasts a high-priority `warn` event to the TUI with an actionable installation hint, and connect-time errors forward the exact underlying `err.Error()` to the interface.

---

## 5. Network Capabilities (`setcap`)

TUN mode creates virtual network interfaces and manipulates routing rules without needing `root` privileges. This is achieved via Linux file capabilities:

```bash
sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /path/to/whoisthat-core
```

::: tip Capability Setup Prompt
If `whoisthat` starts (or spawns) the daemon without the required capabilities **and** TUN autostart is configured (`autoconnect-mode = "tun"`), it offers a one-time `pkexec` prompt in the terminal before the TUI appears. In every other case (proxy-only, no autostart, or no terminal) there is no prompt: toggling TUN in the TUI reports the missing capabilities and points to the `setcap` command above.
:::

---

## 6. Verifying Your Installation (`whoisthat doctor`)

After installing, run the built-in diagnostic tool to verify all binaries, permissions, capabilities, and socket directories:

```bash
whoisthat doctor
```

Example successful output:

```
WhoisThat Doctor v0.15.2 / core v0.3.9 (protocol 3) — System Diagnostics

[+] Binaries & Runtimes
  ✓ whoisthat: /usr/local/bin/whoisthat (v0.15.2)
  ✓ whoisthat-core: /usr/local/bin/whoisthat-core
  ✓ xray-core: ~/.local/share/whoisthat/runtimes/xray/v26.9.9/xray (v26.9.9)
  ✓ system tools: ip, getcap, setcap, pkexec available

[+] Linux Capabilities & Kernel
  ✓ whoisthat-core caps: cap_net_admin,cap_net_raw,cap_setpcap=+ep
  ✓ /dev/net/tun: available (char device, mode 0666)
  ✓ firewall backend: nftables active (recommended)
  ✓ cgroup v2: available — split-tunneling supported

[+] Polkit & Permissions
  ✓ polkit daemon: active (polkit.service)
  ✓ pkexec: /usr/bin/pkexec
  ℹ systemd linger: disabled for user 'kvunoff' (enable with: loginctl enable-linger kvunoff ...)

[+] IPC & Daemon
  ✓ XDG_RUNTIME_DIR: /run/user/1000
  ✓ IPC socket: /run/user/1000/whoisthat/core.sock (mode 0600, secure)
  ✓ daemon connection: active & responsive (VPN: connected, TUN active, profile: #2 (Frankfurt))
  ℹ systemd user service: whoisthat-core.service (status: inactive, autostart: disabled)

[+] DNS & Routing
  ✓ DNS sink address: 198.18.0.53 / fd00::53 not assigned
  ✓ DNS hijack rules: legacy resolver
  ...

[+] Configuration & Storage
  ✓ TUI config: ~/.config/whoisthat/config.toml (theme: graphite-dark, test: http-get)
  ✓ core config: ~/.config/whoisthat/config.json (SOCKS: 3090, HTTP: 3091)
  ✓ encrypted db: ~/.local/share/whoisthat/db (AES-256-GCM key present at ~/.config/whoisthat/db.key)
  ✓ geo assets: geoip.dat (11.8 MB), geosite.dat (4.2 MB)

Result: 21 passed, 0 warnings, 0 failed, 3 info. All systems operational!
```

Categories and item counts vary with the host; the exact JSON schema is documented in [Doctor Diagnostics](/reference/doctor).
