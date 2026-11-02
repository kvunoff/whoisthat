#!/usr/bin/env bash
#
# Tests for scripts/check-docs.sh.
#
# The anchor check used to abort silently when a link pointed into a file
# without headings (anchors_of returns 1 under `set -euo pipefail`). This suite
# pins both the exit code and the report.

set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd) || exit 1
TMP=$(mktemp -d "${TMPDIR:-/tmp}/check-docs-test.XXXXXX") || exit 1
trap 'rm -rf "$TMP"' EXIT

fail=0
pass() { printf '  ok    %s\n' "$*"; }
bad()  { printf '  FAIL  %s\n' "$*" >&2; fail=1; }

printf '\nreal tree\n'
if "$ROOT/scripts/check-docs.sh" >"$TMP/real.log" 2>&1; then
    pass "current docs pass"
else
    cat "$TMP/real.log" >&2
    bad "current docs fail"
fi

printf '\nbroken anchor into a heading-less file\n'
cp -a "$ROOT/docs" "$TMP/docs" || { bad "cannot copy docs"; exit 1; }
printf 'no headings here\n' > "$TMP/docs/zz-nohead.md"
printf '# Fixture\n\n[broken](zz-nohead.md#missing)\n' > "$TMP/docs/zz-anchor.md"

if DOCS_DIR="$TMP/docs" "$ROOT/scripts/check-docs.sh" >"$TMP/broken.log" 2>&1; then
    bad "expected exit 1"
else
    pass "exit 1"
fi

if grep -q 'BROKEN' "$TMP/broken.log"; then
    pass "reports BROKEN"
else
    cat "$TMP/broken.log" >&2
    bad "no BROKEN in output"
fi

printf '\n'
if [ "$fail" -ne 0 ]; then
    echo "check-docs tests FAILED" >&2
    exit 1
fi
echo "check-docs tests passed"
