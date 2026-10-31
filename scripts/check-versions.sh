#!/usr/bin/env bash
#
# Verify — and optionally update — every copy of a version in the tree.
# No build tooling is required: everything is parsed from the files.
#
# Sources of truth:
#   TUI version      Cargo.toml
#   Core version     core/core/lib/version/version.go  (CoreVersion)
#   Protocol version core/core/lib/version/protocol.txt
#   Xray pin         core/core/lib/xraymgr/version.go   (PinnedVersion)
# Every mirror (Cargo.lock, package.json, package-lock.json, PKGBUILD,
# .SRCINFO, install.sh, protocol.rs, docs) is checked against them.
#
# Usage:
#   ./scripts/check-versions.sh
#       Check that all mirrors agree. Exit 1 on any mismatch.
#
#   ./scripts/check-versions.sh --tag v0.16.0
#       Also verify that a release tag matches its component version
#       (v* -> TUI, core-v* -> core). On GitHub Actions the tag is picked up
#       automatically from GITHUB_REF_TYPE/GITHUB_REF_NAME.
#
#   ./scripts/check-versions.sh --set tui=0.16.0 core=0.4.0 protocol=4
#       Rewrite every mirror, then re-run the checks. protocol is required,
#       pkgrel defaults to 1 when tui changes (otherwise it is kept).
#
#   ./scripts/check-versions.sh --set tui=0.16.0 core=0.4.0 protocol=4 --dry-run
#       Print the diff of every file that would change, write nothing.
#
#   ./scripts/check-versions.sh --set tui=0.16.0 core=0.4.0 protocol=4 --tag v0.16.0
#       Write, verify, and confirm the tag in one go.
#
# Xray's PinnedVersion is updated by hand: it changes too rarely to automate.
# The script writes files atomically, aborts on invalid or unreadable input,
# and refuses to write empty content.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

fail=0
changed=0
DRY=0
TAG=""
set_seen=0
pairs=()

ok()  { printf '  ok    %s\n' "$*"; }
bad() { printf '  FAIL  %s\n' "$*" >&2; fail=1; }

usage() {
    cat <<'EOF'
Verify — and optionally update — every copy of a version in the tree.

Usage:
  ./scripts/check-versions.sh
  ./scripts/check-versions.sh --tag v0.15.1
  ./scripts/check-versions.sh --set tui=0.16.0 core=0.4.0 protocol=4 \
      [pkgrel=1] [--dry-run] [--tag v0.16.0]

--set keys: tui, core, protocol (required), pkgrel (default 1 when tui changes).
--set rewrites every mirror and then re-runs the checks, so a missed mirror
cannot slip through. --dry-run only prints the diff.
Xray's PinnedVersion is updated by hand (xraymgr/version.go, install.sh, docs).
EOF
}

semver() { printf '%s' "$1" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; }
uint()   { printf '%s' "$1" | grep -Eq '^[0-9]+$'; }

while [ $# -gt 0 ]; do
    case "$1" in
        --set)
            set_seen=1
            shift
            while [ $# -gt 0 ] && [ "${1#-}" = "$1" ]; do
                case "$1" in
                    *=*) pairs+=("$1") ;;
                    *)   bad "expected key=value after --set, got: $1" ;;
                esac
                shift
            done
            ;;
        --dry-run)
            DRY=1
            shift
            ;;
        --tag)
            shift
            if [ $# -gt 0 ]; then
                TAG="$1"
                shift
            fi
            ;;
        -h|--help)
            usage
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

read_sources() {
    tui=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n1)
    core=$(sed -n 's/^const CoreVersion = "\([^"]*\)"$/\1/p' core/core/lib/version/version.go | head -n1)
    protocol=$(tr -d '[:space:]' < core/core/lib/version/protocol.txt)
    xray=$(sed -n 's/^const PinnedVersion = "\([^"]*\)"$/\1/p' core/core/lib/xraymgr/version.go | head -n1)
}

# --- writing -----------------------------------------------------------------

# commit FILE TMP: show the diff (dry run) or atomically replace FILE.
commit() {
    local file=$1 out=$2
    if cmp -s "$file" "$out"; then
        rm -f "$out"
        return 0
    fi
    if [ ! -s "$out" ]; then
        bad "refusing to write empty content to $file"
        rm -f "$out"
        return
    fi
    if [ "$DRY" -eq 1 ]; then
        printf '  would update %s\n' "$file"
        diff -u "$file" "$out" | sed -n '3,$p' | sed 's/^/    /' || true
        rm -f "$out"
    else
        chmod --reference="$file" "$out" 2>/dev/null || chmod 644 "$out"
        mv -f "$out" "$file"
        printf '  updated %s\n' "$file"
    fi
    changed=$((changed + 1))
}

# transform FILE COMMAND...: feed FILE to COMMAND on stdin, then commit.
transform() {
    local file=$1
    shift
    if [ ! -f "$file" ]; then
        bad "missing file: $file"
        return
    fi
    local out
    out=$(mktemp) || { bad "mktemp failed"; return; }
    if "$@" > "$out" < "$file"; then
        commit "$file" "$out"
    else
        bad "transform failed on $file"
        rm -f "$out"
    fi
}

# Version tokens need placeholders: replacing v<old-tui> could otherwise
# produce a string that the v<old-core> pattern rewrites in the same pass.
transform_docs() {
    local file=$1
    if [ ! -f "$file" ]; then
        bad "missing file: $file"
        return
    fi
    local a b
    a=$(mktemp) || { bad "mktemp failed"; return; }
    b=$(mktemp) || { bad "mktemp failed"; rm -f "$a"; return; }
    if sed -E \
            -e "s/\\bv${tui_rx}\\b/@@WIT_TUI@@/g" \
            -e "s/\\bv${core_rx}\\b/@@WIT_CORE@@/g" \
            -e "s/(\"client_version\"[[:space:]]*:[[:space:]]*\")${tui_rx}\"/\\1@@WIT_TUI_RAW@@\"/g" \
            -e "s/(\"core_version\"[[:space:]]*:[[:space:]]*\")${core_rx}\"/\\1@@WIT_CORE_RAW@@\"/g" \
            -e "s/(\"version\"[[:space:]]*:[[:space:]]*\")${tui_rx}\"/\\1@@WIT_TUI_RAW@@\"/g" \
            -e "s/(core_version = \")${core_rx}\"/\\1${new_core}\"/g" \
            -e "s/(\"protocol_version\"[[:space:]]*:[[:space:]]*)${protocol}\\b/\\1${new_protocol}/g" \
            -e "s/(\"core_protocol_version\"[[:space:]]*:[[:space:]]*)${protocol}\\b/\\1${new_protocol}/g" \
            -e "s/(\\(protocol )${protocol}\\b/\\1${new_protocol}/g" \
            -e "s/(\\*\\*Protocol Version\\*\\*: \`)${protocol}\\b/\\1${new_protocol}/g" \
            "$file" > "$a" &&
        sed -E \
            -e "s/@@WIT_TUI_RAW@@/${new_tui}/g" \
            -e "s/@@WIT_CORE_RAW@@/${new_core}/g" \
            -e "s/@@WIT_TUI@@/v${new_tui}/g" \
            -e "s/@@WIT_CORE@@/v${new_core}/g" \
            "$a" > "$b"; then
        commit "$file" "$b"
    else
        bad "docs transform failed on $file"
        rm -f "$b"
    fi
    rm -f "$a"
}

apply_set() {
    local new_tui="" new_core="" new_protocol="" new_pkgrel="" pair key val
    local old_pkgrel tui_rx core_rx f

    if [ "${#pairs[@]}" -eq 0 ]; then
        bad "--set needs tui=X core=Y protocol=N [pkgrel=R]"
        usage >&2
        return
    fi
    for pair in "${pairs[@]}"; do
        key=${pair%%=*}
        val=${pair#*=}
        case "$key" in
            tui)      new_tui=$val ;;
            core)     new_core=$val ;;
            protocol) new_protocol=$val ;;
            pkgrel)   new_pkgrel=$val ;;
            *) bad "unknown --set key '$key' (expected tui, core, protocol, pkgrel)" ;;
        esac
    done

    [ -n "$new_tui" ]      || bad "--set needs tui=X"
    [ -n "$new_core" ]     || bad "--set needs core=Y"
    [ -n "$new_protocol" ] || bad "--set needs protocol=N"
    if [ -n "$new_tui" ] && ! semver "$new_tui"; then
        bad "tui must be X.Y.Z, got '$new_tui'"
    fi
    if [ -n "$new_core" ] && ! semver "$new_core"; then
        bad "core must be X.Y.Z, got '$new_core'"
    fi
    if [ -n "$new_protocol" ]; then
        if ! uint "$new_protocol"; then
            bad "protocol must be an integer, got '$new_protocol'"
        elif [ "$new_protocol" -lt 1 ]; then
            bad "protocol must be >= 1"
        fi
    fi
    if [ -n "$new_pkgrel" ]; then
        if ! uint "$new_pkgrel"; then
            bad "pkgrel must be an integer, got '$new_pkgrel'"
        elif [ "$new_pkgrel" -lt 1 ]; then
            bad "pkgrel must be >= 1"
        fi
    fi
    if [ -z "$tui" ] || [ -z "$core" ] || [ -z "$protocol" ]; then
        bad "cannot read current versions; refusing to write"
        return
    fi
    [ "$fail" -eq 0 ] || return

    old_pkgrel=$(sed -n 's/^pkgrel=//p' pkg/aur/PKGBUILD)
    if [ -z "$new_pkgrel" ]; then
        # A new upstream version always starts at pkgrel 1.
        if [ "$new_tui" = "$tui" ]; then new_pkgrel=$old_pkgrel; else new_pkgrel=1; fi
    fi
    if [ -z "$new_pkgrel" ]; then
        bad "cannot read current pkgrel"
        return
    fi

    tui_rx=${tui//./\\.}
    core_rx=${core//./\\.}

    printf '\n%s tui=%s core=%s protocol=%s pkgrel=%s\n' \
        "$([ "$DRY" -eq 1 ] && printf 'Would set' || printf 'Setting')" \
        "$new_tui" "$new_core" "$new_protocol" "$new_pkgrel"

    transform Cargo.toml sed -E \
        -e "s/^version = \"${tui_rx}\"$/version = \"${new_tui}\"/"

    # Cargo.lock: only the root [[package]] entry, never a dependency.
    transform Cargo.lock sed -E \
        -e "/^name = \"whoisthat\"$/{n;s/^version = \"[^\"]*\"$/version = \"${new_tui}\"/;}"

    transform package.json sed -E \
        -e "s/^  \"version\": \"[^\"]*\",$/  \"version\": \"${new_tui}\",/"

    # package-lock.json: the first two "version" entries are the root package.
    # shellcheck disable=SC2016  # awk vars, not shell expressions
    transform package-lock.json awk -v old="$tui" -v new="$new_tui" '
        n < 2 && index($0, "\"version\": \"" old "\"") {
            sub("\"version\": \"" old "\"", "\"version\": \"" new "\"")
            n++
        }
        { print }
    '

    transform pkg/aur/PKGBUILD sed -E \
        -e "s/^pkgver=.*/pkgver=${new_tui}/" \
        -e "s/^pkgrel=.*/pkgrel=${new_pkgrel}/"

    transform pkg/aur/.SRCINFO sed -E \
        -e "s/^([[:space:]]*)pkgver = .*/\1pkgver = ${new_tui}/" \
        -e "s/^([[:space:]]*)pkgrel = .*/\1pkgrel = ${new_pkgrel}/" \
        -e "s|^([[:space:]]*)source = .*|\1source = whoisthat-${new_tui}.tar.gz::https://github.com/kvunoff/whoisthat/archive/refs/tags/v${new_tui}.tar.gz|"

    transform core/core/lib/version/version.go sed -E \
        -e "s/^const CoreVersion = \"[^\"]*\"$/const CoreVersion = \"${new_core}\"/"

    transform core/core/lib/version/protocol.txt printf '%s\n' "$new_protocol"

    transform src/core_client/protocol.rs sed -E \
        -e "s/^(pub const EXPECTED_CORE_PROTOCOL_VERSION: u32 = )[0-9]+;/\1${new_protocol};/"

    while IFS= read -r f; do
        transform_docs "$f"
    done < <(find docs README.md -name '*.md' -type f 2>/dev/null)

    if [ "$DRY" -eq 1 ]; then
        printf '\nDry run: %d file(s) would change; post-check skipped.\n' "$changed"
    else
        printf '\nUpdated %d file(s).\n' "$changed"
    fi
}

# --- checks ------------------------------------------------------------------

run_checks() {
    read_sources

    local pair name value
    for pair in "Cargo.toml:$tui" "CoreVersion:$core" "protocol.txt:$protocol" "PinnedVersion:$xray"; do
        name=${pair%%:*}
        value=${pair#*:}
        if [ -z "$value" ]; then
            bad "cannot parse $name"
        else
            ok "source of truth: $name = $value"
        fi
    done

    if [ -z "$tui" ] || [ -z "$core" ] || [ -z "$protocol" ] || [ -z "$xray" ]; then
        bad "sources of truth are unreadable"
        return
    fi

    local lock docs_pkg npm_lock pkgver pkgrel srcver srcrel srcsource
    lock=$(awk '/^name = "whoisthat"$/ { getline; if ($1 == "version") { gsub(/"/, "", $3); print $3; exit } }' Cargo.lock)
    docs_pkg=$(sed -n 's/.*"version": "\([^"]*\)".*/\1/p' package.json | head -n1)
    npm_lock=$(sed -n 's/^  "version": "\([^"]*\)".*/\1/p' package-lock.json | head -n1)
    pkgver=$(sed -n 's/^pkgver=//p' pkg/aur/PKGBUILD)
    pkgrel=$(sed -n 's/^pkgrel=//p' pkg/aur/PKGBUILD)
    srcver=$(sed -n 's/^\tpkgver = //p' pkg/aur/.SRCINFO | head -n1)
    srcrel=$(sed -n 's/^\tpkgrel = //p' pkg/aur/.SRCINFO | head -n1)
    srcsource=$(sed -n 's/^\tsource = //p' pkg/aur/.SRCINFO | head -n1)

    check_eq "Cargo.lock"              "$tui" "$lock"
    check_eq "package.json"            "$tui" "$docs_pkg"
    check_eq "package-lock.json"       "$tui" "$npm_lock"
    check_eq "PKGBUILD pkgver"         "$tui" "$pkgver"
    check_eq ".SRCINFO pkgver"         "$pkgver" "$srcver"
    check_eq ".SRCINFO pkgrel"         "$pkgrel" "$srcrel"

    local expected_source="whoisthat-${pkgver}.tar.gz::https://github.com/kvunoff/whoisthat/archive/refs/tags/v${pkgver}.tar.gz"
    check_eq ".SRCINFO source" "$expected_source" "$srcsource"

    local install_xray rust_proto
    install_xray=$(sed -n 's/^XRAY_VERSION="\([^"]*\)"$/\1/p' install.sh | head -n1)
    check_eq "install.sh XRAY_VERSION" "$xray" "$install_xray"

    # The doctor duplicates the pin; keep it from silently drifting.
    local rust_xray
    rust_xray=$(sed -n 's/^const PINNED_XRAY_VERSION: &str = "\([^"]*\)";$/\1/p' src/doctor/checks.rs | head -n1)
    check_eq "doctor PINNED_XRAY_VERSION" "${xray#v}" "$rust_xray"

    rust_proto=$(sed -n 's/^pub const EXPECTED_CORE_PROTOCOL_VERSION: u32 = \([0-9]*\);$/\1/p' src/core_client/protocol.rs)
    check_eq "Rust EXPECTED_CORE_PROTOCOL_VERSION" "$protocol" "$rust_proto"

    # Every `vX.Y.Z` token in the shipped markdown must be one of the three
    # versions this project publishes (TUI, core, pinned Xray). This catches
    # stale examples without hardcoding line numbers.
    local allowed unexpected token
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

    check_json_field client_version "$tui"
    check_json_field version        "$tui"
    check_json_field core_version   "$core"

    local toml_core
    while IFS= read -r toml_core; do
        check_eq "docs core_version (toml)" "$core" "$toml_core"
    done < <(grep -rhoE 'core_version = "[0-9]+\.[0-9]+\.[0-9]+"' docs --include='*.md' 2>/dev/null |
        sed -E 's/.*"([^"]+)"/\1/' | sort -u)

    check_docs_protocol "docs protocol_version"      '"protocol_version"[[:space:]]*:[[:space:]]*[0-9]+'
    check_docs_protocol "docs core_protocol_version" '"core_protocol_version"[[:space:]]*:[[:space:]]*[0-9]+'
    check_docs_protocol "docs (protocol N)"          '\(protocol [0-9]+\)'
    # shellcheck disable=SC2016  # literal backticks in the markdown pattern
    check_docs_protocol "docs Protocol Version"      '\*\*Protocol Version\*\*: `[0-9]+`'

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
}

check_eq() {
    local label=$1 expected=$2 actual=$3
    if [ "$actual" = "$expected" ]; then
        ok "$label = $actual"
    else
        bad "$label = ${actual:-<missing>} (expected $expected)"
    fi
}

check_json_field() {
    local field=$1 expected=$2 value
    while IFS= read -r value; do
        check_eq "docs $field" "$expected" "$value"
    done < <(grep -rhoE "\"${field}\": \"[0-9]+\.[0-9]+\.[0-9]+\"" docs --include='*.md' 2>/dev/null |
        sed -E 's/.*: "([^"]+)"/\1/' | sort -u)
}

check_docs_protocol() {
    local label=$1 pattern=$2 value
    while IFS= read -r value; do
        check_eq "$label" "$protocol" "$value"
    done < <(grep -rhoE "$pattern" docs --include='*.md' 2>/dev/null |
        grep -oE '[0-9]+' | sort -u)
}

# --- main --------------------------------------------------------------------

if [ "$set_seen" -eq 1 ]; then
    read_sources
    if [ -z "$tui" ] || [ -z "$core" ] || [ -z "$protocol" ]; then
        bad "cannot read current versions; refusing to write"
        exit 1
    fi
    apply_set
    [ "$fail" -eq 0 ] || exit 1
    [ "$DRY" -eq 1 ] && exit 0
    printf '\nVerifying...\n'
fi

run_checks

echo
if [ "$fail" -ne 0 ]; then
    echo "Version check FAILED." >&2
    exit 1
fi
echo "Version check passed."
