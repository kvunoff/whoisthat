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

For server deployments, CI runners, or scripted setups, pass the `--yes` flag:

```bash
curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash -s -- --yes
```

**What the install script does:**
1. Checks for system tools (`curl`, `tar`, `git`, C compiler, `setcap`, `pkexec`).
2. Installs or adopts precompiled **Xray-core (pinned v26.9.9)** with SHA256 checksum verification.
3. Builds `whoisthat-core` (Go) and `whoisthat` (Rust).
4. Installs binaries into `/usr/local/bin/`.
5. Grants `cap_net_admin,cap_net_raw,cap_setpcap=+ep` to `/usr/local/bin/whoisthat-core`.
6. Registers the `whoisthat.desktop` file and `wit://` protocol handler with `xdg-mime`.

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
* **Rust**: `1.80+` ([rustup.rs](https://rustup.rs/))
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

* **Automatic Download & Verification**: On startup, `whoisthat-core` inspects `~/.local/share/whoisthat/runtimes/xray/`. If not present, it downloads the official precompiled release archive directly from GitHub, computes its SHA-256 hash against the hardcoded release checksum, unpacks the binary, and marks it executable.
* **System Binary Adoption**: If `/usr/local/bin/xray` or `/usr/bin/xray` already exists and matches or exceeds the required version, the core automatically adopts the system binary.
* **Native Protocols**: Because Hysteria2 protocol support is compiled directly into modern Xray-core, **no separate hysteria binary is required**.
* **Missing-Binary UX & Hints**: On startup, the core daemon proactively tests for Xray availability. If missing and automatic download cannot proceed (e.g. offline setup), the core unicasts a high-priority `warn` event to the TUI with an actionable installation hint, and connect-time errors forward the exact underlying `err.Error()` to the interface.

---

## 5. Network Capabilities (`setcap`)

TUN mode creates virtual network interfaces and manipulates routing rules without needing `root` privileges. This is achieved via Linux file capabilities:

```bash
sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /path/to/whoisthat-core
```

::: tip One-Time Setup Prompt
If you run `whoisthat` and attempt to activate TUN mode without capabilities set, the TUI detects the missing privileges and automatically displays an interactive `pkexec` dialog to configure capabilities with a single password prompt.
:::

---

## 6. Verifying Your Installation (`whoisthat doctor`)

After installing, run the built-in diagnostic tool to verify all binaries, permissions, capabilities, and socket directories:

```bash
whoisthat doctor
```

Example successful output:

```
=== WhoisThat Doctor ===

[✓] Binaries
  • whoisthat: /usr/local/bin/whoisthat (v0.13.0)
  • whoisthat-core: /usr/local/bin/whoisthat-core

[✓] Capabilities
  • cap_net_admin: granted
  • cap_net_raw: granted
  • cap_setpcap: granted

[✓] Polkit & Helper Tools
  • pkexec: /usr/bin/pkexec
  • iproute2: /usr/bin/ip
  • firewall backend: nftables

[✓] IPC & Daemon Status
  • Socket path: /run/user/1000/whoisthat/core.sock
  • Daemon: active (protocol v2, core v0.2.0)

[✓] Storage & Cryptography
  • Database directory: ~/.local/share/whoisthat/db/ (0700)
  • Master key: ~/.local/share/whoisthat/db/.key (0600)

All checks passed! System is ready.
```
