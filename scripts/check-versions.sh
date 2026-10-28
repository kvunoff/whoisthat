#!/usr/bin/env bash
#
# Verify that every copy of a version in the tree agrees with its source of
# truth. No build tooling is required: everything is parsed from the files.
#
# Usage:
#   ./scripts/check-versions.sh                 # consistency checks
#   ./scripts/check-versions.sh --tag v0.14.3   # also verify a release tag
#
# On GitHub Actions tag builds the tag is picked up automatically from
# GITHUB_REF_TYPE/GITHUB_REF_NAME, so the same script can gate AUR publishing.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

fail=0
ok()  { printf '  ok    %s\n' "$*"; }
bad() { printf '  FAIL  %s\n' "$*" >&2; fail=1; }

TAG=""
while [ $# -gt 0 ]; do
    case "$1" in
        --tag)
            TAG="${2:-}"
            shift 2
            ;;
        -h|--help)
            sed -n '2,12p' "$0"
            exit 0
            ;;
        *)
            bad "unknown argument: $1"
            shift
            ;;
    esac
done
if [ -z "$TAG" ] && [ "${GITHUB_REF_TYPE:-}" = "tag" ]; then
    TAG="${GITHUB_REF_NAME:-}"
fi

# --- sources of truth -------------------------------------------------------

tui=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n1)
core=$(sed -n 's/^const CoreVersion = "\([^"]*\)"$/\1/p' core/core/lib/version/version.go | head -n1)
protocol=$(tr -d '[:space:]' < core/core/lib/version/protocol.txt)
xray=$(sed -n 's/^const PinnedVersion = "\([^"]*\)"$/\1/p' core/core/lib/xraymgr/version.go | head -n1)

for pair in "Cargo.toml:$tui" "CoreVersion:$core" "protocol.txt:$protocol" "PinnedVersion:$xray"; do
    name=${pair%%:*}
    value=${pair#*:}
    if [ -z "$value" ]; then
        bad "cannot parse $name"
    else
        ok "source of truth: $name = $value"
    fi
done

[ -n "$tui" ] && [ -n "$core" ] && [ -n "$protocol" ] && [ -n "$xray" ] || {
    echo "Aborting: sources of truth are unreadable." >&2
    exit 1
}

# --- TUI version mirrors ----------------------------------------------------

check_eq() {
    local label=$1 expected=$2 actual=$3
    if [ "$actual" = "$expected" ]; then
        ok "$label = $actual"
    else
        bad "$label = ${actual:-<missing>} (expected $expected)"
    fi
}

lock=$(awk '/^name = "whoisthat"$/ { getline; if ($1 == "version") { gsub(/"/, "", $3); print $3; exit } }' Cargo.lock)
docs_pkg=$(sed -n 's/.*"version": "\([^"]*\)".*/\1/p' package.json | head -n1)
pkgver=$(sed -n 's/^pkgver=//p' pkg/aur/PKGBUILD)
pkgrel=$(sed -n 's/^pkgrel=//p' pkg/aur/PKGBUILD)
srcver=$(sed -n 's/^\tpkgver = //p' pkg/aur/.SRCINFO | head -n1)
srcrel=$(sed -n 's/^\tpkgrel = //p' pkg/aur/.SRCINFO | head -n1)
srcsource=$(sed -n 's/^\tsource = //p' pkg/aur/.SRCINFO | head -n1)

check_eq "Cargo.lock"              "$tui" "$lock"
check_eq "package.json"            "$tui" "$docs_pkg"
check_eq "PKGBUILD pkgver"         "$tui" "$pkgver"
check_eq ".SRCINFO pkgver"         "$pkgver" "$srcver"
check_eq ".SRCINFO pkgrel"         "$pkgrel" "$srcrel"

expected_source="whoisthat-${pkgver}.tar.gz::https://github.com/kvunoff/whoisthat/archive/refs/tags/v${pkgver}.tar.gz"
check_eq ".SRCINFO source" "$expected_source" "$srcsource"

# --- cross-component pins ---------------------------------------------------

install_xray=$(sed -n 's/^XRAY_VERSION="\([^"]*\)"$/\1/p' install.sh | head -n1)
check_eq "install.sh XRAY_VERSION" "$xray" "$install_xray"

rust_proto=$(sed -n 's/^pub const EXPECTED_CORE_PROTOCOL_VERSION: u32 = \([0-9]*\);$/\1/p' src/core_client/protocol.rs)
check_eq "Rust EXPECTED_CORE_PROTOCOL_VERSION" "$protocol" "$rust_proto"

# --- documentation mirrors --------------------------------------------------

# Every `vX.Y.Z` token in the shipped markdown must be one of the three
# versions this project publishes (TUI, core, pinned Xray). This catches stale
# examples without hardcoding line numbers.
allowed=" v${tui} v${core} ${xray} "
unexpected=$(
    grep -rhoE 'v[0-9]+\.[0-9]+\.[0-9]+' docs README.md --include='*.md' 2>/dev/null |
        sort -u |
        while IFS= read -r token; do
            case "$allowed" in
                *" $token "*) ;;
                *) printf '%s\n' "$token" ;;
            esac
        done
)
if [ -z "$unexpected" ]; then
    ok "docs version tokens match $tui / $core / $xray"
else
    while IFS= read -r token; do
        [ -n "$token" ] && bad "docs mention unexpected version $token (allowed:${allowed% })"
    done <<< "$unexpected"
fi

check_json_field() {
    local field=$1 expected=$2 value
    for value in $(grep -rhoE "\"${field}\": \"[0-9]+\.[0-9]+\.[0-9]+\"" docs --include='*.md' 2>/dev/null |
        sed -E 's/.*: "([^"]+)"/\1/' | sort -u); do
        check_eq "docs $field" "$expected" "$value"
    done
}

check_json_field client_version "$tui"
check_json_field core_version   "$core"

# --- release tag ------------------------------------------------------------

if [ -n "$TAG" ]; then
    case "$TAG" in
        core-v*)
            check_eq "tag $TAG" "$core" "${TAG#core-v}"
            ;;
        v*)
            check_eq "tag $TAG" "$tui" "${TAG#v}"
            ;;
        *)
            ok "tag $TAG does not carry a component version"
            ;;
    esac
else
    ok "no release tag to check (pass --tag or set GITHUB_REF_NAME)"
fi

echo
if [ "$fail" -ne 0 ]; then
    echo "Version check FAILED." >&2
    exit 1
fi
echo "Version check passed."
