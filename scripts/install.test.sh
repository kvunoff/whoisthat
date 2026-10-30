#!/usr/bin/env bash
#
# Offline tests for install.sh: pure functions, argument parsing, the package
# manager dispatch and as_target. Nothing here installs packages, downloads
# files or touches /usr/local.

set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd) || exit 1

fail=0
pass() { printf '  ok    %s\n' "$*"; }
bad() { printf '  FAIL  %s\n' "$*" >&2; fail=1; }
assert_eq() {
    if [ "$1" = "$2" ]; then pass "$3"; else bad "$3: '$1' != '$2'"; fi
}
assert_contains() {
    case "$1" in
        *"$2"*) pass "$3" ;;
        *) bad "$3 (missing '$2')" ;;
    esac
}
assert_empty() {
    if [ -z "$1" ]; then pass "$2"; else bad "$2 (unexpected '$1')"; fi
}

# install.sh enables errexit; keep the test runner going after a failed check.
# shellcheck source=/dev/null
source "$ROOT/install.sh"
set +eu
set +o pipefail

echo "== versions"
assert_eq "$(version_ge 1.26.8 1.26.0 && echo yes)" "yes" "1.26.8 >= 1.26.0"
assert_eq "$(version_ge 1.25.0 1.26.0 || echo no)" "no" "1.25.0 < 1.26.0"
assert_eq "$(version_ge 1.79 1.80 || echo no)" "no" "1.79 < 1.80"
assert_eq "$(version_ge 1.80 1.79 && echo yes)" "yes" "1.80 >= 1.79"

echo "== xray version parsing"
assert_eq "$(xray_version 'Xray 26.9.9 (Xray, Penetration Test) [linux/amd64]')" "26.9.9" "plain"
assert_eq "$(xray_version 'Xray v26.9.9 (Xray)')" "26.9.9" "leading v"
assert_eq "$(xray_version 'Xray 26.9.90 (Xray)')" "26.9.90" "no substring match"
assert_eq "$(xray_version 'Xray 126.9.9 (Xray)')" "126.9.9" "no prefix match"
assert_empty "$(xray_version '')" "empty output"

echo "== release tag selection"
refs='aaaa refs/tags/v0.15.0
bbbb refs/tags/v0.15.1
cccc refs/tags/v0.16.0-rc1
dddd refs/tags/core-v0.3.8
eeee refs/tags/v0.9.9
ffff refs/tags/v0.15.1^{}'
assert_eq "$(printf '%s\n' "$refs" | latest_release_tag)" "v0.15.1" "stable tag wins over rc and core-v"

echo "== argument parsing"
assert_eq "$( (parse_args --branch v1.2.3; printf '%s' "$TARGET_BRANCH") )" "v1.2.3" "--branch sets tag"
assert_eq "$( (parse_args --local; printf '%s' "$BUILD_LOCAL") )" "true" "--local"
assert_eq "$( (parse_args --uninstall; printf '%s' "$UNINSTALL_MODE") )" "true" "--uninstall"
assert_eq "$( (parse_args --yes; printf '%s' "$ASSUME_YES") )" "true" "--yes accepted"
assert_eq "$( (parse_args --bogus >/dev/null 2>&1); printf '%s' "$?" )" "2" "unknown option exits 2"
assert_eq "$( (parse_args --branch >/dev/null 2>&1); printf '%s' "$?" )" "1" "--branch without value exits 1"
assert_eq "$( (parse_args --help >/dev/null 2>&1); printf '%s' "$?" )" "0" "--help exits 0"

echo "== package manager dispatch"
out=$( DISTRO_ID=debian DISTRO_LIKE="" SUDO="echo" install_system_deps )
assert_contains "$out" "iproute2 nftables" "debian packages"
out=$( DISTRO_ID=fedora DISTRO_LIKE="" SUDO="echo" install_system_deps )
assert_contains "$out" "iproute nftables" "fedora uses iproute"
out=$( DISTRO_ID=alpine DISTRO_LIKE="" SUDO="echo" install_system_deps )
assert_contains "$out" "iproute2 nftables" "alpine packages"
out=$( DISTRO_ID=neon DISTRO_LIKE="ubuntu debian" SUDO="echo" install_system_deps )
assert_contains "$out" "DEBIAN_FRONTEND=noninteractive" "ID_LIKE fallback is non-interactive"
out=$( DISTRO_ID=mystery DISTRO_LIKE="" SUDO="echo" install_system_deps )
assert_contains "$out" "iproute2" "unknown distro hint mentions iproute2"

echo "== as_target"
assert_eq "$(as_target id -u)" "$(id -u)" "non-root runs directly"
out=$(
    id() {
        case "$*" in
            "-u") echo 0 ;;
            "-un") echo root ;;
            "-u alice") echo 4242 ;;
            *) command id "$@" ;;
        esac
    }
    runuser() { echo "runuser $*"; }
    # shellcheck disable=SC2034  # read by as_target
    TARGET_USER=alice
    # shellcheck disable=SC2034
    TARGET_HOME=/home/alice
    # shellcheck disable=SC2034
    SUDO=""
    as_target echo hi
)
assert_contains "$out" "HOME=/home/alice" "root path sets HOME"
assert_contains "$out" "PATH=/home/alice/.cargo/bin" "root path sets PATH"
assert_contains "$out" "runuser -u alice" "root path uses runuser"
case "$out" in
    *XDG_RUNTIME_DIR=*) bad "XDG_RUNTIME_DIR must be absent without /run/user/4242" ;;
    *) pass "XDG_RUNTIME_DIR omitted when /run/user/<uid> is missing" ;;
esac

echo "== package ownership and cleanup"
assert_eq "$(package_owns "$(command -v env)" && echo yes || echo no)" "yes" "detects distro-owned file"
assert_eq "$(package_owns /tmp/definitely-not-a-package-file 2>/dev/null && echo yes || echo no)" "no" "rejects unowned path"
tmp_marker=$(mktemp -d)
# shellcheck disable=SC2034  # read by cleanup
WORK_DIR="$tmp_marker"
( cleanup >/dev/null 2>&1 )
assert_eq "$([ -d "$tmp_marker" ] && echo present || echo removed)" "removed" "cleanup removes WORK_DIR"

echo "== dead code"
dead=$(grep -nE 'prompt_yes_no|NONINTERACTIVE|INSTALL_TUN|T2S_ARCH' "$ROOT/install.sh" || true)
assert_empty "$dead" "no removed identifiers left in install.sh"

if [ "$fail" -ne 0 ]; then
    echo "FAILED" >&2
    exit 1
fi
echo "all install.sh tests passed"
