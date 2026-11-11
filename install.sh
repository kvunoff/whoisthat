#!/usr/bin/env bash
# =============================================================================
# WhoisThat — Universal Installer & Updater
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash -s -- --yes
#
# Works for both fresh installs and upgrades. By default it downloads the
# prebuilt static binaries from the latest GitHub release; --from-source keeps
# the old behaviour of cloning a tag and building Go + Rust locally.
#
# Components:
#   1. Runtime prerequisites (curl, unzip, libcap, iproute2, nftables)
#   2. WhoisThat suite (prebuilt release assets):
#        - whoisthat-core   (Go VPN daemon with ambient Linux capabilities)
#        - whoisthat        (Ratatui Rust TUI client)
#   3. Isolated Xray-core runtime (v26.10.31: ~/.local/share/whoisthat/runtimes/xray/)
#
# --from-source additionally installs Go 1.26.10+ and Rust stable, then builds.
# =============================================================================
# Ensure running under bash
if [ -z "${BASH_VERSION:-}" ]; then
    if [ -f "$0" ] && command -v bash >/dev/null 2>&1; then
        exec bash "$0" "$@"
    else
        echo "[-] Error: This installer requires bash. Please run with bash (e.g. 'bash install.sh' or 'curl ... | bash')." >&2
        exit 1
    fi
fi

set -euo pipefail

# --- version constants -------------------------------------------------------
GO_MIN_VERSION="1.26.11"
GO_INSTALL_VERSION="1.26.11"
XRAY_VERSION="v26.10.31"

# Canonical install location. /usr/local/bin is kept only as a legacy cleanup
# target so upgrades stop being shadowed by an old copy.
BIN_DIR="/usr/bin"
LEGACY_BIN_DIR="/usr/local/bin"

# --- configuration & defaults ------------------------------------------------
WORK_DIR=""
BUILD_DIR=""
ARTIFACT_DIR=""
GIT_REPO="https://github.com/kvunoff/whoisthat.git"
GITHUB_REPO="https://github.com/kvunoff/whoisthat"
ASSUME_YES=false
TARGET_BRANCH=""
BUILD_LOCAL=false
INSTALL_FROM_SOURCE=false
RELEASE_TAG=""
UNINSTALL_MODE=false
INSECURE_SKIP_VERIFY=false
XRAY_DEGRADED=false
MODE="Install"
ARCH_FAMILY=""
GO_ARCH=""
XRAY_ARCH=""
DISTRO_ID=""
DISTRO_NAME=""
DISTRO_LIKE=""

# Target regular user and home directory for isolated runtimes and configs
if [ -n "${SUDO_USER:-}" ] && [ "$SUDO_USER" != "root" ]; then
    TARGET_USER="$SUDO_USER"
    TARGET_HOME="$(getent passwd "$SUDO_USER" 2>/dev/null | cut -d: -f6)"
    [ -z "$TARGET_HOME" ] && TARGET_HOME="$HOME"
else
    TARGET_USER="${USER:-$(id -un)}"
    TARGET_HOME="$HOME"
fi

RUNTIMES_DIR="${TARGET_HOME}/.local/share/whoisthat/runtimes"
CONFIG_GEO_DIR="${TARGET_HOME}/.config/whoisthat/geo"

# --- terminal styling --------------------------------------------------------
if [ -t 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    CYAN='\033[0;36m'
    BOLD='\033[1m'
    NC='\033[0m'
else
    RED=''
    GREEN=''
    YELLOW=''
    CYAN=''
    BOLD=''
    NC=''
fi

info()  { echo -e "${GREEN}[+]${NC} $*"; }
warn()  { echo -e "${YELLOW}[!]${NC} $*"; }
err()   { echo -e "${RED}[-]${NC} $*"; }
step()  { echo -e "\n${CYAN}==>${NC} ${BOLD}$*${NC}"; }

# --- platform & privilege setup ----------------------------------------------
cleanup() {
    local exit_code=$?
    if [ -n "${WORK_DIR:-}" ] && [ -d "$WORK_DIR" ]; then
        rm -rf "$WORK_DIR"
    fi
    if [ $exit_code -ne 0 ]; then
        err "Installation failed (exit code $exit_code)."
    fi
}
trap cleanup EXIT

SUDO=""
require_platform_and_privileges() {
    [[ "$(uname)" == "Linux" ]] || { err "Only Linux is supported."; exit 1; }
    if [ "$(id -u)" -eq 0 ]; then
        SUDO=""
    elif command -v sudo &>/dev/null; then
        SUDO="sudo"
    else
        err "This installer requires superuser privileges to install binaries to $BIN_DIR."
        err "Please install 'sudo' or run as root."
        exit 1
    fi
}

ensure_sudo() {
    if [ -n "$SUDO" ]; then
        if ! $SUDO -v 2>/dev/null; then
            info "Requesting sudo privileges for installation..."
            $SUDO -v || { err "Superuser authentication failed."; exit 1; }
        fi
    fi
}

as_target() {
    # Run a command as TARGET_USER with that user's HOME/PATH. Used for anything
    # that touches user-writable files, so root never executes them.
    if [ "$(id -u)" -ne 0 ] || [ "$TARGET_USER" = "$(id -un)" ] || [ -z "${TARGET_HOME:-}" ]; then
        "$@"
        return
    fi
    local uid
    uid=$(id -u "$TARGET_USER")
    local -a e=(HOME="$TARGET_HOME"
                PATH="$TARGET_HOME/.cargo/bin:/usr/local/go/bin:/usr/local/bin:/usr/bin:/bin")
    if [ -d "/run/user/$uid" ]; then
        e+=(XDG_RUNTIME_DIR="/run/user/$uid")
        if [ -S "/run/user/$uid/bus" ]; then
            e+=(DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus")
        fi
    fi
    if command -v runuser >/dev/null 2>&1; then
        runuser -u "$TARGET_USER" -- env "${e[@]}" "$@"
    elif [ -n "$SUDO" ]; then
        $SUDO -u "$TARGET_USER" env "${e[@]}" "$@"
    else
        su -s /bin/sh "$TARGET_USER" -c 'exec "$@"' sh env "${e[@]}" "$@"
    fi
}

# --- helper functions --------------------------------------------------------
show_help() {
    cat <<EOF
WhoisThat Universal Installer & Updater

Usage:
  install.sh [options]
  curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash -s -- [options]

Options:
  -y, --yes          Accepted for compatibility (the installer asks no questions)
  --release <tag>    Install a specific release (default: the latest published one)
  --from-source      Build from source instead of downloading release binaries
  --branch <name>    Build from a specific git branch or tag (implies --from-source)
  --local            Build directly from current repository directory (implies --from-source)
  --insecure-skip-verify  Skip release and Xray checksum verification (DANGEROUS, not recommended)
  --uninstall        Remove whoisthat binaries from $BIN_DIR (and legacy $LEGACY_BIN_DIR)
  -h, --help         Show this help message and exit

Environment Variables:
  ASSUME_YES=1       Equivalent to --yes
  WHOISTHAT_BRANCH   Equivalent to --branch

EOF
}

version_ge() {
    # Returns 0 if version $1 >= version $2
    local v1="$1" v2="$2"
    [[ "$v1" =~ ^[0-9]+\.[0-9]+$ ]] && v1="${v1}.0"
    [[ "$v2" =~ ^[0-9]+\.[0-9]+$ ]] && v2="${v2}.0"
    [ "$(printf '%s\n%s\n' "$v2" "$v1" | sort -V | head -n1)" = "$v2" ]
}

# Pick the highest stable vX.Y.Z tag from `git ls-remote --tags` output.
# Pre-releases (v1.2.3-rc1) and core-v* markers are ignored.
latest_release_tag() {
    awk '{print $2}' \
        | sed 's|refs/tags/||' \
        | grep -v '\^{}' \
        | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' \
        | sort -V \
        | tail -n1
}

release_asset_url() {
    printf '%s/releases/download/%s' "$GITHUB_REPO" "$1"
}

release_tag_from_url() {
    printf '%s\n' "$1" | sed 's|.*/||'
}

# Resolve --release or the latest published release. Draft releases are not
# returned by /releases/latest, so a release is visible only after CI uploaded
# every asset and published it.
resolve_release_tag() {
    local tag="${RELEASE_TAG:-}" url
    if [ -z "$tag" ]; then
        url=$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
            "${GITHUB_REPO}/releases/latest" 2>/dev/null || true)
        tag=$(release_tag_from_url "$url")
    fi
    if ! printf '%s' "$tag" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
        err "Could not resolve a release tag (got '${tag:-<empty>}')."
        err "Pass --release vX.Y.Z or use --from-source."
        return 1
    fi
    printf '%s\n' "$tag"
}

verify_release_asset() {
    local file="$1" name="$2" sums="$3" expected actual
    if [ "$INSECURE_SKIP_VERIFY" = "true" ]; then
        warn "Skipping SHA-256 verification for ${name} (--insecure-skip-verify)."
        return 0
    fi
    if ! command -v sha256sum >/dev/null 2>&1; then
        err "sha256sum is required to verify release assets but was not found."
        return 1
    fi
    expected=$(awk -v n="$name" '$2 == n { print $1; exit }' "$sums")
    if ! printf '%s' "$expected" | grep -Eq '^[0-9a-fA-F]{64}$'; then
        err "No SHA-256 checksum for ${name} in SHA256SUMS."
        err "Refusing to install an unverified binary."
        return 1
    fi
    actual=$(sha256sum "$file" | awk '{print $1}')
    if [ "${actual,,}" != "${expected,,}" ]; then
        err "Checksum mismatch for ${name}: expected ${expected}, got ${actual}."
        err "The download may be corrupted or tampered with."
        return 1
    fi
    info "${name} SHA-256 checksum verified OK"
}

extract_tarball() {
    local archive="$1" target_dir="$2"
    mkdir -p "$target_dir"
    tar -xzf "$archive" -C "$target_dir"
}

download_release_artifacts() {
    step "Download WhoisThat release"

    local tag base bundle dl
    tag=$(resolve_release_tag) || exit 1
    base=$(release_asset_url "$tag")
    bundle="whoisthat-linux-${ARCH_FAMILY}.tar.gz"
    dl="$WORK_DIR/release"
    mkdir -p "$dl" "$ARTIFACT_DIR"

    info "Release: ${tag}"
    if ! download_file_with_fallback "$dl/$bundle" "$base/$bundle"; then
        err "Release ${tag} does not contain ${bundle}."
        err "It may still be publishing. Retry in a few minutes, pin an older"
        err "release with --release, or build from source with --from-source."
        exit 1
    fi
    if ! download_file_with_fallback "$dl/SHA256SUMS" "$base/SHA256SUMS"; then
        err "Release ${tag} has no SHA256SUMS; refusing to install unverified binaries."
        exit 1
    fi

    verify_release_asset "$dl/$bundle" "$bundle" "$dl/SHA256SUMS" || exit 1
    extract_tarball "$dl/$bundle" "$ARTIFACT_DIR"

    local f
    # The URL handler and icon were added after 0.17.7; older archives install
    # without them and the TUI writes the current entries on first launch.
    for f in whoisthat whoisthat-core whoisthat.desktop; do
        [ -f "$ARTIFACT_DIR/$f" ] || { err "Release archive is missing ${f}."; exit 1; }
    done
    if [ ! -f "$ARTIFACT_DIR/whoisthat-url.desktop" ] || [ ! -f "$ARTIFACT_DIR/whoisthat.svg" ]; then
        warn "Release archive has no separate URL handler/icon (expected for old releases)."
        warn "Per-user desktop entries will be refreshed after installation."
    fi
    chmod +x "$ARTIFACT_DIR/whoisthat" "$ARTIFACT_DIR/whoisthat-core"
    info "Downloaded and verified ${tag}."
}

extract_zip() {
    local zip_file="$1"
    local target_dir="$2"
    mkdir -p "$target_dir"
    if command -v unzip &>/dev/null; then
        unzip -q -o "$zip_file" -d "$target_dir"
    elif command -v python3 &>/dev/null; then
        python3 -m zipfile -e "$zip_file" "$target_dir"
    else
        err "Neither 'unzip' nor 'python3' is available to extract $zip_file"
        return 1
    fi
}

ensure_user_dir() {
    local dir="$1"
    mkdir -p "$dir"
    if [ "$(id -u)" -eq 0 ] && [ "$TARGET_USER" != "root" ]; then
        chown "${TARGET_USER}:" "$dir" 2>/dev/null || true
    fi
}

download_file_with_fallback() {
    local dest="$1"
    shift
    for url in "$@"; do
        info "Downloading ${url}..."
        if curl -fsSL --connect-timeout 15 "$url" -o "$dest"; then
            return 0
        fi
        warn "Failed to download from ${url}, trying fallback mirror..."
    done
    return 1
}

# True when a distro package owns the path, so the installer does not clobber
# (or later delete) a file that pacman/dpkg/rpm will expect to exist.
package_owns() {
    local path="$1"
    if command -v pacman >/dev/null 2>&1 && pacman -Qo "$path" >/dev/null 2>&1; then
        return 0
    fi
    if command -v dpkg >/dev/null 2>&1 && dpkg -S "$path" >/dev/null 2>&1; then
        return 0
    fi
    if command -v rpm >/dev/null 2>&1 && rpm -qf "$path" >/dev/null 2>&1; then
        return 0
    fi
    return 1
}

# Fail before downloading or building: a package-managed install must be
# upgraded or removed through the package manager, never overwritten here.
assert_not_package_installed() {
    local dir bin
    for dir in "$BIN_DIR" "$LEGACY_BIN_DIR"; do
        for bin in whoisthat whoisthat-core; do
            if package_owns "$dir/$bin"; then
                err "WhoisThat is installed by a package manager."
                err "Upgrade or remove the package first, then run this installer."
                exit 1
            fi
        done
    done
}

# --- argument parsing --------------------------------------------------------
parse_args() {
    [ "${ASSUME_YES:-0}" = "1" ] && ASSUME_YES=true
    if [ -n "${WHOISTHAT_BRANCH:-}" ]; then
        TARGET_BRANCH="$WHOISTHAT_BRANCH"
        INSTALL_FROM_SOURCE=true
    fi

    while [ $# -gt 0 ]; do
        case "$1" in
            -y|--yes)
                ASSUME_YES=true
                shift
                ;;
            --no-tun)
                # Deprecated: TUN mode is now natively supported by Xray-core
                shift
                ;;
            --no-hy2)
                # Deprecated: Hysteria2 is now natively supported by Xray-core
                shift
                ;;
            --release)
                if [ -n "${2:-}" ]; then
                    RELEASE_TAG="$2"
                    shift 2
                else
                    err "--release requires an argument"
                    exit 1
                fi
                ;;
            --from-source)
                INSTALL_FROM_SOURCE=true
                shift
                ;;
            --branch)
                if [ -n "${2:-}" ]; then
                    TARGET_BRANCH="$2"
                    INSTALL_FROM_SOURCE=true
                    shift 2
                else
                    err "--branch requires an argument"
                    exit 1
                fi
                ;;
            --local)
                BUILD_LOCAL=true
                INSTALL_FROM_SOURCE=true
                shift
                ;;
            --insecure-skip-verify)
                INSECURE_SKIP_VERIFY=true
                warn "Checksum verification disabled by --insecure-skip-verify. This is dangerous."
                shift
                ;;
            --uninstall)
                UNINSTALL_MODE=true
                shift
                ;;
            -h|--help)
                show_help
                exit 0
                ;;
            *)
                err "Unknown option: $1"
                exit 2
                ;;
        esac
    done

    if [ "$INSTALL_FROM_SOURCE" = "true" ] && [ -n "$RELEASE_TAG" ]; then
        err "--release cannot be combined with --from-source, --branch or --local"
        exit 1
    fi
}

# --- uninstallation ----------------------------------------------------------
uninstall_whoisthat() {
    step "Uninstalling WhoisThat"
    ensure_sudo

    # Stop the daemon and the user unit before deleting the binaries they run.
    # systemctl --user must go through as_target: under sudo it would talk to
    # root's session bus instead of the user's.
    if [ -x "$BIN_DIR/whoisthat" ]; then
        as_target "$BIN_DIR/whoisthat" autostart disable >/dev/null 2>&1 || true
    fi
    if command -v systemctl >/dev/null 2>&1; then
        as_target systemctl --user disable --now whoisthat-core.service >/dev/null 2>&1 || true
    fi
    if [ -x "$BIN_DIR/whoisthat" ]; then
        as_target "$BIN_DIR/whoisthat" core stop >/dev/null 2>&1 || true
    elif [ -x "$LEGACY_BIN_DIR/whoisthat" ]; then
        as_target "$LEGACY_BIN_DIR/whoisthat" core stop >/dev/null 2>&1 || true
    fi
    if command -v pkill >/dev/null 2>&1; then
        pkill -TERM -u "$TARGET_USER" -x whoisthat-core >/dev/null 2>&1 || true
    fi

    if [ -x /usr/lib/whoisthat/install-init ]; then
        $SUDO /usr/lib/whoisthat/install-init remove >/dev/null 2>&1 \
            || warn "could not remove the init services (install-init remove)"
    fi

    # A graceful stop deliberately leaves the kill-switch armed. Removing the
    # package must not leave the machine offline, so clear the rules here.
    if command -v nft >/dev/null 2>&1; then
        $SUDO nft delete table inet whoisthat_ks >/dev/null 2>&1 || true
    fi
    for ipt in iptables ip6tables; do
        command -v "$ipt" >/dev/null 2>&1 || continue
        while $SUDO "$ipt" -C OUTPUT -j WHOISTHAT_KS >/dev/null 2>&1; do
            $SUDO "$ipt" -D OUTPUT -j WHOISTHAT_KS >/dev/null 2>&1 || break
        done
        $SUDO "$ipt" -F WHOISTHAT_KS >/dev/null 2>&1 || true
        $SUDO "$ipt" -X WHOISTHAT_KS >/dev/null 2>&1 || true
    done

    local unit="$TARGET_HOME/.config/systemd/user/whoisthat-core.service"
    if [ -f "$unit" ]; then
        rm -f "$unit"
        as_target systemctl --user daemon-reload >/dev/null 2>&1 || true
        info "Removed $unit"
    fi
    local wants="$TARGET_HOME/.config/systemd/user/default.target.wants/whoisthat-core.service"
    if [ -L "$wants" ]; then
        rm -f "$wants"
        info "Removed $wants"
    fi

    local entry user_desktop system_desktop
    for entry in whoisthat.desktop whoisthat-url.desktop; do
        user_desktop="$TARGET_HOME/.local/share/applications/$entry"
        if [ -f "$user_desktop" ]; then
            rm -f "$user_desktop"
            info "Removed $user_desktop"
        fi
        system_desktop="/usr/share/applications/$entry"
        if [ -f "$system_desktop" ] && ! package_owns "$system_desktop"; then
            $SUDO rm -f "$system_desktop"
            info "Removed $system_desktop"
        elif [ -f "$system_desktop" ]; then
            info "Leaving package-owned $system_desktop in place"
        fi
    done

    local user_icon="$TARGET_HOME/.local/share/icons/hicolor/scalable/apps/whoisthat.svg"
    if [ -f "$user_icon" ]; then
        rm -f "$user_icon"
        info "Removed $user_icon"
    fi
    local system_icon="/usr/share/icons/hicolor/scalable/apps/whoisthat.svg"
    if [ -f "$system_icon" ] && ! package_owns "$system_icon"; then
        $SUDO rm -f "$system_icon"
        info "Removed $system_icon"
    elif [ -f "$system_icon" ]; then
        info "Leaving package-owned $system_icon in place"
    fi

    local generated
    for generated in \
        /usr/share/bash-completion/completions/whoisthat \
        /usr/share/zsh/site-functions/_whoisthat \
        /usr/share/fish/vendor_completions.d/whoisthat.fish \
        /usr/share/man/man1/whoisthat.1; do
        if [ -f "$generated" ] && ! package_owns "$generated"; then
            $SUDO rm -f "$generated"
            info "Removed $generated"
        elif [ -f "$generated" ]; then
            info "Leaving package-owned $generated in place"
        fi
    done

    # User-owned caches must be refreshed as the target user, never as root.
    if command -v update-desktop-database >/dev/null 2>&1; then
        as_target update-desktop-database "$TARGET_HOME/.local/share/applications" >/dev/null 2>&1 || true
        $SUDO update-desktop-database /usr/share/applications >/dev/null 2>&1 || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        as_target gtk-update-icon-cache -qtf "$TARGET_HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
        $SUDO gtk-update-icon-cache -qtf /usr/share/icons/hicolor >/dev/null 2>&1 || true
    fi

    local removed=0 dir bin
    for dir in "$BIN_DIR" "$LEGACY_BIN_DIR"; do
        for bin in whoisthat whoisthat-core whoisthat-runner whoisthat-parser; do
            if [ -f "$dir/$bin" ]; then
                if package_owns "$dir/$bin"; then
                    info "Leaving package-owned $dir/$bin in place"
                    continue
                fi
                $SUDO rm -f "$dir/$bin"
                info "Removed $dir/$bin"
                removed=1
            fi
        done
    done

    if [ "$removed" -eq 0 ]; then
        info "No WhoisThat binaries were found in $BIN_DIR or $LEGACY_BIN_DIR."
    else
        info "WhoisThat binaries removed successfully."
    fi

    if [ -d /usr/lib/whoisthat ] && ! package_owns /usr/lib/whoisthat/cgroup-setup; then
        $SUDO rm -rf /usr/lib/whoisthat
        info "Removed /usr/lib/whoisthat"
    fi

    echo
    warn "User configuration (~/.config/whoisthat) and database/runtimes (~/.local/share/whoisthat) were preserved."
    warn "To completely delete all user data, credentials, and isolated runtimes, run:"
    echo -e "    ${YELLOW}rm -rf ~/.config/whoisthat ~/.local/share/whoisthat${NC}"
    echo
    exit 0
}

# --- hardware and system detection -------------------------------------------
detect_arch() {
    local m
    m="$(uname -m)"
    case "$m" in
        x86_64|amd64)
            ARCH_FAMILY="amd64"
            GO_ARCH="amd64"
            XRAY_ARCH="64"
            ;;
        aarch64|arm64)
            ARCH_FAMILY="arm64"
            GO_ARCH="arm64"
            XRAY_ARCH="arm64-v8a"
            ;;
        *)
            err "Unsupported architecture: $m"
            err "Precompiled engine packages support x86_64 and aarch64."
            exit 1
            ;;
    esac
    info "Platform: Linux ($m -> $ARCH_FAMILY)"
}

detect_distro() {
    if [ -f /etc/os-release ]; then
        # shellcheck disable=SC1091
        . /etc/os-release
        DISTRO_ID="${ID:-unknown}"
        DISTRO_NAME="${PRETTY_NAME:-$DISTRO_ID}"
        DISTRO_LIKE="${ID_LIKE:-}"
    else
        DISTRO_ID="unknown"
        DISTRO_NAME="Unknown Linux"
        DISTRO_LIKE=""
    fi
    info "Distribution: ${DISTRO_NAME}"
}

# --- step 1: system build tools ----------------------------------------------
install_system_deps() {
    step "System build prerequisites"

    case "$DISTRO_ID" in
        debian|ubuntu|linuxmint|pop)
            info "Debian/Ubuntu family — installing build-essential, git, curl, unzip, libcap2-bin, iproute2, nftables"
            $SUDO apt-get update -qq || warn "apt-get update failed; continuing with the existing package lists"
            $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
                build-essential git curl unzip libcap2-bin iproute2 nftables
            return
            ;;
        fedora|rhel|centos|rocky|almalinux)
            info "Fedora/RHEL family — installing gcc, git, curl, make, unzip, libcap, iproute, nftables"
            $SUDO dnf install -y -q gcc git curl make unzip libcap iproute nftables
            return
            ;;
        arch|manjaro|endeavouros)
            info "Arch family — installing base-devel, git, curl, unzip, libcap, iproute2, nftables"
            $SUDO pacman -S --noconfirm --needed base-devel git curl unzip libcap iproute2 nftables
            return
            ;;
        alpine)
            info "Alpine — installing build-base, git, curl, unzip, libcap, iproute2, nftables"
            $SUDO apk add --no-cache build-base git curl unzip libcap iproute2 nftables
            return
            ;;
        opensuse*|suse)
            info "openSUSE — installing gcc, git, curl, make, unzip, libcap-progs, iproute2, nftables"
            $SUDO zypper install -y -l gcc git curl make unzip libcap-progs iproute2 nftables
            return
            ;;
    esac

    # Fallback for derivative distributions via ID_LIKE
    case "${DISTRO_LIKE}" in
        *debian*|*ubuntu*)
            info "Debian/Ubuntu derivative (${DISTRO_ID}) — installing build-essential, git, curl, unzip, libcap2-bin, iproute2, nftables"
            $SUDO apt-get update -qq || warn "apt-get update failed; continuing with the existing package lists"
            $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
                build-essential git curl unzip libcap2-bin iproute2 nftables
            ;;
        *fedora*|*rhel*)
            info "Fedora/RHEL derivative (${DISTRO_ID}) — installing gcc, git, curl, make, unzip, libcap, iproute, nftables"
            $SUDO dnf install -y -q gcc git curl make unzip libcap iproute nftables
            ;;
        *arch*)
            info "Arch derivative (${DISTRO_ID}) — installing base-devel, git, curl, unzip, libcap, iproute2, nftables"
            $SUDO pacman -S --noconfirm --needed base-devel git curl unzip libcap iproute2 nftables
            ;;
        *)
            warn "Unrecognized distribution (${DISTRO_ID})."
            warn "Ensure you have: C compiler, make, git, curl, unzip, libcap (setcap),"
            warn "iproute2 (ip, ss) and nftables or iptables."
            ;;
    esac
}

# --- release prerequisites ---------------------------------------------------
# Prebuilt binaries are static, so only runtime tools are installed here.
install_runtime_deps() {
    step "Runtime prerequisites"

    local packages
    case "$DISTRO_ID" in
        debian|ubuntu|linuxmint|pop)
            info "Debian/Ubuntu family — installing curl, unzip, libcap2-bin, iproute2, nftables"
            $SUDO apt-get update -qq || warn "apt-get update failed; continuing with the existing package lists"
            $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
                curl unzip libcap2-bin iproute2 nftables ca-certificates
            return
            ;;
        fedora|rhel|centos|rocky|almalinux)
            info "Fedora/RHEL family — installing curl, unzip, libcap, iproute, nftables"
            $SUDO dnf install -y -q curl unzip libcap iproute nftables ca-certificates
            return
            ;;
        arch|manjaro|endeavouros)
            info "Arch family — installing curl, unzip, libcap, iproute2, nftables"
            $SUDO pacman -S --noconfirm --needed curl unzip libcap iproute2 nftables ca-certificates
            return
            ;;
        alpine)
            info "Alpine — installing curl, unzip, libcap, iproute2, nftables"
            $SUDO apk add --no-cache curl unzip libcap iproute2 nftables ca-certificates
            return
            ;;
        opensuse*|suse)
            info "openSUSE — installing curl, unzip, libcap-progs, iproute2, nftables"
            $SUDO zypper install -y -l curl unzip libcap-progs iproute2 nftables ca-certificates
            return
            ;;
    esac

    case "${DISTRO_LIKE}" in
        *debian*|*ubuntu*)
            info "Debian/Ubuntu derivative (${DISTRO_ID}) — installing runtime prerequisites"
            $SUDO apt-get update -qq || warn "apt-get update failed; continuing with the existing package lists"
            $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
                curl unzip libcap2-bin iproute2 nftables ca-certificates
            ;;
        *fedora*|*rhel*)
            info "Fedora/RHEL derivative (${DISTRO_ID}) — installing runtime prerequisites"
            $SUDO dnf install -y -q curl unzip libcap iproute nftables ca-certificates
            ;;
        *arch*)
            info "Arch derivative (${DISTRO_ID}) — installing runtime prerequisites"
            $SUDO pacman -S --noconfirm --needed curl unzip libcap iproute2 nftables ca-certificates
            ;;
        *)
            packages="curl unzip libcap (setcap) iproute2 nftables"
            warn "Unrecognized distribution (${DISTRO_ID})."
            warn "Ensure you have: ${packages} and ca-certificates."
            ;;
    esac
}

# --- step 2: Go toolchain ----------------------------------------------------
install_go() {
    step "Verify Go toolchain (>= ${GO_MIN_VERSION})"

    # Check existing environment PATH + standard /usr/local/go/bin
    if [ -d "/usr/local/go/bin" ] && [[ ":$PATH:" != *":/usr/local/go/bin:"* ]]; then
        export PATH="/usr/local/go/bin:$PATH"
    fi

    if command -v go &>/dev/null; then
        local current_go
        current_go=$(go version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1 || true)
        if [ -n "$current_go" ] && version_ge "$current_go" "$GO_MIN_VERSION"; then
            info "Go ${current_go} detected (>= ${GO_MIN_VERSION}), skipping installation"
            return
        fi
        warn "Found Go ${current_go:-unknown}, but >= ${GO_MIN_VERSION} is required. Upgrading..."
    else
        info "Go not found in PATH. Installing Go ${GO_INSTALL_VERSION}..."
    fi

    if ! command -v sha256sum &>/dev/null; then
        err "sha256sum is required to verify the Go archive but was not found."
        exit 1
    fi

    local go_tarball="go${GO_INSTALL_VERSION}.linux-${GO_ARCH}.tar.gz"
    local dl_dir="$WORK_DIR/go"
    local want actual
    mkdir -p "$dl_dir"

    info "Downloading ${go_tarball}..."
    curl -fsSL --proto '=https' --tlsv1.2 --retry 3 \
        "https://dl.google.com/go/${go_tarball}" -o "$dl_dir/$go_tarball"
    want=$(curl -fsSL --proto '=https' --tlsv1.2 --retry 3 \
        "https://dl.google.com/go/${go_tarball}.sha256" | tr -d '[:space:]')
    actual=$(sha256sum "$dl_dir/$go_tarball" | awk '{print $1}')
    if ! printf '%s' "$want" | grep -Eq '^[0-9a-f]{64}$' || [ "$want" != "$actual" ]; then
        err "Go checksum mismatch (expected '${want}', got '${actual}')."
        err "Refusing to replace /usr/local/go with an unverified toolchain."
        exit 1
    fi

    info "Extracting to /usr/local/go..."
    # Recover from an interrupted previous run before touching the toolchain.
    if [ ! -e /usr/local/go ] && [ -d /usr/local/go.prev ]; then
        $SUDO mv /usr/local/go.prev /usr/local/go
    fi
    $SUDO rm -rf /usr/local/.go-tmp
    $SUDO mkdir -p /usr/local/.go-tmp
    if ! $SUDO tar -C /usr/local/.go-tmp --strip-components=1 -xzf "$dl_dir/$go_tarball"; then
        $SUDO rm -rf /usr/local/.go-tmp
        err "Failed to extract the Go archive; existing toolchain left untouched."
        exit 1
    fi
    $SUDO rm -rf /usr/local/go.prev
    if [ -e /usr/local/go ]; then
        $SUDO mv /usr/local/go /usr/local/go.prev
    fi
    if ! $SUDO mv /usr/local/.go-tmp /usr/local/go; then
        if [ -d /usr/local/go.prev ]; then
            $SUDO mv /usr/local/go.prev /usr/local/go
        fi
        err "Failed to activate the new Go toolchain."
        exit 1
    fi
    $SUDO rm -rf /usr/local/go.prev

    export PATH="/usr/local/go/bin:$PATH"

    local installed_go
    installed_go=$(go version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)
    if [ "${installed_go:-}" != "$GO_INSTALL_VERSION" ]; then
        warn "Installed Go reports ${installed_go:-unknown}, expected ${GO_INSTALL_VERSION}."
    fi

    local profile_file="$TARGET_HOME/.profile"
    if [ -f "$profile_file" ] && ! grep -q '/usr/local/go/bin' "$profile_file" 2>/dev/null; then
        echo 'export PATH="/usr/local/go/bin:$PATH"' >> "$profile_file"
        info "Added /usr/local/go/bin to ~/.profile"
    fi

    info "Go ready: $(go version)"
}

# --- step 3: Rust toolchain --------------------------------------------------
install_rust() {
    step "Verify Rust toolchain"

    local rust_ver
    if as_target sh -c 'command -v cargo >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1'; then
        rust_ver=$(as_target rustc --version 2>/dev/null | awk '{print $2}' || true)
        if ! version_ge "${rust_ver:-0}" "1.88"; then
            if as_target sh -c 'command -v rustup >/dev/null 2>&1'; then
                info "Rust ${rust_ver:-unknown} is older than 1.88, running 'rustup update stable'..."
                as_target rustup update stable
                rust_ver=$(as_target rustc --version 2>/dev/null | awk '{print $2}' || true)
            fi
            if ! version_ge "${rust_ver:-0}" "1.88"; then
                err "Rust ${rust_ver:-unknown} is too old: whoisthat needs 1.88 or newer."
                err "Update it with: rustup update stable"
                exit 1
            fi
        fi
        info "Rust ${rust_ver} detected, skipping installation"
        return
    fi

    info "Installing Rust stable via official rustup installer (https://rustup.rs)..."
    as_target sh -c 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable'

    rust_ver=$(as_target rustc --version 2>/dev/null | awk '{print $2}' || true)
    if [ -z "${rust_ver:-}" ]; then
        err "Rust installation finished, but rustc is not available to ${TARGET_USER}."
        exit 1
    fi
    info "Rust ready: ${rust_ver}"
}

# --- step 4: build WhoisThat binaries ----------------------------------------
build_whoisthat() {
    step "Build WhoisThat suite"

    local src_dir="$BUILD_DIR"

    # Option to build in-place if running directly from a local repo clone
    if [ "$BUILD_LOCAL" = "true" ]; then
        info "Building directly from current directory: $(pwd)"
        src_dir="$(pwd)"
    else
        local tag="$TARGET_BRANCH"
        if [ -z "$tag" ]; then
            info "Looking up latest release tag from GitHub..."
            tag=$(git ls-remote --tags "$GIT_REPO" 'refs/tags/v*' 2>/dev/null | latest_release_tag || true)
            if [ -z "$tag" ]; then
                warn "Could not determine latest release tag. Falling back to 'main' branch."
                tag="main"
            fi
        fi

        info "Cloning whoisthat (${tag}) into ${BUILD_DIR}..."
        as_target git clone --depth 1 --branch "${tag}" "$GIT_REPO" "$BUILD_DIR"
    fi

    cd "$src_dir"

    # The checkout is authoritative for the required Rust version: --branch may
    # point at a tag whose Cargo.toml differs from the one this script shipped with.
    local rust_min
    rust_min=$(sed -n 's/^rust-version = "\([^"]*\)"$/\1/p' "$src_dir/Cargo.toml" 2>/dev/null | head -1 || true)
    if [ -n "${rust_min:-}" ]; then
        local rust_now
        rust_now=$(as_target rustc --version 2>/dev/null | awk '{print $2}' || true)
        if ! version_ge "${rust_now:-0}" "$rust_min"; then
            err "Rust ${rust_now:-unknown} is older than ${rust_min} required by this checkout."
            err "Update it with: rustup update stable"
            exit 1
        fi
    fi

    # 1. whoisthat-core (Go)
    info "1/2 Building whoisthat-core..."
    ( cd "$src_dir/core/core" && as_target go build -o whoisthat-core )
    [ -f "$src_dir/core/core/whoisthat-core" ] || { err "whoisthat-core build failed"; exit 1; }

    # 2. whoisthat TUI (Rust) and the per-user runner (same workspace)
    info "2/2 Building whoisthat TUI and runner..."
    ( cd "$src_dir" && as_target cargo build --release --locked )
    [ -f "$src_dir/target/release/whoisthat" ] || { err "whoisthat TUI build failed"; exit 1; }
    [ -f "$src_dir/target/release/whoisthat-runner" ] || { err "whoisthat-runner build failed"; exit 1; }

    mkdir -p "$ARTIFACT_DIR"
    cp "$src_dir/target/release/whoisthat" "$ARTIFACT_DIR/whoisthat"
    cp "$src_dir/target/release/whoisthat-runner" "$ARTIFACT_DIR/whoisthat-runner"
    cp "$src_dir/core/core/whoisthat-core" "$ARTIFACT_DIR/whoisthat-core"
    cp "$src_dir/pkg/whoisthat.desktop" "$ARTIFACT_DIR/whoisthat.desktop"
    mkdir -p "$ARTIFACT_DIR/lib/whoisthat"
    for helper in cgroup-setup run-runner run-core install-init init-hook; do
        [ -f "$src_dir/scripts/$helper" ] && cp "$src_dir/scripts/$helper" "$ARTIFACT_DIR/lib/whoisthat/$helper"
    done
    [ -d "$src_dir/pkg/init" ] && cp -R "$src_dir/pkg/init" "$ARTIFACT_DIR/lib/whoisthat/init"
    # Tags released before the two-entry desktop integration lack these files.
    [ -f "$src_dir/pkg/whoisthat-url.desktop" ] &&
        cp "$src_dir/pkg/whoisthat-url.desktop" "$ARTIFACT_DIR/whoisthat-url.desktop"
    [ -f "$src_dir/docs/public/logo.svg" ] &&
        cp "$src_dir/docs/public/logo.svg" "$ARTIFACT_DIR/whoisthat.svg"
    [ -f "$src_dir/LICENSE" ] && cp "$src_dir/LICENSE" "$ARTIFACT_DIR/LICENSE"
    info "All WhoisThat components built successfully."
}

# --- step 6: install binaries & set capabilities -----------------------------
# Completion scripts and the man page are generated from the installed binary,
# so they always match its version. Packages install the same paths directly.
install_generated() {
    local dst=$1
    shift
    local tmp
    tmp=$(mktemp)
    if "$BIN_DIR/whoisthat" "$@" >"$tmp" 2>/dev/null; then
        $SUDO install -Dm644 "$tmp" "$dst"
    else
        warn "Could not generate $dst"
    fi
    rm -f "$tmp"
}

install_shell_integration() {
    [ -x "$BIN_DIR/whoisthat" ] || return 0
    install_generated /usr/share/bash-completion/completions/whoisthat completions bash
    install_generated /usr/share/zsh/site-functions/_whoisthat completions zsh
    install_generated /usr/share/fish/vendor_completions.d/whoisthat.fish completions fish
    install_generated /usr/share/man/man1/whoisthat.1 man
    if command -v mandb >/dev/null 2>&1; then
        $SUDO mandb >/dev/null 2>&1 || true
    fi
}

install_binaries() {
    step "Install binaries to $BIN_DIR"

    # A distro package owns these paths; overwriting them would break dpkg/rpm
    # verification and the next package upgrade.
    if package_owns "$BIN_DIR/whoisthat" || package_owns "$BIN_DIR/whoisthat-core" \
        || package_owns "$BIN_DIR/whoisthat-runner"; then
        err "WhoisThat is installed by a distribution package."
        err "Upgrade it with your package manager, or remove the package first."
        exit 1
    fi

    # Stage both binaries as hidden files first: the pair is swapped only after
    # the new core has capabilities, so an interrupted or failed install never
    # leaves a new TUI next to an old, incompatible core.
    info "Installing whoisthat, whoisthat-core and whoisthat-runner..."
    $SUDO install -Dm755 "$ARTIFACT_DIR/whoisthat" "$BIN_DIR/.whoisthat.new"
    $SUDO install -Dm755 "$ARTIFACT_DIR/whoisthat-core" "$BIN_DIR/.whoisthat-core.new"
    if [ -f "$ARTIFACT_DIR/whoisthat-runner" ]; then
        $SUDO install -Dm755 "$ARTIFACT_DIR/whoisthat-runner" "$BIN_DIR/.whoisthat-runner.new"
    else
        warn "Release archive has no whoisthat-runner (old release); split-tunnel launching will be unavailable."
    fi

    info "Granting network capabilities to whoisthat-core..."
    if $SUDO setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep "$BIN_DIR/.whoisthat-core.new" 2>/dev/null; then
        info "Capabilities configured: cap_net_admin, cap_net_raw, cap_setpcap"
    else
        warn "setcap failed or filesystem does not support capabilities."
        warn "Run 'whoisthat doctor' for the manual setup command."
    fi

    $SUDO mv -f "$BIN_DIR/.whoisthat.new" "$BIN_DIR/whoisthat"
    $SUDO mv -f "$BIN_DIR/.whoisthat-core.new" "$BIN_DIR/whoisthat-core"
    if [ -f "$BIN_DIR/.whoisthat-runner.new" ]; then
        $SUDO mv -f "$BIN_DIR/.whoisthat-runner.new" "$BIN_DIR/whoisthat-runner"
    fi

    # Remove binaries from the old canonical location so PATH order can never
    # pick a stale copy over the new /usr/bin install.
    for bin in whoisthat whoisthat-core whoisthat-runner whoisthat-parser; do
        if [ -f "$LEGACY_BIN_DIR/$bin" ]; then
            $SUDO rm -f "$LEGACY_BIN_DIR/$bin"
            info "Removed legacy $LEGACY_BIN_DIR/$bin"
        fi
    done

    local entry desktop
    for entry in whoisthat.desktop whoisthat-url.desktop; do
        [ -f "$ARTIFACT_DIR/$entry" ] || continue
        desktop="/usr/share/applications/$entry"
        if package_owns "$desktop"; then
            info "Leaving package-owned ${desktop} in place"
        else
            $SUDO install -Dm644 "$ARTIFACT_DIR/$entry" "$desktop"
        fi
    done

    if [ -f "$ARTIFACT_DIR/whoisthat.svg" ]; then
        local icon="/usr/share/icons/hicolor/scalable/apps/whoisthat.svg"
        if package_owns "$icon"; then
            info "Leaving package-owned ${icon} in place"
        else
            $SUDO install -Dm644 "$ARTIFACT_DIR/whoisthat.svg" "$icon"
        fi
    fi

    if command -v update-desktop-database >/dev/null 2>&1; then
        $SUDO update-desktop-database /usr/share/applications 2>/dev/null || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        $SUDO gtk-update-icon-cache -qtf /usr/share/icons/hicolor 2>/dev/null || true
    fi

    # Older versions wrote a per-user handler that shadows this launcher and
    # does nothing without a URL. Rewrite it now and register wit:// eagerly.
    ensure_user_dir "$TARGET_HOME/.config"
    if ! as_target "$BIN_DIR/whoisthat" link register >/dev/null 2>&1; then
        warn "Could not refresh per-user desktop entries; run 'whoisthat link register'."
    fi

install_shell_integration
    install_system_helpers
    enable_init_services
}

# Root-owned helpers and init templates shipped inside the release archive
# under lib/whoisthat/. Older releases do not have them; that is a warning.
install_system_helpers() {
    local src="$ARTIFACT_DIR/lib/whoisthat"
    if [ ! -d "$src" ]; then
        warn "Release archive has no split-tunnel helpers (old release); skipping."
        return 0
    fi
    local lib=/usr/lib/whoisthat
    $SUDO mkdir -p "$lib"
    $SUDO cp -a "$src/." "$lib/"
    $SUDO chown -R root:root "$lib"
    $SUDO find "$lib" -maxdepth 1 -type f -exec chmod 755 {} +
    $SUDO find "$lib/init/openrc" "$lib/init/runit" -type f -exec chmod 755 {} + 2>/dev/null || true
    $SUDO find "$lib/init/dinit" -type f -exec chmod 644 {} + 2>/dev/null || true
}

# On systemd hosts the TUI creates its unit. Elsewhere install-init sets up the
# cgroup hook, the per-user runner and the core service once, as root.
enable_init_services() {
    [ -d /run/systemd/system ] && return 0
    local user="${TARGET_USER:-}"
    [ -n "$user" ] || return 0
    local uid
    uid=$(id -u "$user" 2>/dev/null) || return 0
    [ "$uid" -ge 1000 ] || return 0
    local lib=/usr/lib/whoisthat
    [ -x "$lib/install-init" ] || return 0
    if [ -x "$lib/cgroup-setup" ]; then
        "$lib/cgroup-setup" "$user" || warn "cgroup-setup failed; split tunnelling may be unavailable"
    fi
    "$lib/install-init" cgroup enable || warn "could not install the cgroup boot hook"
    "$lib/install-init" runner enable "$user" "$TARGET_HOME" \
        || warn "could not install whoisthat-runner"
    "$lib/install-init" core enable "$user" "$TARGET_HOME" \
        || warn "could not install the whoisthat-core service"
}

# --- Xray archive verification ------------------------------------------------
# Fail closed: a missing or mismatched checksum aborts the installation. The
# digest is fetched from GitHub only (never from the download mirror), so a
# compromised mirror cannot supply both the archive and its checksum.
verify_xray_archive() {
    local archive="$1"
    local xray_zip="$2"
    local tmp_dir="$3"

    if [ "$INSECURE_SKIP_VERIFY" = "true" ]; then
        warn "Skipping Xray checksum verification (--insecure-skip-verify)."
        return 0
    fi

    if ! command -v sha256sum &>/dev/null; then
        err "sha256sum is required to verify the Xray archive but was not found."
        return 1
    fi

    local dgst_file="${tmp_dir}/${xray_zip}.dgst"
    local expected_sha=""
    if curl -fsSL --connect-timeout 10 \
        "https://github.com/XTLS/Xray-core/releases/download/${XRAY_VERSION}/${xray_zip}.dgst" \
        -o "$dgst_file" 2>/dev/null; then
        expected_sha=$(grep -i "SHA2-256" "$dgst_file" | grep -oE '[a-fA-F0-9]{64}' | head -1 | tr '[:upper:]' '[:lower:]' || true)
    fi

    if [ -z "$expected_sha" ]; then
        err "Could not obtain the official Xray SHA-256 checksum."
        err "Refusing to install an unverified binary. Re-run with --insecure-skip-verify to override (not recommended)."
        return 1
    fi

    local actual_sha
    actual_sha=$(sha256sum "${archive}" | awk '{print $1}')
    if [ "$actual_sha" != "$expected_sha" ]; then
        err "Xray checksum mismatch: expected ${expected_sha}, got ${actual_sha}."
        err "The archive may be corrupted or tampered with. Aborting installation."
        return 1
    fi

    info "Xray-core SHA-256 checksum verified OK"
    return 0
}

# Extract the version from `xray version` output exactly like the core does
# (core/core/lib/xraymgr/version.go): "Xray v1.2.3 ...". A bare substring
# match would accept "26.9.9" inside "26.9.90" or "126.9.9".
xray_version() {
    printf '%s\n' "$1" \
        | grep -ioE 'Xray[[:space:]]+v?[0-9]+(\.[0-9]+)+' \
        | head -1 \
        | grep -oE '[0-9]+(\.[0-9]+)+' || true
}

# --- step 5: install Xray-core -----------------------------------------------
install_xray() {
    step "Isolated Xray-core runtime (${XRAY_VERSION})"

    local xray_dir="${RUNTIMES_DIR}/xray/${XRAY_VERSION}"
    local xray_bin="${xray_dir}/xray"

    ensure_user_dir "$xray_dir"
    ensure_user_dir "$CONFIG_GEO_DIR"

    # 1. Check if already installed in isolated runtime directory
    if [ -x "$xray_bin" ]; then
        local current_out current_ver
        current_out="$(as_target "$xray_bin" version 2>&1 || true)"
        current_ver=$(xray_version "$current_out")
        if [ "$current_ver" = "${XRAY_VERSION#v}" ]; then
            info "Xray-core matches pinned version ${XRAY_VERSION} in isolated runtime (${xray_bin})"
            return
        else
            info "Existing isolated xray differs from pinned ${XRAY_VERSION}, updating..."
        fi
    fi

    # 2. Check if a system binary matches pinned version and can be adopted
    local sys_xray=""
    for candidate in /usr/local/bin/xray /usr/bin/xray; do
        if [ -x "$candidate" ]; then
            local cand_out
            cand_out="$(as_target "$candidate" version 2>&1 || true)"
            if [ "$(xray_version "$cand_out")" = "${XRAY_VERSION#v}" ]; then
                sys_xray="$candidate"
                break
            fi
        fi
    done

    if [ -n "$sys_xray" ]; then
        info "Found matching system xray at ${sys_xray}, adopting into isolated runtime..."
        cp -f "$sys_xray" "$xray_bin"
        chmod 0755 "$xray_bin"
        # Also copy geo assets if available on system
        for geo_src in /usr/share/xray /usr/local/share/xray; do
            [ -f "${geo_src}/geoip.dat" ] && cp -f "${geo_src}/geoip.dat" "${xray_dir}/" && cp -f "${geo_src}/geoip.dat" "${CONFIG_GEO_DIR}/" 2>/dev/null || true
            [ -f "${geo_src}/geosite.dat" ] && cp -f "${geo_src}/geosite.dat" "${xray_dir}/" && cp -f "${geo_src}/geosite.dat" "${CONFIG_GEO_DIR}/" 2>/dev/null || true
        done
        if [ "$(id -u)" -eq 0 ] && [ "$TARGET_USER" != "root" ]; then
            chown -R "${TARGET_USER}:" "$xray_dir" "$CONFIG_GEO_DIR" 2>/dev/null || true
        fi
        info "Xray-core ready: $(as_target "$xray_bin" version 2>&1 | head -1 || true)"
        return
    fi

    # 3. Download official precompiled release into isolated runtime
    local xray_zip="Xray-linux-${XRAY_ARCH}.zip"
    local xray_url1="https://github.com/XTLS/Xray-core/releases/download/${XRAY_VERSION}/${xray_zip}"
    local xray_url2="https://ghfast.top/https://github.com/XTLS/Xray-core/releases/download/${XRAY_VERSION}/${xray_zip}"
    local tmp_dir="$WORK_DIR/xray"

    info "Downloading precompiled Xray-core ${XRAY_VERSION} (${XRAY_ARCH})..."
    mkdir -p "$tmp_dir"
    if download_file_with_fallback "${tmp_dir}/${xray_zip}" "$xray_url1" "$xray_url2"; then
        if ! verify_xray_archive "${tmp_dir}/${xray_zip}" "$xray_zip" "$tmp_dir"; then
            rm -rf "$tmp_dir"
            exit 1
        fi

        extract_zip "${tmp_dir}/${xray_zip}" "$tmp_dir"
        install -Dm755 "${tmp_dir}/xray" "$xray_bin"

        # Install bundled geo assets into isolated runtime & config geo directory
        [ -f "${tmp_dir}/geoip.dat" ] && cp -f "${tmp_dir}/geoip.dat" "${xray_dir}/geoip.dat" && cp -f "${tmp_dir}/geoip.dat" "${CONFIG_GEO_DIR}/geoip.dat"
        [ -f "${tmp_dir}/geosite.dat" ] && cp -f "${tmp_dir}/geosite.dat" "${xray_dir}/geosite.dat" && cp -f "${tmp_dir}/geosite.dat" "${CONFIG_GEO_DIR}/geosite.dat"

        if [ "$(id -u)" -eq 0 ] && [ "$TARGET_USER" != "root" ]; then
            chown -R "${TARGET_USER}:" "$xray_dir" "$CONFIG_GEO_DIR" 2>/dev/null || true
        fi

        rm -rf "$tmp_dir"
        info "Isolated Xray-core installed: $(as_target "$xray_bin" version 2>&1 | head -1 || true)"
    else
        rm -rf "$tmp_dir"
        XRAY_DEGRADED=true
        warn "Failed to download Xray-core archive."
        warn "WhoisThat core will attempt automatic background download upon startup."
    fi
}

# --- pre-flight checks -------------------------------------------------------
check_disk_space() {
    local avail need=262144
    [ "$INSTALL_FROM_SOURCE" = "true" ] && need=2097152
    avail=$(df -Pk /tmp /usr/bin 2>/dev/null | awk 'NR > 1 {print $4}' | sort -n | head -1 || true)
    if [ -n "${avail:-}" ] && [ "$avail" -lt "$need" ]; then
        if [ "$INSTALL_FROM_SOURCE" = "true" ]; then
            warn "Less than 2 GB free on /tmp or /usr/bin; the build may fail."
        else
            warn "Less than 256 MB free on /tmp or /usr/bin; the download may fail."
        fi
    fi
}

# --- post-install verification -----------------------------------------------
verify_installation() {
    local tui core
    tui=$(as_target "$BIN_DIR/whoisthat" --version 2>/dev/null | head -1 || true)
    core=$(as_target "$BIN_DIR/whoisthat-core" --version 2>/dev/null | head -1 || true)

    if [ -z "$core" ]; then
        # File capabilities make the binary impossible to exec when the
        # container's bounding set lacks NET_ADMIN/NET_RAW. Probe an uncapped
        # copy so the installer can still verify the binary itself.
        local probe="$WORK_DIR/whoisthat-core.probe"
        if cp "$BIN_DIR/whoisthat-core" "$probe" 2>/dev/null; then
            chmod +x "$probe"
            core=$(as_target "$probe" --version 2>/dev/null | head -1 || true)
            if [ -n "$core" ]; then
                warn "whoisthat-core cannot run with file capabilities in this environment."
                warn "TUN mode needs NET_ADMIN and NET_RAW in the bounding set (containers need --cap-add)."
            fi
        fi
    fi

    if [ -z "$tui" ] || [ -z "$core" ]; then
        err "Installed binaries did not run:"
        err "  whoisthat: ${tui:-<no output>}"
        err "  whoisthat-core: ${core:-<no output>}"
        exit 1
    fi
    info "Verified: ${tui}; ${core}"
    # doctor exits non-zero on missing capabilities, which is exactly the state
    # after a failed setcap, so it is a hint here, never a failure.
    if ! as_target "$BIN_DIR/whoisthat" doctor >/dev/null 2>&1; then
        warn "whoisthat doctor reported issues; run 'whoisthat doctor' for details."
    fi
}

# --- final banner & instructions ---------------------------------------------
print_final_message() {
    local action="installed"
    [ "$MODE" = "Upgrade" ] && action="upgraded"

    echo
    echo -e "${GREEN}======================================================${NC}"
    echo -e "${GREEN}  WhoisThat ${action} successfully!${NC}"
    echo -e "${GREEN}======================================================${NC}"
    echo
    if [ "$XRAY_DEGRADED" = "true" ]; then
        echo -e "  ${YELLOW}[!] Xray-core ${XRAY_VERSION} is not installed (download failed).${NC}"
        echo -e "      TUN mode is unavailable until the core fetches it on first start."
        echo
    fi
    echo -e "  Launch:"
    echo -e "    ${BOLD}whoisthat${NC}  (or the WhoisThat app-menu entry)"
    echo
    case ":$PATH:" in
        *":$BIN_DIR:"*) ;;
        *)
            echo -e "  ${YELLOW}[!]${NC} $BIN_DIR is not in PATH; add it or use the full path."
            echo
            ;;
    esac
    if [ "$MODE" = "Upgrade" ]; then
        echo -e "  ${YELLOW}[!]${NC} A core that was already running keeps the previous version."
        echo -e "      Restart it when convenient: ${BOLD}whoisthat core restart${NC}"
        echo
    fi
    echo -e "  Modes:"
    echo -e "    • Proxy mode:     SOCKS5 (127.0.0.1:3090) & HTTP (127.0.0.1:3091)"
    echo -e "    • TUN mode:        Press '${BOLD}v${NC}' inside the TUI for full-system routing"
    echo -e "                      (runs as regular user via Linux ambient capabilities)"
    echo
    echo -e "  Key bindings:"
    echo -e "    Press ${BOLD}h${NC} inside the TUI for the full help and all hotkeys."
    echo
    echo -e "  Configuration & Data:"
    echo -e "    Config:    ~/.config/whoisthat/"
    echo -e "    Database:  ~/.local/share/whoisthat/db/ (AES-256-GCM encrypted)"
    echo -e "    Runtimes:  ~/.local/share/whoisthat/runtimes/ (isolated Xray-core)"
    echo
    echo -e "  Diagnostics:"
    echo -e "    Run '${BOLD}whoisthat doctor${NC}' to inspect system health and runtime status"
    echo

    if [ -d "/usr/local/go/bin" ] && ! grep -q '/usr/local/go/bin' "$TARGET_HOME/.profile" 2>/dev/null; then
        echo -e "  ${YELLOW}[!] To make Go available in future shells, run:${NC}"
        echo -e "      source ~/.profile"
        echo
    fi
}

# --- main --------------------------------------------------------------------
main() {
    parse_args "$@"
    require_platform_and_privileges

    WORK_DIR="$(mktemp -d)"
    BUILD_DIR="$WORK_DIR/src"
    ARTIFACT_DIR="$WORK_DIR/bin"
    if [ "$(id -u)" -eq 0 ] && [ "$TARGET_USER" != "root" ] && [ -n "$TARGET_HOME" ]; then
        chown "$TARGET_USER:" "$WORK_DIR"
    fi

    if [ "$UNINSTALL_MODE" = "true" ]; then
        uninstall_whoisthat
    fi

    assert_not_package_installed

    if command -v whoisthat &>/dev/null || [ -f "$BIN_DIR/whoisthat" ] || [ -f "$LEGACY_BIN_DIR/whoisthat" ]; then
        MODE="Upgrade"
        info "Existing whoisthat installation found -> Upgrading"
        # Do not stop the running core: the daemon keeps the old inode and the TUI
        # replaces it on its next launch, so an active VPN is not interrupted here.
    fi

    echo
    echo -e "${CYAN}${BOLD}  WhoisThat — Universal ${MODE}er${NC}"
    echo -e "${CYAN}  ======================================${NC}"
    echo

    detect_arch
    detect_distro
    ensure_sudo
    check_disk_space
    if [ "$INSTALL_FROM_SOURCE" = "true" ]; then
        install_system_deps
        install_go
        install_rust
        build_whoisthat
    else
        install_runtime_deps
        download_release_artifacts
    fi
    install_xray
    ensure_sudo
    install_binaries
    verify_installation
    print_final_message
}

if ! (return 0 2>/dev/null); then
    main "$@"
fi
