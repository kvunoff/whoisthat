#!/usr/bin/env bash
# =============================================================================
# WhoisThat — Universal Installer & Updater
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash -s -- --yes
#
# Works for both fresh installs and upgrades. When whoisthat is already installed,
# it will rebuild from the latest tagged release and update the binaries.
#
# Components:
#   1. System prerequisites (build-essential/base-devel, git, curl, unzip, libcap)
#   2. Go 1.26+ (official go.dev distribution)
#   3. Rust stable (official rustup.rs distribution)
#   4. WhoisThat suite:
#        - whoisthat-core   (Go VPN daemon with ambient Linux capabilities & native URI parser)
#        - whoisthat        (Ratatui Rust TUI client)
#   5. Isolated Xray-core runtime (v26.9.9: ~/.local/share/whoisthat/runtimes/xray/)
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
GO_MIN_VERSION="1.26.0"
GO_INSTALL_VERSION="1.26.8"
XRAY_VERSION="v26.9.9"

# --- configuration & defaults ------------------------------------------------
WORK_DIR=""
BUILD_DIR=""
BUILD_SRC_DIR=""
GIT_REPO="https://github.com/kvunoff/whoisthat.git"
ASSUME_YES=false
NONINTERACTIVE=false
INSTALL_TUN=true
TARGET_BRANCH=""
BUILD_LOCAL=false
UNINSTALL_MODE=false
INSECURE_SKIP_VERIFY=false
MODE="Install"
ARCH_FAMILY=""
GO_ARCH=""
XRAY_ARCH=""
T2S_ARCH=""
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
    BLUE='\033[0;34m'
    CYAN='\033[0;36m'
    BOLD='\033[1m'
    NC='\033[0m'
else
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    CYAN=''
    BOLD=''
    NC=''
fi

info()  { echo -e "${GREEN}[+]${NC} $*"; }
warn()  { echo -e "${YELLOW}[!]${NC} $*"; }
err()   { echo -e "${RED}[-]${NC} $*"; }
step()  { echo -e "\n${CYAN}==>${NC} ${BOLD}$*${NC}"; }

# --- platform & privilege setup ----------------------------------------------
[[ "$(uname)" == "Linux" ]] || { err "Only Linux is supported."; exit 1; }

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

# Determine sudo requirement (support running directly as root)
if [ "$(id -u)" -eq 0 ]; then
    SUDO=""
elif command -v sudo &>/dev/null; then
    SUDO="sudo"
else
    err "This installer requires superuser privileges to install binaries to /usr/local/bin."
    err "Please install 'sudo' or run as root."
    exit 1
fi

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
  -y, --yes          Automatic yes to prompts (install all components)
  --branch <name>    Build from a specific git branch or tag (default: latest release tag)
  --local            Build directly from current repository directory instead of cloning
  --insecure-skip-verify  Skip Xray archive checksum verification (DANGEROUS, not recommended)
  --uninstall        Remove whoisthat binaries from /usr/local/bin
  -h, --help         Show this help message and exit

Environment Variables:
  NONINTERACTIVE=1   Assume non-interactive execution (defaults to skipping optional prompts unless -y)
  ASSUME_YES=1       Equivalent to --yes
  WHOISTHAT_BRANCH   Equivalent to --branch

EOF
}

prompt_yes_no() {
    local prompt="$1"
    local default="${2:-N}"
    local answer=""

    if [ "$ASSUME_YES" = "true" ]; then
        return 0
    fi
    if [ "$NONINTERACTIVE" = "true" ]; then
        [[ "$default" =~ ^[Yy]$ ]] && return 0 || return 1
    fi

    # Read from /dev/tty if available (crucial for curl | bash)
    if [ -c /dev/tty ]; then
        read -rp "$prompt " answer < /dev/tty || answer="$default"
    elif [ -t 0 ]; then
        read -rp "$prompt " answer || answer="$default"
    else
        answer="$default"
    fi

    [[ "$answer" =~ ^[Yy]$ ]]
}

version_ge() {
    # Returns 0 if version $1 >= version $2
    local v1="$1" v2="$2"
    [[ "$v1" =~ ^[0-9]+\.[0-9]+$ ]] && v1="${v1}.0"
    [[ "$v2" =~ ^[0-9]+\.[0-9]+$ ]] && v2="${v2}.0"
    [ "$(printf '%s\n%s\n' "$v2" "$v1" | sort -V | head -n1)" = "$v2" ]
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
        chown -R "${TARGET_USER}:" "$dir" 2>/dev/null || true
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

# --- argument parsing --------------------------------------------------------
parse_args() {
    [ "${ASSUME_YES:-0}" = "1" ] && ASSUME_YES=true
    [ "${NONINTERACTIVE:-0}" = "1" ] && NONINTERACTIVE=true
    [ -n "${WHOISTHAT_BRANCH:-}" ] && TARGET_BRANCH="$WHOISTHAT_BRANCH"

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
            --branch)
                if [ -n "${2:-}" ]; then
                    TARGET_BRANCH="$2"
                    shift 2
                else
                    err "--branch requires an argument"
                    exit 1
                fi
                ;;
            --local)
                BUILD_LOCAL=true
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
                warn "Unknown option: $1"
                shift
                ;;
        esac
    done
}

# --- uninstallation ----------------------------------------------------------
uninstall_whoisthat() {
    step "Uninstalling WhoisThat"
    ensure_sudo

    local removed=0
    for bin in whoisthat whoisthat-core whoisthat-parser; do
        if [ -f "/usr/local/bin/$bin" ]; then
            $SUDO rm -f "/usr/local/bin/$bin"
            info "Removed /usr/local/bin/$bin"
            removed=1
        fi
    done

    if [ "$removed" -eq 0 ]; then
        info "No WhoisThat binaries were found in /usr/local/bin."
    else
        info "WhoisThat binaries removed successfully."
    fi

    # Systemd user unit (created by the TUI's autostart toggle) and the desktop
    # entries (installed by the package or registered by the wit:// handler).
    systemctl --user disable --now whoisthat-core.service >/dev/null 2>&1 || true
    local unit="$TARGET_HOME/.config/systemd/user/whoisthat-core.service"
    if [ -f "$unit" ]; then
        rm -f "$unit"
        systemctl --user daemon-reload >/dev/null 2>&1 || true
        info "Removed $unit"
    fi

    local user_desktop="$TARGET_HOME/.local/share/applications/whoisthat.desktop"
    if [ -f "$user_desktop" ]; then
        rm -f "$user_desktop"
        info "Removed $user_desktop"
    fi
    if [ -f "/usr/share/applications/whoisthat.desktop" ]; then
        $SUDO rm -f "/usr/share/applications/whoisthat.desktop"
        info "Removed /usr/share/applications/whoisthat.desktop"
    fi
    update-desktop-database "$TARGET_HOME/.local/share/applications" >/dev/null 2>&1 || true
    update-desktop-database /usr/share/applications >/dev/null 2>&1 || true

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
            T2S_ARCH="amd64"
            ;;
        aarch64|arm64)
            ARCH_FAMILY="arm64"
            GO_ARCH="arm64"
            XRAY_ARCH="arm64-v8a"
            T2S_ARCH="arm64"
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
    step "Step 1/6: System prerequisites"

    case "$DISTRO_ID" in
        debian|ubuntu|linuxmint|pop)
            info "Debian/Ubuntu family — installing build-essential, git, curl, unzip, libcap2-bin"
            $SUDO apt-get update -qq
            $SUDO apt-get install -y -qq build-essential git curl unzip libcap2-bin
            return
            ;;
        fedora|rhel|centos|rocky|almalinux)
            info "Fedora/RHEL family — installing gcc, git, curl, make, unzip, libcap"
            $SUDO dnf install -y -q gcc git curl make unzip libcap
            return
            ;;
        arch|manjaro|endeavouros)
            info "Arch family — installing base-devel, git, curl, unzip, libcap"
            $SUDO pacman -S --noconfirm --needed base-devel git curl unzip libcap
            return
            ;;
        alpine)
            info "Alpine — installing build-base, git, curl, unzip, libcap"
            $SUDO apk add --no-cache build-base git curl unzip libcap
            return
            ;;
        opensuse*|suse)
            info "openSUSE — installing gcc, git, curl, make, unzip, libcap-progs"
            $SUDO zypper install -y -l gcc git curl make unzip libcap-progs
            return
            ;;
    esac

    # Fallback for derivative distributions via ID_LIKE
    case "${DISTRO_LIKE}" in
        *debian*|*ubuntu*)
            info "Debian/Ubuntu derivative (${DISTRO_ID}) — installing build-essential, git, curl, unzip, libcap2-bin"
            $SUDO apt-get update -qq
            $SUDO apt-get install -y -qq build-essential git curl unzip libcap2-bin
            ;;
        *fedora*|*rhel*)
            info "Fedora/RHEL derivative (${DISTRO_ID}) — installing gcc, git, curl, make, unzip, libcap"
            $SUDO dnf install -y -q gcc git curl make unzip libcap
            ;;
        *arch*)
            info "Arch derivative (${DISTRO_ID}) — installing base-devel, git, curl, unzip, libcap"
            $SUDO pacman -S --noconfirm --needed base-devel git curl unzip libcap
            ;;
        *)
            warn "Unrecognized distribution (${DISTRO_ID})."
            warn "Ensure you have: C compiler, make, git, curl, unzip, and libcap (setcap)."
            ;;
    esac
}

# --- step 2: Go toolchain ----------------------------------------------------
install_go() {
    step "Step 2/6: Verify Go toolchain (>= ${GO_MIN_VERSION})"

    # Check existing environment PATH + standard /usr/local/go/bin
    if [ -d "/usr/local/go/bin" ] && [[ ":$PATH:" != *":/usr/local/go/bin:"* ]]; then
        export PATH="/usr/local/go/bin:$PATH"
    fi

    if command -v go &>/dev/null; then
        local current_go
        current_go=$(go version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1)
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
    step "Step 3/6: Verify Rust toolchain"

    local rust_ver
    if as_target sh -c 'command -v cargo >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1'; then
        rust_ver=$(as_target rustc --version 2>/dev/null | awk '{print $2}' || true)
        if ! version_ge "${rust_ver:-0}" "1.79"; then
            err "Rust ${rust_ver:-unknown} is too old: whoisthat needs 1.79 or newer."
            err "Update it with: rustup update stable"
            exit 1
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
    step "Step 4/6: Build WhoisThat suite"

    local src_dir="$BUILD_DIR"

    # Option to build in-place if running directly from local repo clone
    if [ "$BUILD_LOCAL" = "true" ] || ([ -f "./Cargo.toml" ] && [ -d "./core/core" ] && [ -z "$TARGET_BRANCH" ]); then
        info "Building directly from current directory: $(pwd)"
        src_dir="$(pwd)"
    else
        local tag="$TARGET_BRANCH"
        if [ -z "$tag" ]; then
            info "Looking up latest release tag from GitHub..."
            tag=$(git ls-remote --tags --sort=-version:refname "$GIT_REPO" 'refs/tags/v*' 2>/dev/null \
                | grep -v '\^{}' \
                | head -1 \
                | awk '{print $2}' \
                | sed 's|refs/tags/||' || true)
            if [ -z "$tag" ]; then
                tag=$(git ls-remote --tags "$GIT_REPO" 'refs/tags/v*' 2>/dev/null \
                    | grep -v '\^{}' \
                    | awk '{print $2}' \
                    | sed 's|refs/tags/||' \
                    | sort -V \
                    | tail -n1 || true)
            fi
            if [ -z "$tag" ]; then
                warn "Could not determine latest release tag. Falling back to 'main' branch."
                tag="main"
            fi
        fi

        info "Cloning whoisthat (${tag}) into ${BUILD_DIR}..."
        as_target git clone --depth 1 --branch "${tag}" "$GIT_REPO" "$BUILD_DIR"
    fi

    cd "$src_dir"

    # 1. whoisthat-core (Go)
    info "1/2 Building whoisthat-core..."
    ( cd "$src_dir/core/core" && as_target go build -o whoisthat-core )
    [ -f "$src_dir/core/core/whoisthat-core" ] || { err "whoisthat-core build failed"; exit 1; }

    # 2. whoisthat TUI (Rust)
    info "2/2 Building whoisthat TUI..."
    ( cd "$src_dir" && as_target cargo build --release --locked )
    [ -f "$src_dir/target/release/whoisthat" ] || { err "whoisthat TUI build failed"; exit 1; }

    BUILD_SRC_DIR="$src_dir"
    info "All WhoisThat components built successfully."
}

# --- step 5: install binaries & set capabilities -----------------------------
install_binaries() {
    step "Step 5/6: Install binaries to /usr/local/bin"

    cd "$BUILD_SRC_DIR"

    info "Installing whoisthat and whoisthat-core..."
    $SUDO install -Dm755 target/release/whoisthat              /usr/local/bin/whoisthat
    $SUDO install -Dm755 core/core/whoisthat-core               /usr/local/bin/whoisthat-core
    if [ -f "pkg/whoisthat.desktop" ]; then
        $SUDO install -Dm644 pkg/whoisthat.desktop              /usr/share/applications/whoisthat.desktop
        if command -v update-desktop-database >/dev/null 2>&1; then
            $SUDO update-desktop-database /usr/share/applications 2>/dev/null || true
        fi
    fi

    # Clean up legacy parser binary if present from earlier versions (< v0.11.5)
    if [ -f "/usr/local/bin/whoisthat-parser" ]; then
        info "Removing obsolete /usr/local/bin/whoisthat-parser (parser is now embedded in whoisthat-core)..."
        $SUDO rm -f "/usr/local/bin/whoisthat-parser"
    fi

    info "Granting network capabilities to whoisthat-core..."
    if $SUDO setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /usr/local/bin/whoisthat-core 2>/dev/null; then
        info "Capabilities configured: cap_net_admin, cap_net_raw, cap_setpcap"
    else
        warn "setcap failed or filesystem does not support capabilities."
        warn "WhoisThat will offer pkexec capability setup on startup if required for TUN mode."
    fi
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

# --- step 6: install Xray-core -----------------------------------------------
install_xray() {
    step "Step 6/6: Isolated Xray-core runtime (${XRAY_VERSION})"

    local xray_dir="${RUNTIMES_DIR}/xray/${XRAY_VERSION}"
    local xray_bin="${xray_dir}/xray"

    ensure_user_dir "$xray_dir"
    ensure_user_dir "$CONFIG_GEO_DIR"

    # 1. Check if already installed in isolated runtime directory
    if [ -x "$xray_bin" ]; then
        local current_ver
        current_ver="$(as_target "$xray_bin" version 2>&1 | head -1 || true)"
        if echo "$current_ver" | grep -q "${XRAY_VERSION#v}"; then
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
            local cand_ver
            cand_ver="$(as_target "$candidate" version 2>&1 | head -1 || true)"
            if echo "$cand_ver" | grep -q "${XRAY_VERSION#v}"; then
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
        warn "Failed to download Xray-core archive."
        warn "WhoisThat core will attempt automatic background download upon startup."
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
    echo -e "  Launch:"
    echo -e "    ${BOLD}whoisthat${NC}"
    echo
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
    echo -e "    ${BOLD}j / k / ↑ / ↓${NC}  — navigate profiles"
    echo -e "    ${BOLD}Enter / c${NC}      — connect / reconnect"
    echo -e "    ${BOLD}d${NC}              — disconnect"
    echo -e "    ${BOLD}v${NC}              — toggle TUN mode"
    echo -e "    ${BOLD}t / T${NC}          — test latency (t = focused profile, T = all)"
    echo -e "    ${BOLD}C${NC}              — cancel running tests"
    echo -e "    ${BOLD}U${NC}              — add new subscription (group)"
    echo -e "    ${BOLD}u${NC}              — update subscription"
    echo -e "    ${BOLD}e${NC}              — edit subscription"
    echo -e "    ${BOLD}X${NC}              — delete subscription"
    echo -e "    ${BOLD}y${NC}              — copy profile URI"
    echo -e "    ${BOLD}h${NC}              — view full help & all hotkeys"
    echo -e "    ${BOLD}q${NC}              — detach (VPN keeps running in background)"
    echo -e "    ${BOLD}Q / Ctrl+C${NC}     — full quit (stops VPN and exits)"
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

    WORK_DIR="$(mktemp -d)"
    BUILD_DIR="$WORK_DIR/src"
    if [ "$(id -u)" -eq 0 ] && [ "$TARGET_USER" != "root" ] && [ -n "$TARGET_HOME" ]; then
        chown "$TARGET_USER:" "$WORK_DIR"
    fi

    if [ "$UNINSTALL_MODE" = "true" ]; then
        uninstall_whoisthat
    fi

    if command -v whoisthat &>/dev/null || [ -f /usr/local/bin/whoisthat ]; then
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
    install_system_deps
    install_go
    install_rust
    build_whoisthat
    install_binaries
    install_xray
    print_final_message
}

if ! (return 0 2>/dev/null); then
    main "$@"
fi
