# Installation

WhoisThat can be installed via an automated installer script, a distribution package (`.deb` / `.rpm`), the Ubuntu PPA, the Arch User Repository (AUR), or compiled manually from source.

::: warning Pick one installation method
The installer, the `.deb`/`.rpm` packages, the Ubuntu PPA and the AUR packages
all own the same files under `/usr/bin`. Mixing them causes conflicts and
package-manager warnings. The AUR source and prebuilt packages also conflict
with each other.
The installer refuses to overwrite package-managed binaries, and the AUR
packages refuse to install over a binary that pacman does not own (an
`install.sh` or manual install). Remove the other installation first.
:::

---

## 1. Quick Install (Any Linux Distribution)

The installer detects your CPU architecture (`x86_64` or `aarch64`), downloads the
prebuilt **static musl** binaries from the latest GitHub release, verifies their
SHA-256 checksums, and configures required network capabilities. No Rust or Go
toolchain is needed.

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

The script is always fetched from `main`. To install a specific release, pass
`--release vX.Y.Z`. To build locally instead of downloading, pass
`--from-source` (optionally with `--branch vX.Y.Z` or `--local`).

**What the install script does:**
1. Installs runtime prerequisites (`curl`, `unzip`, `libcap`/`setcap`, `iproute2`, `nftables`, CA certificates). `pkexec`/`iptables` are optional and reported by `whoisthat doctor`.
2. Downloads `whoisthat` and `whoisthat-core` from the release bundle and verifies `SHA256SUMS`. The installer **fails closed** on a missing or mismatched checksum (override only with the explicit `--insecure-skip-verify` flag).
3. Installs or adopts precompiled **Xray-core (pinned v26.10.31)** with the same checksum policy.
4. Installs binaries into `/usr/bin/` and removes legacy copies from `/usr/local/bin/`.
5. Grants `cap_net_admin,cap_net_raw,cap_setpcap=+ep` to `/usr/bin/whoisthat-core`.
6. Installs `whoisthat.desktop` (application menu launcher) and `whoisthat-url.desktop` (hidden `wit://` handler) into `/usr/share/applications`, plus `whoisthat.svg` into the icon theme, and refreshes the desktop and icon caches. It then runs `whoisthat link register` as the invoking user: this refreshes the per-user entries left by older versions and binds `wit://` with `xdg-mime`. The TUI re-verifies the registration on every launch.
7. Generates shell completions (bash, zsh, fish) and the man page from the installed binary and writes them to `/usr/share/bash-completion/completions/whoisthat`, `/usr/share/zsh/site-functions/_whoisthat`, `/usr/share/fish/vendor_completions.d/whoisthat.fish` and `/usr/share/man/man1/whoisthat.1`. No shell rc file is edited.

Every release archive also carries a GitHub build-provenance attestation. To
check a downloaded archive before installing:

```bash
gh attestation verify whoisthat-linux-amd64.tar.gz --repo kvunoff/whoisthat
```

If a distribution package owns the binaries, the installer refuses to overwrite
them; upgrade through the package manager instead.

---

## 2. Distribution Packages

Every release publishes `.deb` and `.rpm` packages for `amd64`/`x86_64` and
`arm64`/`aarch64` alongside the installer archives.

### Ubuntu PPA

Ubuntu 24.04 LTS (Noble) users can install and upgrade through the PPA:

```bash
sudo add-apt-repository ppa:kvunoff/whoisthat
sudo apt update
sudo apt install whoisthat
```

Verify that the package comes from the PPA and that both binaries run:

```bash
apt-cache policy whoisthat
whoisthat --version
whoisthat-core --version
```

The PPA ships the same static binaries as the release archives and sets the
required file capabilities in its post-install script, so no manual `setcap`
step is needed. It currently publishes Noble `amd64` builds; other series and
`arm64` can be enabled in the PPA settings, or use the release `.deb` or the
installer. Do not mix the PPA with the GitHub `.deb`, the installer or the AUR
package: all of them own the same files under `/usr/bin`.

### Direct `.deb` / `.rpm`

```bash
# Debian / Ubuntu
sudo apt install ./whoisthat_<version>-1_amd64.deb

# Fedora / RHEL
sudo dnf install ./whoisthat-<version>-1.x86_64.rpm
```

Both formats install `/usr/bin/whoisthat`, `/usr/bin/whoisthat-core`, the two
desktop entries (`whoisthat.desktop`, `whoisthat-url.desktop`), the icon and the
license, and re-apply file capabilities plus desktop/icon cache refresh in their
post-install script. User configuration and the encrypted database are never
touched by package upgrades or removal.

---

## 3. Arch Linux (AUR)

WhoisThat is available in two flavours:

* [`whoisthat-bin`](https://aur.archlinux.org/packages/whoisthat-bin) — prebuilt
  release binaries (recommended);
* [`whoisthat`](https://aur.archlinux.org/packages/whoisthat) — builds from
  source.

```bash
# Using paru
paru -S whoisthat-bin

# Using yay
yay -S whoisthat-bin
```

Both packages install the same files:
* `/usr/bin/whoisthat` (client TUI / CLI)
* `/usr/bin/whoisthat-core` (VPN daemon with capabilities pre-configured)
* `/usr/share/applications/whoisthat.desktop` (application menu launcher, opens the TUI)
* `/usr/share/applications/whoisthat-url.desktop` (hidden `wit://` deep link handler)
* `/usr/share/icons/hicolor/scalable/apps/whoisthat.svg`
* `/usr/share/bash-completion/completions/whoisthat`
* `/usr/share/zsh/site-functions/_whoisthat`
* `/usr/share/fish/vendor_completions.d/whoisthat.fish`
* `/usr/share/man/man1/whoisthat.1` (`install.sh`) or `whoisthat.1.gz` (packages compress man pages)

They conflict with each other, so pacman will prompt to remove the installed one
when you switch between `whoisthat` and `whoisthat-bin`.

If WhoisThat was installed with `install.sh`, the package aborts before touching
any files. Remove the script install first:

```bash
curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | sudo bash -s -- --uninstall
```

---

## 4. Manual Build From Source

### Prerequisites

Ensure you have the following installed on your machine:
* **Rust**: `1.88` or newer with edition 2024 support ([rustup.rs](https://rustup.rs/))
* **Go**: `1.26.11+` ([go.dev](https://go.dev/))
* **Git**, **curl**, and a C compiler (`gcc` or `clang`)
* **iproute2** and either **nftables** or **iptables**

Runtime requirements for the full feature set:

| Requirement | Needed for |
| :--- | :--- |
| Linux with `/dev/net/tun` | TUN mode |
| `nftables` (recommended) | Split tunneling, kill-switch, DNS hijack |
| `iptables` | Legacy fallback for kill-switch and DNS hijack (split tunneling still needs `nftables`) |
| cgroup v2 (`/sys/fs/cgroup/cgroup.controllers`) | Split tunneling (`whoisthat run`) |
| systemd user manager **or** OpenRC/dinit/runit | Autostart and split-tunnel launching (see [Non-systemd systems](/guide/non-systemd)) |
| `libcap` (`setcap`/`getcap`) | Rootless TUN via file capabilities |
| `x86_64` or `aarch64` | Prebuilt Xray runtime |

Proxy mode only needs the binaries and a reachable server; TUN, split tunneling and the kill-switch require the kernel and firewall pieces above.

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
cargo build --release --locked

# 4. Install binaries
sudo install -Dm755 target/release/whoisthat /usr/bin/whoisthat
sudo install -Dm755 core/core/whoisthat-core /usr/bin/whoisthat-core

# 5. Grant network capabilities to the core daemon
sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /usr/bin/whoisthat-core

# 6. Install desktop launcher, icon and register wit:// deep link handler (optional)
install -Dm644 pkg/whoisthat.desktop ~/.local/share/applications/whoisthat.desktop
install -Dm644 pkg/whoisthat-url.desktop ~/.local/share/applications/whoisthat-url.desktop
install -Dm644 docs/public/logo.svg ~/.local/share/icons/hicolor/scalable/apps/whoisthat.svg
update-desktop-database ~/.local/share/applications
xdg-mime default whoisthat-url.desktop x-scheme-handler/wit
```

---

## 5. Xray-Core Runtime Management

WhoisThat relies on a unified proxy engine: **Xray-core v26.10.31**.

* **Automatic Download & Verification**: On startup, `whoisthat-core` inspects `~/.local/share/whoisthat/runtimes/xray/v26.10.31/`. If the pinned binary is not present, it downloads the official precompiled release archive (GitHub first, then the `ghfast.top` mirror), fetches the official `.dgst` file **only from GitHub**, verifies SHA-256, and only then unpacks the binary. A missing or mismatched checksum aborts the install.
* **System Binary Adoption**: If `/usr/local/bin/xray` or `/usr/bin/xray` already exists and exactly matches the pinned version, the core automatically adopts the system binary.
* **Native Protocols**: Because Hysteria2 protocol support is compiled directly into modern Xray-core, **no separate hysteria binary is required**.
* **Missing-Binary UX & Hints**: On startup, the core daemon proactively tests for Xray availability. If missing and automatic download cannot proceed (e.g. offline setup), the core unicasts a high-priority `warn` event to the TUI with an actionable installation hint, and connect-time errors forward the exact underlying `err.Error()` to the interface.

---

## 6. Network Capabilities (`setcap`)

TUN mode creates virtual network interfaces and manipulates routing rules without needing `root` privileges. This is achieved via Linux file capabilities:

```bash
sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /path/to/whoisthat-core
```

::: tip Capability Setup Prompt
If `whoisthat` starts (or spawns) the daemon without the required capabilities **and** TUN autostart is configured (`autoconnect-mode = "tun"`), it offers a one-time `pkexec` prompt in the terminal before the TUI appears. In every other case (proxy-only, no autostart, or no terminal) there is no prompt: toggling TUN in the TUI reports the missing capabilities and points to the `setcap` command above.
:::

---

## 7. Verifying Your Installation (`whoisthat doctor`)

After installing, run the built-in diagnostic tool to verify all binaries, permissions, capabilities, and socket directories:

```bash
whoisthat doctor
```

Example successful output:

```
WhoisThat Doctor v0.18.7 / core v0.5.3 (protocol 5) — System Diagnostics

[+] Binaries & Runtimes
  ✓ whoisthat: /usr/bin/whoisthat (v0.18.7)
  ✓ whoisthat-core: /usr/bin/whoisthat-core
  ✓ xray-core: ~/.local/share/whoisthat/runtimes/xray/v26.10.31/xray (v26.10.31)
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
