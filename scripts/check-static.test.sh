#!/usr/bin/env bash
# Negative cases only: a static binary needs the musl toolchain, so the
# positive path is covered by the CI `musl` job.
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT" || exit 1

fail=0
ok() { printf '  ok    %s\n' "$*"; }
bad() { printf '  FAIL  %s\n' "$*" >&2; fail=1; }

if ./scripts/check-static.sh /bin/true >/dev/null 2>&1; then
    bad 'dynamic binary accepted'
else
    ok 'dynamic binary rejected'
fi

./scripts/check-static.sh >/dev/null 2>&1
if [ $? -eq 2 ]; then
    ok 'missing arguments exit 2'
else
    bad 'missing arguments should exit 2'
fi

exit $fail
