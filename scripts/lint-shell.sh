#!/usr/bin/env bash
# Lint every shell script in the repository, discovered by its shebang.
#
# A hand-written file list silently misses new scripts; the shebang scan also
# picks up tracked scripts that have no .sh extension.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# --others: catch scripts that are not committed yet, so the check works before
# the first commit of a change.
mapfile -t files < <(git ls-files --cached --others --exclude-standard -z |
    xargs -0 grep -lIE '^#!.*(bash|/sh|openrc-run)' 2>/dev/null |
    grep -v '\.md$' | sort -u)

if [ "${#files[@]}" -eq 0 ]; then
    echo 'lint-shell: no shell scripts found' >&2
    exit 1
fi

echo "shellcheck: ${#files[@]} files"
shellcheck -S warning "${files[@]}"
