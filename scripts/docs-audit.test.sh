#!/usr/bin/env bash
# Tests for scripts/docs-audit.sh.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)" || exit 1
TMP="$(mktemp -d "${TMPDIR:-/tmp}/docs-audit-test.XXXXXX")" || exit 1
trap 'rm -rf "$TMP"' EXIT

fail=0
pass() { printf '  ok    %s\n' "$*"; }
bad()  { printf '  FAIL  %s\n' "$*" >&2; fail=1; }

mkdir -p "$TMP/docs"

cat >"$TMP/docs/expired.md" <<'EOF'
---
title: "Expired"
description: "Fixture."
audience: "user"
type: "reference"
status: "deprecated"
removed_in: "0.18.0"
---

# Expired
EOF

cat >"$TMP/docs/overdue.md" <<'EOF'
---
title: "Overdue"
description: "Fixture."
audience: "user"
type: "reference"
review_by: "2020-01-01"
---

# Overdue
EOF

cat >"$TMP/docs/current.md" <<'EOF'
---
title: "Current"
description: "Fixture."
audience: "user"
type: "reference"
---

# Current
EOF

if out="$(DOCS_DIR="$TMP/docs" CURRENT_VERSION="0.19.0" TODAY="2026-11-13" "$ROOT/scripts/docs-audit.sh")"; then
    pass "exit 0"
else
    bad "expected exit 0"
fi

if grep -q 'expired.md (removed_in 0.18.0' <<<"$out"; then
    pass "reports expired deprecation"
else
    cat <<<"$out" >&2
    bad "missing expired deprecation"
fi

if grep -q 'overdue.md (review_by 2020-01-01)' <<<"$out"; then
    pass "reports overdue review"
else
    cat <<<"$out" >&2
    bad "missing overdue review"
fi

if grep -q 'current.md' <<<"$out"; then
    bad "current page should not be reported"
else
    pass "ignores current pages"
fi

printf '\n'
if [ "$fail" -ne 0 ]; then
    echo "docs-audit tests FAILED" >&2
    exit 1
fi
echo "docs-audit tests passed"
