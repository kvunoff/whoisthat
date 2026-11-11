#!/bin/sh
# Static sanity checks for dinit service descriptions. These files are not
# shell, so shellcheck does not apply; dinit 0.21 accepts only these service
# types and no args.
set -eu

fail=0
for file in pkg/init/dinit/*; do
    [ -f "$file" ] || continue
    name=$(basename "$file")
    grep -q '^type = ' "$file" || { echo "dinit: $name has no type" >&2; fail=1; }
    grep -q '^command = ' "$file" || { echo "dinit: $name has no command" >&2; fail=1; }
    if grep -q '^args = ' "$file"; then
        echo "dinit: $name uses unsupported 'args'; put arguments in 'command'" >&2
        fail=1
    fi
    if grep -q '^type = oneshot' "$file"; then
        echo "dinit: $name uses unsupported type 'oneshot' (use 'scripted')" >&2
        fail=1
    fi
    if grep -q '^run-as = .*:' "$file"; then
        echo "dinit: $name uses 'run-as = user:group'; dinit accepts only a user name" >&2
        fail=1
    fi
done

[ "$fail" -eq 0 ] || exit 1
echo "dinit service checks passed"
