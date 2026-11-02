#!/usr/bin/env bash
#
# Tests for the per-asset upload policy in .github/workflows/release.yml.
#
# The functions are extracted from the workflow and exercised with a fake `gh`
# and a local asset store, so the published-release rules are pinned without
# touching GitHub:
#   - draft release:      replaceable (upload --clobber)
#   - published release:  attach missing assets, never overwrite an existing one
#   - after a backfill:   SHA256SUMS is rebuilt from the release contents

set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd) || exit 1
TMP=$(mktemp -d "${TMPDIR:-/tmp}/release-upload-test.XXXXXX") || exit 1
trap 'rm -rf "$TMP"' EXIT

WORKFLOW="$ROOT/.github/workflows/release.yml"
STORE="$TMP/store"
UPLOAD_LOG="$TMP/uploads.log"
DRAFT=false
ATTACHED=0
mkdir -p "$STORE"
: > "$UPLOAD_LOG"

fail=0
pass() { printf '  ok    %s\n' "$*"; }
bad()  { printf '  FAIL  %s\n' "$*" >&2; fail=1; }

# Extract `name() { ... }` from the workflow's run block.
extract_fn() {
    awk -v start="          $1() {" '
        $0 == start { inside = 1 }
        inside { sub(/^          /, ""); print }
        inside && $0 == "}" { exit }
    ' "$WORKFLOW" > "$TMP/$1.sh"
    [ -s "$TMP/$1.sh" ] || { bad "could not extract $1 from $WORKFLOW"; exit 1; }
    # shellcheck source=/dev/null
    source "$TMP/$1.sh"
}

extract_fn upload_assets
extract_fn refresh_sums

# Fake gh backed by $STORE. Argument order mirrors the real calls.
gh() {
    case "${1:-} ${2:-}" in
        "release view")
            case "${5:-}" in
                isDraft) printf '%s\n' "$DRAFT" ;;
                assets)  ( cd "$STORE" && ls -1 2>/dev/null ) || true ;;
            esac
            ;;
        "release download")
            if [ "${4:-}" = "--dir" ]; then
                cp "$STORE"/* "$5"/ 2>/dev/null || true
            else
                cp "$STORE/$5" "$7/$5"
            fi
            ;;
        "release upload")
            local f
            shift 3
            for f in "$@"; do
                [ "$f" = "--clobber" ] && continue
                cp "$f" "$STORE/"
                printf '%s\n' "$(basename "$f")" >> "$UPLOAD_LOG"
            done
            ;;
        "release create")
            ;;
        *)
            return 1
            ;;
    esac
}

reset_case() {
    rm -rf "$STORE" "$TMP/dist"
    mkdir -p "$STORE" "$TMP/dist"
    : > "$UPLOAD_LOG"
    DRAFT=false
    # shellcheck disable=SC2034  # read by the extracted workflow functions
    ATTACHED=0
    printf 'alpha\n' > "$TMP/dist/a.txt"
    printf 'beta\n'  > "$TMP/dist/b.txt"
}

printf '\ndraft release\n'
reset_case
printf 'old\n' > "$STORE/a.txt"
DRAFT=true
if upload_assets v1.2.3 "$TMP/dist"; then pass "draft upload succeeds"; else bad "draft upload failed"; fi
[ "$(cat "$STORE/b.txt" 2>/dev/null)" = "beta" ] && pass "draft attaches new asset" || bad "draft did not attach b.txt"
[ "$(wc -l < "$UPLOAD_LOG")" -eq 2 ] && pass "draft uploads every asset" || bad "unexpected upload count"

printf '\npublished release, identical assets\n'
reset_case
cp "$TMP/dist"/* "$STORE/"
if upload_assets v1.2.3 "$TMP/dist"; then pass "identical assets accepted"; else bad "identical assets rejected"; fi
[ ! -s "$UPLOAD_LOG" ] && pass "nothing re-uploaded" || bad "re-uploaded identical assets"

printf '\npublished release, missing asset + SHA256SUMS refresh\n'
reset_case
cp "$TMP/dist/b.txt" "$STORE/"
( cd "$STORE" && sha256sum b.txt > SHA256SUMS )
if upload_assets v1.2.3 "$TMP/dist"; then pass "missing asset attached"; else bad "backfill failed"; fi
[ "$(cat "$STORE/a.txt" 2>/dev/null)" = "alpha" ] && pass "a.txt attached" || bad "a.txt missing"
refresh_sums v1.2.3 "$TMP/dist"
( cd "$STORE" && sha256sum -c --quiet SHA256SUMS ) && pass "SHA256SUMS matches the release" || bad "SHA256SUMS is stale"
[ "$(wc -l < "$UPLOAD_LOG")" -eq 2 ] && pass "attached asset and sums uploaded" || bad "unexpected upload count"

printf '\npublished release, existing asset with different content\n'
reset_case
printf 'tampered\n' > "$STORE/a.txt"
cp "$TMP/dist/b.txt" "$STORE/"
if upload_assets v1.2.3 "$TMP/dist"; then pass "existing assets accepted"; else bad "existing assets rejected"; fi
[ "$(cat "$STORE/a.txt")" = "tampered" ] && pass "existing asset not overwritten" || bad "existing asset was overwritten"
[ ! -s "$UPLOAD_LOG" ] && pass "nothing uploaded when every asset exists" || bad "unexpected upload"

printf '\n'
if [ "$fail" -ne 0 ]; then
    echo "release upload tests FAILED" >&2
    exit 1
fi
echo "release upload tests passed"
