#!/usr/bin/env bash
# Shared helpers for scripts/package.sh targets. Sourced, never executed.
#
# A target file defines prepare/build/publish/verify and uses these helpers.
# Keep every target's CLI and log format identical so AUR, PPA and (later)
# Fedora differ only in their four functions.

# shellcheck disable=SC2034  # read by package.sh and the target scripts
PUBLISH=0
DRY_RUN=0
ARTIFACT_DIR="$ROOT/dist"
WORKDIR=""
# shellcheck disable=SC2034  # read by package/ppa.sh
SERIES="noble,resolute"
TAG=""
VERSION=""
PKGREL=""
# shellcheck disable=SC2034  # read by package/ppa.sh
KEYID=""
# shellcheck disable=SC2034  # read by package/ppa.sh
GPG_SIGN_CMD=""
LAUNCHPAD_USER="${LAUNCHPAD_USER:-kvunoff}"
LAUNCHPAD_PPA="${LAUNCHPAD_PPA:-whoisthat}"
# SHA256 fingerprint of ppa.launchpad.net's RSA host key.
LAUNCHPAD_HOST_FP="SHA256:MGq+4hxD7RduVTcfwlwwboZnsgJC6SL/NltM8ye+gNg"

usage() {
    cat <<'EOF'
Usage: scripts/package.sh <target> [options]

Targets: aur, aur-bin, ppa

Options:
  --tag vX.Y.Z       release tag (default: version from Cargo.toml)
  --publish          push/upload (default: build only)
  --dry-run          print the plan and do not publish
  --artifacts DIR    release artifacts directory (default: dist)
  --workdir DIR      keep generated files in DIR
  --series LIST      PPA series, comma-separated (default: noble,resolute)
  -h, --help
EOF
}

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

log() {
    printf '[%s] %s\n' "$TARGET" "$*"
}

require_cmd() {
    local cmd
    for cmd in "$@"; do
        command -v "$cmd" >/dev/null 2>&1 || die "required command not found: $cmd"
    done
}

parse_args() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --tag)
                [ $# -ge 2 ] || die "--tag needs a value"
                TAG=$2
                shift 2
                ;;
            --publish)
                PUBLISH=1
                shift
                ;;
            --dry-run)
                DRY_RUN=1
                shift
                ;;
            --artifacts)
                [ $# -ge 2 ] || die "--artifacts needs a value"
                ARTIFACT_DIR=$2
                shift 2
                ;;
            --workdir)
                [ $# -ge 2 ] || die "--workdir needs a value"
                WORKDIR=$2
                shift 2
                ;;
            --series)
                [ $# -ge 2 ] || die "--series needs a value"
                SERIES=$2
                shift 2
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                die "unknown option: $1"
                ;;
        esac
    done

    if [ "$DRY_RUN" = 1 ]; then
        PUBLISH=0
    fi

    if [ -z "$TAG" ]; then
        TAG="v$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$ROOT/Cargo.toml" | head -n1)"
    fi
    case "$TAG" in
        v*) ;;
        *) TAG="v$TAG" ;;
    esac
    VERSION=${TAG#v}
    printf '%s' "$VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' ||
        die "tag must look like vX.Y.Z (got '$TAG')"

    PKGREL=$(sed -n 's/^pkgrel=//p' "$ROOT/pkg/aur/PKGBUILD" | head -n1)
    printf '%s' "$PKGREL" | grep -Eq '^[0-9]+$' ||
        die "cannot read pkgrel from pkg/aur/PKGBUILD"

    case "$ARTIFACT_DIR" in
        /*) ;;
        *) ARTIFACT_DIR="$ROOT/$ARTIFACT_DIR" ;;
    esac

    if [ -z "$WORKDIR" ]; then
        WORKDIR=$(mktemp -d)
        trap 'rm -rf "$WORKDIR"' EXIT
    else
        mkdir -p "$WORKDIR"
    fi

    if ! "$ROOT/scripts/check-versions.sh" --tag "$TAG" >/dev/null; then
        "$ROOT/scripts/check-versions.sh" --tag "$TAG" >&2 || true
        die "version check failed for $TAG"
    fi
}

# setup_ssh installs one SSH key for both AUR and Launchpad SFTP into a private
# HOME, so no target depends on the caller's ~/.ssh. PACKAGING_SSH_KEY is the
# single CI secret; AUR_SSH_KEY is accepted as a legacy fallback, and a local
# ~/.ssh/ur is picked up automatically.
setup_ssh() {
    local key="" home="$WORKDIR/home" sshdir="$WORKDIR/home/.ssh"
    local original_home="${HOME:-}"

    if [ -n "${PACKAGING_SSH_KEY:-}" ]; then
        key="$sshdir/ur"
        mkdir -p "$sshdir"
        chmod 700 "$home" "$sshdir"
        printf '%s\n' "$PACKAGING_SSH_KEY" > "$key"
        chmod 600 "$key"
    elif [ -n "${AUR_SSH_KEY:-}" ]; then
        key="$sshdir/ur"
        mkdir -p "$sshdir"
        chmod 700 "$home" "$sshdir"
        printf '%s\n' "$AUR_SSH_KEY" > "$key"
        chmod 600 "$key"
    elif [ -n "$original_home" ] && [ -f "$original_home/.ssh/ur" ]; then
        key="$sshdir/ur"
        mkdir -p "$sshdir"
        chmod 700 "$home" "$sshdir"
        cp "$original_home/.ssh/ur" "$key"
        chmod 600 "$key"
    fi

    if [ -z "$key" ]; then
        if [ "$TARGET" = ppa ] && [ "$PUBLISH" = 1 ]; then
            die "no SSH key: set PACKAGING_SSH_KEY or put it at ~/.ssh/ur"
        fi
        return 0
    fi

    local known="$sshdir/known_hosts"
    : > "$known"

    # Pin the official AUR Ed25519 host key instead of trusting keyscan.
    # Fingerprint: SHA256:RFzBCUItH9LZS0cKB5UE6ceAYhBD5C8GeOBip8Z11+4
    printf '%s\n' \
        'aur.archlinux.org ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEuBKrPzbawxA/k2g6NcyV5jmqwJ2s+zpgZGZ7tpLIcN' \
        >> "$known"

    if [ "$TARGET" = ppa ] && [ "$PUBLISH" = 1 ]; then
        require_cmd ssh-keyscan ssh-keygen
        local scan="$WORKDIR/launchpad-hostkey" err="$WORKDIR/launchpad-hostkey.err"
        local fp attempt
        # Cloud runners occasionally time out a single keyscan (the default
        # connect timeout is 5s), which used to fail the whole upload. Retry
        # before giving up; the fingerprint check below still guards the key.
        for attempt in 1 2 3; do
            if ssh-keyscan -T 10 -t rsa ppa.launchpad.net >"$scan" 2>"$err"; then
                break
            fi
            if [ "$attempt" = 3 ]; then
                die "cannot fetch ppa.launchpad.net host key: $(tr '\n' ' ' <"$err")"
            fi
            sleep 2
        done
        fp=$(ssh-keygen -lf "$scan" | awk '{print $2}' | head -n1)
        [ "$fp" = "$LAUNCHPAD_HOST_FP" ] ||
            die "ppa.launchpad.net host key mismatch: $fp"
        cat "$scan" >> "$known"
    fi
    chmod 600 "$known"

    {
        printf 'Host aur.archlinux.org\n'
        printf '  HostName aur.archlinux.org\n'
        printf '  User aur\n'
        printf '  IdentityFile %s\n' "$key"
        printf '  IdentitiesOnly yes\n'
        printf '  StrictHostKeyChecking yes\n'
        printf '  UserKnownHostsFile %s\n' "$known"
        printf '\n'
        printf 'Host ppa.launchpad.net\n'
        printf '  HostName ppa.launchpad.net\n'
        printf '  User %s\n' "$LAUNCHPAD_USER"
        printf '  IdentityFile %s\n' "$key"
        printf '  IdentitiesOnly yes\n'
        printf '  StrictHostKeyChecking yes\n'
        printf '  UserKnownHostsFile %s\n' "$known"
    } > "$sshdir/config"
    chmod 600 "$sshdir/config"

    cat > "$home/.dput.cf" <<EOF
[whoisthat-ppa]
fqdn = ppa.launchpad.net
method = sftp
incoming = ~${LAUNCHPAD_USER}/ubuntu/${LAUNCHPAD_PPA}/
login = ${LAUNCHPAD_USER}
allow_unsigned_uploads = 0
EOF
    chmod 600 "$home/.dput.cf"

    # Old dput shells out to ssh for its sftp transport, so a user config is
    # ignored. Put a wrapper first in PATH that forces our private config.
    require_cmd ssh
    local ssh_bin
    ssh_bin=$(command -v ssh)
    mkdir -p "$WORKDIR/bin"
    cat > "$WORKDIR/bin/ssh" <<EOF
#!/bin/sh
exec "$ssh_bin" -F "$sshdir/config" "\$@"
EOF
    chmod 755 "$WORKDIR/bin/ssh"
    export PATH="$WORKDIR/bin:$PATH"

    export HOME="$home"
    export GIT_SSH_COMMAND="ssh -F $sshdir/config"
}

# compute_pkgrel prints the pkgrel the AUR package should get:
#   same version        -> increment the live pkgrel
#   rollback to older   -> reuse the pkgrel from the tag
#   new version         -> 1
compute_pkgrel() {
    local dir=$1 aurver aurrel
    if [ ! -f "$dir/PKGBUILD" ]; then
        printf '1\n'
        return 0
    fi
    aurver=$(sed -n 's/^pkgver=//p' "$dir/PKGBUILD" | head -n1)
    aurrel=$(sed -n 's/^pkgrel=//p' "$dir/PKGBUILD" | head -n1)
    if [ -z "$aurver" ] || [ -z "$aurrel" ]; then
        printf '1\n'
        return 0
    fi

    if [ "$aurver" = "$VERSION" ]; then
        printf '%s\n' "$((aurrel + 1))"
    elif [ "$(printf '%s\n%s\n' "$aurver" "$VERSION" | sort -V | head -n1)" = "$VERSION" ]; then
        sed -n 's/^pkgrel=//p' "$ROOT/pkg/aur/PKGBUILD" | head -n1
    else
        printf '1\n'
    fi
}

# write_srcinfo renders .SRCINFO from a PKGBUILD without makepkg (not available
# on Ubuntu runners). Arrays are discovered at runtime, so the same helper
# works for the source and binary packages.
write_srcinfo() {
    local dir=$1
    (
        cd "$dir" || exit 1
        # shellcheck disable=SC1091
        source ./PKGBUILD

        # shellcheck disable=SC2154  # variables come from PKGBUILD
        printf 'pkgbase = %s\n' "$pkgname"
        emit_scalar pkgdesc
        emit_scalar pkgver
        emit_scalar pkgrel
        emit_scalar url
        emit_scalar install
        emit_array arch
        # shellcheck disable=SC2154  # license comes from PKGBUILD
        if declare -p license >/dev/null 2>&1; then
            printf '\tlicense = %s\n' "${license[0]}"
        fi
        local field
        for field in makedepends depends optdepends provides conflicts options \
            source source_x86_64 source_aarch64 \
            sha256sums sha256sums_x86_64 sha256sums_aarch64; do
            emit_array "$field"
        done
        printf '\n'
        printf 'pkgname = %s\n' "$pkgname"
    )
}

emit_scalar() {
    local name=$1
    if declare -p "$name" >/dev/null 2>&1; then
        printf '\t%s = %s\n' "$name" "${!name}"
    fi
}

emit_array() {
    local name=$1
    declare -p "$name" >/dev/null 2>&1 || return 0
    declare -n ref="$name"
    local value
    for value in "${ref[@]}"; do
        printf '\t%s = %s\n' "$name" "$value"
    done
    unset -n ref
}

fetch() {
    local url=$1 out=$2 attempt
    for attempt in 1 2 3 4 5; do
        if curl -fsSL "$url" -o "$out"; then
            return 0
        fi
        log "archive not ready (attempt ${attempt}/5), retrying"
        sleep 10
    done
    die "failed to download $url"
}

sha256() {
    sha256sum "$1" | awk '{print $1}'
}
