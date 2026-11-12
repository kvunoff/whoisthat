---
title: "Installation"
description: "Install WhoisThat from the install script, distribution packages, AUR, or source."
audience: "user"
type: "tutorial"
---

# Installation

WhoisThat can be installed with the universal script, a distribution package
(`.deb` / `.rpm`), the Ubuntu PPA, the Arch User Repository, or a manual build.

::: warning Pick one installation method
The installer, the `.deb`/`.rpm` packages, the Ubuntu PPA and the AUR packages
all own the same files under `/usr/bin`. Mixing them causes conflicts and
package-manager warnings. The AUR source and prebuilt packages also conflict
with each other. Remove the previous installation before switching.
:::

## Install script

The installer detects `x86_64` or `aarch64`, downloads the prebuilt static musl
binaries from the latest GitHub release, verifies their SHA-256 checksums, and
configures network capabilities. No Rust or Go toolchain is required.

```bash
curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash
```

The installer is non-interactive. `--yes` is accepted for compatibility. Useful
flags:

| Flag | Effect |
| :--- | :--- |
| `--release vX.Y.Z` | Install a specific release instead of the latest one |
| `--from-source` | Build from source instead of downloading the release bundle |
| `--branch vX.Y.Z` | Build a specific tag or branch (implies `--from-source`) |
| `--local` | Build from the current checkout (implies `--from-source`) |
| `--insecure-skip-verify` | Skip release and Xray checksums (dangerous) |
| `--uninstall` | Remove unowned binaries, units, desktop entries and icon (data is kept) |

The installer fails closed on a missing or mismatched checksum. Every release
archive also carries a GitHub build-provenance attestation:

```bash
gh attestation verify whoisthat-linux-amd64.tar.gz --repo kvunoff/whoisthat
```

### What gets installed

The release and installer provide:

| Path | Purpose |
| :--- | :--- |
| `/usr/bin/whoisthat` | Rust TUI and CLI |
| `/usr/bin/whoisthat-core` | Go daemon with network capabilities |
| `/usr/bin/whoisthat-runner` | per-user split-tunnel helper for non-systemd hosts |
| `/usr/lib/whoisthat/cgroup-setup` | creates the per-user cgroup and runtime directory |
| `/usr/lib/whoisthat/run-core` | non-systemd core wrapper |
| `/usr/lib/whoisthat/install-init` | installs/removes init services and templates |
| `/usr/lib/whoisthat/init-hook` | boot hook for cgroup markers |
| `/usr/lib/whoisthat/run-runner` | starts the per-user runner at boot |
| `/usr/lib/whoisthat/init/<init>/` | OpenRC, dinit and runit templates (installer/AUR) |
| `/usr/share/applications/whoisthat.desktop` | application menu launcher |
| `/usr/share/applications/whoisthat-url.desktop` | hidden `wit://` handler |
| `/usr/share/icons/hicolor/scalable/apps/whoisthat.svg` | icon |
| `/usr/share/bash-completion/completions/whoisthat` | bash completion |
| `/usr/share/zsh/site-functions/_whoisthat` | zsh completion |
| `/usr/share/fish/vendor_completions.d/whoisthat.fish` | fish completion |
| `/usr/share/man/man1/whoisthat.1` | man page (packages may compress it) |

Distribution packages install the binaries and helpers. Init templates are
provided by the installer and AUR packages; use them when a non-systemd host
needs `install-init`.

The installer refreshes desktop and icon caches and runs `whoisthat link
register` as the invoking user, which binds `wit://` with `xdg-mime`. It does not
edit shell rc files.

## Distribution packages

Every release publishes `.deb` and `.rpm` packages for `amd64`/`x86_64` and
`arm64`/`aarch64`.

### Ubuntu PPA

Ubuntu 24.04 LTS (Noble) users can install and upgrade through the PPA:

```bash
sudo add-apt-repository ppa:kvunoff/whoisthat
sudo apt update
sudo apt install whoisthat
```

The PPA ships the same static binaries as the release archives and sets file
capabilities in its post-install script, so no manual `setcap` is needed. It
currently publishes Noble `amd64` builds; enable other series or `arm64` in the
PPA settings, or use the release `.deb` or the installer.

```bash
apt-cache policy whoisthat
whoisthat --version
whoisthat-core --version
```

### Direct `.deb` / `.rpm`

```bash
# Debian / Ubuntu
sudo apt install ./whoisthat_<version>-1_amd64.deb

# Fedora / RHEL
sudo dnf install ./whoisthat-<version>-1.x86_64.rpm
```

The packages re-apply file capabilities and desktop caches in their post-install
script. Configuration and the encrypted database are never touched by package
upgrades or removal.

## Arch Linux (AUR)

Two flavours are available:

- [`whoisthat-bin`](https://aur.archlinux.org/packages/whoisthat-bin) — prebuilt
  release binaries (recommended);
- [`whoisthat`](https://aur.archlinux.org/packages/whoisthat) — builds from
  source.

```bash
paru -S whoisthat-bin
# or
yay -S whoisthat-bin
```

They conflict with each other and with `install.sh`. If WhoisThat was installed
with the script, remove it first:

```bash
curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | sudo bash -s -- --uninstall
```

## Manual build from source

### Prerequisites

- **Rust 1.88** or newer with `edition 2024` support
  ([rustup.rs](https://rustup.rs/)).
- **Go 1.26.11** or newer ([go.dev](https://go.dev/)).
- **Git**, **curl**, and a C compiler (`gcc` or `clang`).
- **iproute2** and either **nftables** or **iptables**.

Runtime requirements for the full feature set:

| Requirement | Needed for |
| :--- | :--- |
| Linux with `/dev/net/tun` | TUN mode |
| `nftables` (recommended) | Split tunneling, kill-switch, DNS hijack |
| `iptables` | Legacy fallback for kill-switch and DNS hijack; split still needs nftables |
| cgroup v2 (`/sys/fs/cgroup/cgroup.controllers`) | Split tunneling (`whoisthat run`) |
| systemd user manager or OpenRC/dinit/runit | Autostart and split-tunnel launching |
| `libcap` (`setcap`/`getcap`) | Rootless TUN via file capabilities |
| `x86_64` or `aarch64` | Prebuilt Xray runtime |

Proxy mode needs only the binaries and a reachable server. TUN, split tunneling
and the kill-switch need the kernel and firewall pieces above. See
[Platforms & Service Management](/guide/platforms) for the init-specific setup.

### Build and install

```bash
make build
sudo make install
```

`make install` installs the client, core and runner, and applies `setcap`. If you
build by hand, remember that a new `go build` output has a new inode and **loses
its capabilities**:

```bash
sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /usr/bin/whoisthat-core
whoisthat doctor
```

The same loop with explicit commands:

```bash
git clone https://github.com/kvunoff/whoisthat.git
cd whoisthat
( cd core/core && go build -o whoisthat-core )
cargo build --release --locked
sudo install -Dm755 target/release/whoisthat /usr/bin/whoisthat
sudo install -Dm755 core/core/whoisthat-core /usr/bin/whoisthat-core
sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /usr/bin/whoisthat-core
```

For a development build without installing, see [Build & Run](/developers/build).

## Network capabilities

TUN mode creates virtual interfaces and routing rules without root. It uses file
capabilities:

```bash
sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /usr/bin/whoisthat-core
```

If the core lacks capabilities and TUN autostart is configured, the TUI offers a
one-time `pkexec` prompt before it starts. In every other case it reports the
missing capabilities and points to the `setcap` command. The capability model is
explained in [TUN Mode & Capabilities](/networking/tun-mode).

## Verify

```bash
whoisthat doctor
```

`doctor` checks binaries, capabilities, polkit, IPC, DNS/routing, storage and
logs. It prints a summary and, with `--json`, a machine-readable report. See
[Doctor Diagnostics](/reference/doctor) for categories and remediation.

## Xray runtime

The pinned Xray-core runtime is downloaded and checksum-verified automatically on
first use. A matching system binary may be adopted. Details and the exact
directory layout are in [Configuration](/reference/configuration#xray-runtime).

## See also

- [Quick Start](/guide/quick-start)
- [Platforms & Service Management](/guide/platforms)
- [Maintenance, Upgrading & Recovery](/guide/maintenance)
- [Doctor Diagnostics](/reference/doctor)
