#!/usr/bin/env bash
# The swallowed-error gate must fail on a discarded state write and pass on a
# clean tree.
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT" || exit 1

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

fail=0
ok() { printf '  ok    %s\n' "$*"; }
bad() { printf '  FAIL  %s\n' "$*" >&2; fail=1; }

printf 'package x\n\nimport "os"\n\nfunc f() { _ = os.WriteFile("a", nil, 0o600) }\n' > "$TMP/bad.go"
if ./scripts/check-swallowed-errors.sh "$TMP" >/dev/null 2>&1; then
    bad 'discarded state write accepted'
else
    ok 'discarded state write rejected'
fi

rm -f "$TMP/bad.go"
printf 'package x\n' > "$TMP/good.go"
if ./scripts/check-swallowed-errors.sh "$TMP" >/dev/null 2>&1; then
    ok 'clean tree accepted'
else
    bad 'clean tree rejected'
fi

exit $fail
