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
#   2. Go 1.24+ (official go.dev distribution)
#   3. Rust stable (official rustup.rs distribution)
#   4. WhoisThat suite:
#        - whoisthat-parser (standalone Rust URI -> Xray JSON generator)
#        - whoisthat-core   (Go VPN daemon with ambient Linux capabilities)
#        - whoisthat        (Ratatui Rust TUI client)
#   5. Xray-core (official release: xray, geoip.dat, geosite.dat)
#   6. tun2socks (optional: official release for system-wide TUN mode)
#   7. hysteria2 (optional: official client for hysteria2:// / hy2:// profiles)
# =============================================================================
set -euo pipefail

# --- version constants -------------------------------------------------------
GO_MIN_VERSION="1.24.0"
GO_INSTALL_VERSION="1.25.0"
XRAY_VERSION="v1.8.23"
TUN2SOCKS_VERSION="v2.5.2"
HYSTERIA_VERSION="2.9.3"

# --- configuration & defaults ------------------------------------------------
BUILD_DIR="/tmp/whoisthat-build-$$"
GIT_REPO="https://github.com/kvunoff/whoisthat.git"
ASSUME_YES=false
NONINTERACTIVE=false
INSTALL_TUN=true
INSTALL_HY2=true
TARGET_BRANCH=""
BUILD_LOCAL=false
UNINSTALL_MODE=false
MODE="Install"

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
    if [ -d "$BUILD_DIR" ] && [ "${SKIP_CLEANUP:-false}" != "true" ]; then
        rm -rf "$BUILD_DIR"
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

# --- helper functions --------------------------------------------------------
show_help() {
    cat <<EOF
WhoisThat Universal Installer & Updater

Usage:
  install.sh [options]
  curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash -s -- [options]

Options:
  -y, --yes          Automatic yes to prompts (install all optional components)
  --no-tun           Skip installation of tun2socks (TUN mode engine)
  --no-hy2           Skip installation of hysteria2 client
  --branch <name>    Build from a specific git branch or tag (default: latest release tag)
  --local            Build directly from current repository directory instead of cloning
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
                INSTALL_TUN=false
                shift
                ;;
            --no-hy2)
                INSTALL_HY2=false
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

    echo
    warn "User configuration (~/.config/whoisthat) and database (~/.local/share/whoisthat) were preserved."
    warn "To completely delete all user data and credentials, run:"
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
            HY_ARCH="amd64"
            ;;
        aarch64|arm64)
            ARCH_FAMILY="arm64"
            GO_ARCH="arm64"
            XRAY_ARCH="arm64-v8a"
            T2S_ARCH="arm64"
            HY_ARCH="arm64"
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
    else
        DISTRO_ID="unknown"
        DISTRO_NAME="Unknown Linux"
    fi
    info "Distribution: ${DISTRO_NAME}"
}

# --- step 1: system build tools ----------------------------------------------
install_system_deps() {
    step "Step 1/8: System prerequisites"

    case "$DISTRO_ID" in
        debian|ubuntu|linuxmint|pop)
            info "Debian/Ubuntu family — installing build-essential, git, curl, unzip, libcap2-bin"
            $SUDO apt-get update -qq
            $SUDO apt-get install -y -qq build-essential git curl unzip libcap2-bin
            ;;
        fedora|rhel|centos|rocky|almalinux)
            info "Fedora/RHEL family — installing gcc, git, curl, make, unzip, libcap"
            $SUDO dnf install -y -q gcc git curl make unzip libcap
            ;;
        arch|manjaro|endeavouros)
            info "Arch family — installing base-devel, git, curl, unzip, libcap"
            $SUDO pacman -S --noconfirm --needed base-devel git curl unzip libcap
            ;;
        alpine)
            info "Alpine — installing build-base, git, curl, unzip, libcap"
            $SUDO apk add --no-cache build-base git curl unzip libcap
            ;;
        opensuse*|suse)
            info "openSUSE — installing gcc, git, curl, make, unzip, libcap-progs"
            $SUDO zypper install -y -l gcc git curl make unzip libcap-progs
            ;;
        *)
            warn "Unrecognized distribution (${DISTRO_ID})."
            warn "Ensure you have: C compiler, make, git, curl, unzip, and libcap (setcap)."
            ;;
    esac
}

# --- step 2: Go toolchain ----------------------------------------------------
install_go() {
    step "Step 2/8: Verify Go toolchain (>= ${GO_MIN_VERSION})"

    # Check existing environment PATH + standard /usr/local/go/bin
    if [ -d "/usr/local/go/bin" ] && [[ ":$PATH:" != *":/usr/local/go/bin:"* ]]; then
        export PATH="/usr/local/go/bin:$PATH"
    fi

    if command -v go &>/dev/null; then
        local current_go
        current_go=$(go version 2>/dev/null | awk '{print $3}' | sed 's/^go//')
        if version_ge "$current_go" "$GO_MIN_VERSION"; then
            info "Go ${current_go} detected (>= ${GO_MIN_VERSION}), skipping installation"
            return
        fi
        warn "Found Go ${current_go}, but >= ${GO_MIN_VERSION} is required. Upgrading..."
    else
        info "Go not found in PATH. Installing Go ${GO_INSTALL_VERSION}..."
    fi

    local go_tarball="go${GO_INSTALL_VERSION}.linux-${GO_ARCH}.tar.gz"
    local go_url="https://go.dev/dl/${go_tarball}"
    local tmp_tar="/tmp/${go_tarball}"

    info "Downloading ${go_url}..."
    curl -fsSL "$go_url" -o "$tmp_tar"

    info "Extracting to /usr/local/go..."
    $SUDO rm -rf /usr/local/go
    $SUDO tar -C /usr/local -xzf "$tmp_tar"
    rm -f "$tmp_tar"

    export PATH="/usr/local/go/bin:$PATH"

    local profile_file="$HOME/.profile"
    if [ -f "$profile_file" ] && ! grep -q '/usr/local/go/bin' "$profile_file" 2>/dev/null; then
        echo 'export PATH="/usr/local/go/bin:$PATH"' >> "$profile_file"
        info "Added /usr/local/go/bin to ~/.profile"
    fi

    info "Go ready: $(go version)"
}

# --- step 3: Rust toolchain --------------------------------------------------
install_rust() {
    step "Step 3/8: Verify Rust toolchain"

    if [ -f "$HOME/.cargo/env" ]; then
        # shellcheck source=/dev/null
        source "$HOME/.cargo/env"
    elif [ -d "$HOME/.cargo/bin" ] && [[ ":$PATH:" != *":$HOME/.cargo/bin:"* ]]; then
        export PATH="$HOME/.cargo/bin:$PATH"
    fi

    if command -v cargo &>/dev/null && command -v rustc &>/dev/null; then
        local rust_ver
        rust_ver=$(rustc --version | awk '{print $2}')
        info "Rust ${rust_ver} detected, skipping installation"
        return
    fi

    info "Installing Rust stable via official rustup installer (https://rustup.rs)..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain stable

    if [ -f "$HOME/.cargo/env" ]; then
        # shellcheck source=/dev/null
        source "$HOME/.cargo/env"
    else
        export PATH="$HOME/.cargo/bin:$PATH"
    fi

    info "Rust ready: $(rustc --version)"
}

# --- step 4: build WhoisThat binaries ----------------------------------------
build_whoisthat() {
    step "Step 4/8: Build WhoisThat suite"

    local src_dir="$BUILD_DIR"

    # Option to build in-place if running directly from local repo clone
    if [ "$BUILD_LOCAL" = "true" ] || ([ -f "./Cargo.toml" ] && [ -d "./core/core" ] && [ -d "./parser" ] && [ -z "$TARGET_BRANCH" ]); then
        info "Building directly from current directory: $(pwd)"
        src_dir="$(pwd)"
        SKIP_CLEANUP=true
    else
        rm -rf "$BUILD_DIR"
        mkdir -p "$BUILD_DIR"

        local tag="$TARGET_BRANCH"
        if [ -z "$tag" ]; then
            info "Looking up latest release tag from GitHub..."
            tag=$(git ls-remote --tags --sort=-version:refname "$GIT_REPO" 'refs/tags/v*' \
                | grep -v '\^{}' \
                | head -1 \
                | awk '{print $2}' \
                | sed 's|refs/tags/||')
            if [ -z "$tag" ]; then
                warn "Could not determine latest release tag. Falling back to 'main' branch."
                tag="main"
            fi
        fi

        info "Cloning whoisthat (${tag}) into ${BUILD_DIR}..."
        git clone --depth 1 --branch "${tag}" "$GIT_REPO" "$BUILD_DIR"
    fi

    cd "$src_dir"

    # 1. whoisthat-parser (Rust)
    info "1/3 Building whoisthat-parser..."
    cargo build --release --manifest-path parser/Cargo.toml
    [ -f parser/target/release/whoisthat-parser ] || { err "whoisthat-parser build failed"; exit 1; }

    # 2. whoisthat-core (Go)
    info "2/3 Building whoisthat-core..."
    (cd core/core && go build -o whoisthat-core)
    [ -f core/core/whoisthat-core ] || { err "whoisthat-core build failed"; exit 1; }

    # 3. whoisthat TUI (Rust)
    info "3/3 Building whoisthat TUI..."
    cargo build --release
    [ -f target/release/whoisthat ] || { err "whoisthat TUI build failed"; exit 1; }

    BUILD_SRC_DIR="$src_dir"
    info "All WhoisThat components built successfully."
}

# --- step 5: install binaries & set capabilities -----------------------------
install_binaries() {
    step "Step 5/8: Install binaries to /usr/local/bin"

    cd "$BUILD_SRC_DIR"

    info "Installing whoisthat, whoisthat-core, whoisthat-parser..."
    $SUDO install -Dm755 target/release/whoisthat              /usr/local/bin/whoisthat
    $SUDO install -Dm755 core/core/whoisthat-core               /usr/local/bin/whoisthat-core
    $SUDO install -Dm755 parser/target/release/whoisthat-parser /usr/local/bin/whoisthat-parser

    info "Granting network capabilities to whoisthat-core..."
    if $SUDO setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /usr/local/bin/whoisthat-core 2>/dev/null; then
        info "Capabilities configured: cap_net_admin, cap_net_raw, cap_setpcap"
    else
        warn "setcap failed or filesystem does not support capabilities."
        warn "WhoisThat will offer pkexec capability setup on startup if required for TUN mode."
    fi
}

# --- step 6: install Xray-core -----------------------------------------------
install_xray() {
    step "Step 6/8: Verify Xray-core (${XRAY_VERSION})"

    if command -v xray &>/dev/null; then
        info "xray already installed: $(xray version 2>&1 | head -1)"
        return
    fi

    local xray_zip="Xray-linux-${XRAY_ARCH}.zip"
    local xray_url="https://github.com/XTLS/Xray-core/releases/download/${XRAY_VERSION}/${xray_zip}"
    local tmp_dir="/tmp/whoisthat-xray-$$"

    info "Downloading precompiled Xray-core ${XRAY_VERSION} (${XRAY_ARCH})..."
    mkdir -p "$tmp_dir"
    if curl -fsSL "$xray_url" -o "${tmp_dir}/${xray_zip}"; then
        extract_zip "${tmp_dir}/${xray_zip}" "$tmp_dir"
        $SUDO install -Dm755 "${tmp_dir}/xray" /usr/local/bin/xray

        # Install bundled geo assets for fallback routing
        $SUDO mkdir -p /usr/local/share/xray
        [ -f "${tmp_dir}/geoip.dat" ] && $SUDO install -Dm644 "${tmp_dir}/geoip.dat" /usr/local/share/xray/geoip.dat
        [ -f "${tmp_dir}/geosite.dat" ] && $SUDO install -Dm644 "${tmp_dir}/geosite.dat" /usr/local/share/xray/geosite.dat

        rm -rf "$tmp_dir"
        info "Xray-core installed: $(xray version 2>&1 | head -1)"
    else
        rm -rf "$tmp_dir"
        err "Failed to download Xray-core from ${xray_url}"
        exit 1
    fi
}

# --- step 7: install tun2socks (optional) ------------------------------------
install_tun2socks() {
    step "Step 7/8: Verify tun2socks (optional — TUN mode engine)"

    if command -v tun2socks &>/dev/null; then
        info "tun2socks already installed"
        return
    fi

    if [ "$INSTALL_TUN" = "false" ]; then
        info "Skipping tun2socks (--no-tun specified)"
        return
    fi

    warn "tun2socks enables transparent system-wide TUN mode."
    if ! prompt_yes_no "    Install tun2socks ${TUN2SOCKS_VERSION}? [y/N]" "N"; then
        info "Skipping tun2socks"
        return
    fi

    local t2s_zip="tun2socks-linux-${T2S_ARCH}.zip"
    local t2s_url="https://github.com/xjasonlyu/tun2socks/releases/download/${TUN2SOCKS_VERSION}/${t2s_zip}"
    local tmp_dir="/tmp/whoisthat-tun2socks-$$"

    info "Downloading tun2socks ${TUN2SOCKS_VERSION} (${T2S_ARCH})..."
    mkdir -p "$tmp_dir"
    if curl -fsSL "$t2s_url" -o "${tmp_dir}/${t2s_zip}"; then
        extract_zip "${tmp_dir}/${t2s_zip}" "$tmp_dir"
        local bin
        bin=$(find "$tmp_dir" -type f -name "tun2socks*" | head -1)
        if [ -n "$bin" ]; then
            $SUDO install -Dm755 "$bin" /usr/local/bin/tun2socks
            info "tun2socks installed successfully"
        else
            warn "tun2socks binary not found in downloaded archive"
        fi
        rm -rf "$tmp_dir"
    else
        rm -rf "$tmp_dir"
        warn "Failed to download tun2socks from ${t2s_url}"
    fi
}

# --- step 8: install hysteria2 (optional) ------------------------------------
install_hysteria2() {
    step "Step 8/8: Verify hysteria2 client (optional — hysteria2:// profiles)"

    if command -v hysteria &>/dev/null; then
        local hy_ver
        hy_ver=$(hysteria version 2>&1 | grep -m1 "Version:" | awk '{print $2}' || echo "")
        info "hysteria already installed: ${hy_ver:-ok}"
        return
    fi

    if [ "$INSTALL_HY2" = "false" ]; then
        info "Skipping hysteria2 (--no-hy2 specified)"
        return
    fi

    warn "xray-core does NOT implement the hysteria2 protocol."
    warn "Subscriptions containing hysteria2:// / hy2:// profiles require the hysteria binary."
    if ! prompt_yes_no "    Install hysteria2 client v${HYSTERIA_VERSION}? [y/N]" "N"; then
        info "Skipping hysteria2"
        warn "hysteria2:// / hy2:// profiles will not be able to connect without the hysteria binary."
        return
    fi

    local hy_bin="hysteria-linux-${HY_ARCH}"
    local hy_url="https://github.com/apernet/hysteria/releases/download/app%2Fv${HYSTERIA_VERSION}/${hy_bin}"
    local tmp_bin="/tmp/whoisthat-hysteria-$$"

    info "Downloading hysteria v${HYSTERIA_VERSION} (${HY_ARCH})..."
    if curl -fsSL "$hy_url" -o "$tmp_bin"; then
        local hy_ver
        hy_ver=$("$tmp_bin" version 2>&1 | grep -m1 "Version:" | awk '{print $2}' || echo "v${HYSTERIA_VERSION}")
        $SUDO install -Dm755 "$tmp_bin" /usr/local/bin/hysteria
        rm -f "$tmp_bin"
        info "hysteria installed: ${hy_ver}"
    else
        rm -f "$tmp_bin"
        warn "Failed to download hysteria client from ${hy_url}"
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
    echo -e "    ${BOLD}t / T${NC}          — test latency (t = all, T = selected)"
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
    echo -e "  Configuration:"
    echo -e "    Config:    ~/.config/whoisthat/"
    echo -e "    Database:  ~/.local/share/whoisthat/db/ (AES-256-GCM encrypted)"
    echo

    if [ -d "/usr/local/go/bin" ] && ! grep -q '/usr/local/go/bin' "$HOME/.profile" 2>/dev/null; then
        echo -e "  ${YELLOW}[!] To make Go available in future shells, run:${NC}"
        echo -e "      source ~/.profile"
        echo
    fi

    if ! command -v hysteria &>/dev/null; then
        echo -e "  ${YELLOW}[i] hysteria2 binary not installed${NC}"
        echo -e "      hysteria2:// / hy2:// profiles will not work without it."
        echo -e "      Install anytime: curl -fsSL https://github.com/apernet/hysteria/releases/download/app%2Fv${HYSTERIA_VERSION}/hysteria-linux-amd64 | sudo install -Dm755 /dev/stdin /usr/local/bin/hysteria"
        echo
    fi
}

# --- main --------------------------------------------------------------------
main() {
    parse_args "$@"

    if [ "$UNINSTALL_MODE" = "true" ]; then
        uninstall_whoisthat
    fi

    if command -v whoisthat &>/dev/null || [ -f /usr/local/bin/whoisthat ]; then
        MODE="Upgrade"
        local cur
        cur=$(whoisthat --version 2>/dev/null || echo "detected")
        info "Existing whoisthat installation found (${cur}) -> Upgrading"
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
    install_tun2socks
    install_hysteria2
    print_final_message
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
