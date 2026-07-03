#!/usr/bin/env bash
# =============================================================================
# WhoisThat — universal installer / updater
# Usage:  curl -fsSL https://raw.githubusercontent.com/kvunoff/whoisthat/main/install.sh | bash
#
# Works for both fresh installs and upgrades. If whoisthat is already installed
# it will rebuild from the latest tagged release and overwrite the old binaries.
#
# What it does:
#   1. Detects the Linux distribution
#   2. Installs system prerequisites (git, curl, C compiler)
#   3. Installs Go 1.24+ from the official tarball (go.dev)
#   4. Installs Rust via rustup (official installer)
#   5. Clones the latest tagged release and builds parser → core → TUI
#   6. Copies whoisthat, whoisthat-core, whoisthat-parser to /usr/local/bin
#   7. Installs xray-core (go install, pinned version)
#   8. Optionally: tun2socks for TUN mode
#
# Transparency: every step is printed to the terminal, nothing is hidden.
# Only the C compiler and basic tools come from distro repos.
# Go and Rust use official installers because distro packages are outdated.
# =============================================================================
set -euo pipefail

# --- constants ---------------------------------------------------------------
BUILD_DIR="/tmp/whoisthat-build"          # temporary build directory
GO_VERSION="1.25.0"                       # minimum Go version (bump when go.mod changes)
XRAY_VERSION="v1.8.23"                    # pinned Xray-core release
TUN2SOCKS_VERSION="v2.5.2"               # pinned tun2socks release
HYSTERIA_VERSION="v2.7.5"                 # pinned hysteria2 client release

# --- terminal colors ---------------------------------------------------------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

info()  { echo -e "${GREEN}[+]${NC} $*"; }
warn()  { echo -e "${YELLOW}[!]${NC} $*"; }
err()   { echo -e "${RED}[-]${NC} $*"; }
step()  { echo -e "\n${CYAN}==>${NC} ${YELLOW}$*${NC}"; }

# --- platform guard ----------------------------------------------------------
[[ "$(uname)" == "Linux" ]] || { err "Only Linux is supported"; exit 1; }

# --- cleanup on failure ------------------------------------------------------
trap 'err "Build failed. Cleaning up..."; rm -rf "$BUILD_DIR"' ERR

# --- detect the Linux distribution -------------------------------------------
detect_distro() {
    if [ -f /etc/os-release ]; then
        . /etc/os-release
        DISTRO_ID="${ID:-unknown}"
    else
        DISTRO_ID="unknown"
    fi
    info "Detected distro: ${DISTRO_ID}"
}

# --- install system-level build tools ----------------------------------------
install_system_deps() {
    step "Step 1/8: System prerequisites"

    # Only the C compiler + git + curl come from the distro repo.
    # Go and Rust are installed separately via official channels.
    case "$DISTRO_ID" in
        debian|ubuntu|linuxmint|pop)
            info "Debian/Ubuntu — installing build-essential git curl"
            sudo apt-get update -qq
            sudo apt-get install -y -qq build-essential git curl
            ;;
        fedora|rhel|centos|rocky|almalinux)
            info "Fedora/RHEL — installing gcc git curl make"
            sudo dnf install -y -q gcc git curl make
            ;;
        arch|manjaro|endeavouros)
            info "Arch — installing base-devel git curl"
            sudo pacman -S --noconfirm --needed base-devel git curl
            ;;
        alpine)
            info "Alpine — installing build-base git curl"
            sudo apk add --no-cache build-base git curl
            ;;
        opensuse*|suse)
            info "openSUSE — installing gcc git curl make"
            sudo zypper install -y -l gcc git curl make
            ;;
        *)
            warn "Unknown distro (${DISTRO_ID})."
            warn "Make sure you have: a C compiler (gcc/clang), git, curl, make."
            warn "Continuing anyway — build may fail if tools are missing."
            ;;
    esac
}

# --- install Go from the official tarball (go.dev) ---------------------------
#     Distro repos ship ancient versions. We need 1.24+ for omitzero.
install_go() {
    step "Step 2/8: Install Go ${GO_VERSION} (official tarball)"

    if ! command -v curl &>/dev/null; then
        err "curl is required to download Go. It should have been installed in step 1."
        exit 1
    fi

    # Check if the right version is already available
    if command -v go &>/dev/null; then
        local current_go
        current_go=$(go version 2>/dev/null | awk '{print $3}' | sed 's/^go//')
        if [ "$(printf '%s\n' "$GO_VERSION" "$current_go" | sort -V | head -1)" = "$GO_VERSION" ]; then
            info "Go ${current_go} already installed (>= ${GO_VERSION}), skipping"
            return
        fi
        warn "Found Go ${current_go} (need >= ${GO_VERSION}), upgrading"
    fi

    local go_arch
    case "$(uname -m)" in
        x86_64)  go_arch="amd64" ;;
        aarch64) go_arch="arm64" ;;
        *)       err "Unsupported architecture: $(uname -m)"; exit 1 ;;
    esac

    local go_tarball="go${GO_VERSION}.linux-${go_arch}.tar.gz"
    local go_url="https://go.dev/dl/${go_tarball}"

    info "Downloading from go.dev: ${go_url}"
    curl -fsSL "$go_url" -o "/tmp/${go_tarball}"

    info "Extracting to /usr/local/go"
    sudo rm -rf /usr/local/go
    sudo tar -C /usr/local -xzf "/tmp/${go_tarball}"
    rm -f "/tmp/${go_tarball}"

    # Add Go to PATH for the current session
    export PATH="/usr/local/go/bin:$PATH"

    # Add to shell profile so it survives terminal restarts
    local profile_file="$HOME/.profile"
    if ! grep -q '/usr/local/go/bin' "$profile_file" 2>/dev/null; then
        echo 'export PATH="/usr/local/go/bin:$PATH"' >> "$profile_file"
        info "Added /usr/local/go/bin to ~/.profile"
    fi

    info "Go installed: $(go version)"
}

# --- install Rust via rustup (official installer) ----------------------------
install_rust() {
    step "Step 3/8: Install Rust (rustup.rs)"

    if command -v rustc &>/dev/null; then
        local rust_ver
        rust_ver=$(rustc --version | awk '{print $2}')
        info "Rust ${rust_ver} already installed, skipping"
        return
    fi

    info "Running the official rustup installer (rustup.rs)"
    # --default-toolchain stable — install the stable compiler
    # -y — non-interactive (no prompts)
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain stable

    # rustup installs everything into ~/.cargo/bin
    if [ -f "$HOME/.cargo/env" ]; then
        # shellcheck source=/dev/null
        source "$HOME/.cargo/env"
    else
        export PATH="$HOME/.cargo/bin:$PATH"
    fi

    info "Rust installed: $(rustc --version)"
}

# --- check that sudo is available --------------------------------------------
ensure_sudo() {
    if ! command -v sudo &>/dev/null; then
        warn "sudo not found — some steps require root."
        warn "If the script fails, install sudo or run as root."
    fi
}

# --- build WhoisThat (parser → core → TUI) -----------------------------------
#     Clones the latest tagged release (stable), not the main branch.
#     Verifies that each build step produced the expected binary.
build_whoisthat() {
    step "Step 4/8: Build WhoisThat"

    rm -rf "$BUILD_DIR"

    # Fetch the latest release tag so we build a stable version, not HEAD.
    # Uses git ls-remote (no GitHub API rate limit).
    info "Looking up latest release tag..."
    local tag
    tag=$(git ls-remote --tags --sort=-version:refname \
        https://github.com/kvunoff/whoisthat.git 'refs/tags/v*' \
        | head -1 | awk '{print $2}' | sed 's|refs/tags/||')
    if [ -z "$tag" ]; then
        err "Could not find any release tag. Falling back to main branch."
        tag="main"
    fi
    info "Cloning whoisthat ${tag} into ${BUILD_DIR}..."
    git clone --depth 1 --branch "${tag}" https://github.com/kvunoff/whoisthat.git "$BUILD_DIR"
    cd "$BUILD_DIR"

    # 1. Build the parser (whoisthat-parser) — standalone Rust binary.
    #    Parses VLESS/VMess/Trojan/SS URIs and generates Xray JSON config.
    info "Building whoisthat-parser..."
    cargo build --release --manifest-path parser/Cargo.toml
    if [ ! -f parser/target/release/whoisthat-parser ]; then
        err "whoisthat-parser build failed — binary not found"
        exit 1
    fi
    info "  -> $(parser/target/release/whoisthat-parser --version 2>/dev/null || echo 'ok')"

    # 2. Build the Go core (whoisthat-core) — the VPN engine.
    #    Manages Xray, TUN, profile DB, and the TCP command server.
    info "Building whoisthat-core..."
    (cd core/core && go build -o whoisthat-core)
    if [ ! -f core/core/whoisthat-core ]; then
        err "whoisthat-core build failed — binary not found"
        exit 1
    fi
    info "  -> ok"

    # 3. Build the Rust TUI (whoisthat) — the terminal interface.
    info "Building whoisthat TUI..."
    cargo build --release
    if [ ! -f target/release/whoisthat ]; then
        err "whoisthat TUI build failed — binary not found"
        exit 1
    fi
    info "  -> ok"

    info "Build complete."
}

# --- install binaries to /usr/local/bin --------------------------------------
install_binaries() {
    step "Step 5/8: Install binaries to /usr/local/bin"

    cd "$BUILD_DIR"

    info "Copying whoisthat, whoisthat-core, whoisthat-parser"
    sudo install -Dm755 target/release/whoisthat              /usr/local/bin/whoisthat
    sudo install -Dm755 core/core/whoisthat-core               /usr/local/bin/whoisthat-core
    sudo install -Dm755 parser/target/release/whoisthat-parser /usr/local/bin/whoisthat-parser
    sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /usr/local/bin/whoisthat-core

    info "Binaries installed:"
    info "  whoisthat        — TUI (run 'whoisthat' to start)"
    info "  whoisthat-core   — VPN engine (auto-spawned by the TUI)"
    info "  whoisthat-parser — URI → Xray config (internal)"
}

# --- install Xray-core (pinned version for reproducibility) ------------------
install_xray() {
    step "Step 6/8: Install Xray-core ${XRAY_VERSION}"

    if command -v xray &>/dev/null; then
        info "xray already installed: $(xray version 2>&1 | head -1)"
        return
    fi

    # go install with a pinned tag ensures reproducible builds.
    info "Installing Xray-core via go install (may take a minute)..."
    go install "github.com/XTLS/Xray-core@${XRAY_VERSION}"

    # go install places the binary into GOPATH/bin; move it to a system path.
    local gobin="${GOPATH:-$HOME/go}/bin"
    if [ -f "$gobin/xray" ]; then
        sudo install -Dm755 "$gobin/xray" /usr/local/bin/xray
        info "Xray-core installed: $(xray version 2>&1 | head -1)"
    elif [ -f "$HOME/go/bin/xray" ]; then
        sudo install -Dm755 "$HOME/go/bin/xray" /usr/local/bin/xray
        info "Xray-core installed"
    else
        warn "Xray-core binary not found in GOPATH."
        warn "Install manually: go install github.com/XTLS/Xray-core@${XRAY_VERSION}"
        warn "Then copy: sudo install -Dm755 ~/go/bin/xray /usr/local/bin/xray"
    fi
}

# --- install tun2socks (optional, for TUN mode only) -------------------------
install_tun2socks() {
    step "Step 7/8: Install tun2socks (optional — TUN mode only)"

    if command -v tun2socks &>/dev/null; then
        info "tun2socks already installed"
        return
    fi

    # Explicit opt-in — do not install unless the user says yes.
    warn "TUN mode requires root privileges and does not work on all systems."
    read -rp "    Install tun2socks ${TUN2SOCKS_VERSION}? [y/N] " answer
    if [[ ! "$answer" =~ ^[Yy]$ ]]; then
        info "Skipping tun2socks"
        return
    fi

    info "Installing tun2socks via go install..."
    go install "github.com/xjasonlyu/tun2socks/v2@${TUN2SOCKS_VERSION}"

    local gobin="${GOPATH:-$HOME/go}/bin"
    if [ -f "$gobin/tun2socks" ]; then
        sudo install -Dm755 "$gobin/tun2socks" /usr/local/bin/tun2socks
        info "tun2socks installed"
    elif [ -f "$HOME/go/bin/tun2socks" ]; then
        sudo install -Dm755 "$HOME/go/bin/tun2socks" /usr/local/bin/tun2socks
        info "tun2socks installed"
    else
        warn "tun2socks not found — build may have failed."
        warn "Try manually: go install github.com/xjasonlyu/tun2socks/v2@${TUN2SOCKS_VERSION}"
    fi
}

# --- install hysteria2 (optional, for hysteria2:// subscriptions) -------------
install_hysteria2() {
    step "Step 8/8: Install hysteria2 client (optional — hysteria2:// profiles)"

    if command -v hysteria &>/dev/null; then
        info "hysteria already installed: $(hysteria version 2>&1 | head -1)"
        return
    fi

    # xray-core does NOT implement the hysteria2 protocol. Subscriptions
    # containing hysteria2:// / hy2:// URIs need the official hysteria2 client
    # binary from apernet/hysteria2. Without it, connect attempts fail silently.
    read -rp "    Install hysteria2 ${HYSTERIA_VERSION}? [y/N] " answer
    if [[ ! "$answer" =~ ^[Yy]$ ]]; then
        info "Skipping hysteria2"
        warn "hysteria2:// / hy2:// profiles will not work without the hysteria binary."
        warn "Install manually: go install github.com/apernet/hysteria2/v2@${HYSTERIA_VERSION}"
        return
    fi

    info "Installing hysteria2 via go install..."
    go install "github.com/apernet/hysteria2/v2@${HYSTERIA_VERSION}"

    local gobin="${GOPATH:-$HOME/go}/bin"
    if [ -f "$gobin/hysteria" ]; then
        sudo install -Dm755 "$gobin/hysteria" /usr/local/bin/hysteria
        info "hysteria2 installed: $(hysteria version 2>&1 | head -1)"
    elif [ -f "$HOME/go/bin/hysteria" ]; then
        sudo install -Dm755 "$HOME/go/bin/hysteria" /usr/local/bin/hysteria
        info "hysteria2 installed"
    else
        warn "hysteria2 binary not found in GOPATH."
        warn "Install manually: go install github.com/apernet/hysteria2/v2@${HYSTERIA_VERSION}"
        warn "Then copy: sudo install -Dm755 ~/go/bin/hysteria /usr/local/bin/hysteria"
    fi
}

# --- final summary -----------------------------------------------------------
print_final_message() {
    echo
    echo -e "${GREEN}============================================${NC}"
    echo -e "${GREEN}  WhoisThat ${mode,,}ed successfully!${NC}"
    echo -e "${GREEN}============================================${NC}"
    echo
    echo "  Usage:"
    echo -e "    ${YELLOW}whoisthat${NC}            — normal mode"
    echo -e "    ${YELLOW}sudo -E whoisthat${NC}    — TUN mode (system-wide VPN)"
    echo
    echo "  Config & data:"
    echo "    ~/.config/whoisthat/        — core and TUI config"
    echo "    ~/.local/share/whoisthat/db/ — profile database"
    echo
    echo "  Key bindings:"
    echo "    j/k/↑/↓  — navigate"
    echo "    U         — add group (subscription)"
    echo "    u         — update subscription"
    echo "    e         — edit group"
    echo "    c/Enter   — connect"
    echo "    d         — disconnect"
    echo "    X         — delete group"
    echo "    h         — help (all keys)"
    echo "    q         — detach (VPN keeps running in background)"
    echo "    Q/Ctrl+C  — full quit (stop VPN + exit)"
    echo
    echo "  Clean up the build directory:"
    echo -e "    ${YELLOW}rm -rf ${BUILD_DIR}${NC}"
    echo

    # Suggest PATH reload if Go or Cargo were freshly installed.
    # We check the profile file because PATH is already set in-script.
    if ! grep -q '/usr/local/go/bin' "$HOME/.profile" 2>/dev/null; then
        echo -e "  ${YELLOW}[!] Restart your terminal or run:${NC}"
        echo "      source ~/.profile"
        if [ -f "$HOME/.cargo/env" ]; then
            echo "      source ~/.cargo/env"
        fi
        echo
    fi
}

# =============================================================================
# Main
# =============================================================================
main() {
    local mode="Install"
    if command -v whoisthat &>/dev/null || [ -f /usr/local/bin/whoisthat ]; then
        mode="Upgrade"
        local cur
        cur=$(whoisthat --version 2>/dev/null || echo "unknown")
        info "whoisthat ${cur} detected — will upgrade to latest release"
    fi

    echo
    echo -e "${CYAN}  WhoisThat — Universal ${mode}er${NC}"
    echo -e "${CYAN}  ==================================${NC}"
    echo

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

main "$@"
