#!/usr/bin/env bash
# Every self-contained scripts/*.test.sh, then the init-detect check.
#
# ipc-e2e needs built binaries and runs via `make test-e2e`, so it is skipped
# here. Failures are collected: one broken script does not hide the others.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1

fail=0
for t in scripts/*.test.sh; do
    case $t in scripts/ipc-e2e.test.sh) continue ;; esac
    printf '\n== %s ==\n' "$t"
    "$t" || fail=1
done

printf '\n== scripts/check-dinit.sh ==\n'
./scripts/check-dinit.sh || fail=1

exit $fail
