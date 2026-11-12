#!/usr/bin/env bash
#
# Tests for scripts/check-docs.sh.
#
# The anchor check used to abort silently when a link pointed into a file
# without headings (anchors_of returns 1 under `set -euo pipefail`). This suite
# pins both the exit code and the report. It also covers the frontmatter,
# lifecycle, and sidebar checks added with the documentation restructuring.

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

printf '\nbroken anchor and invalid metadata\n'
cp -a "$ROOT/docs" "$TMP/docs" || { bad "cannot copy docs"; exit 1; }
printf 'no headings here\n' > "$TMP/docs/zz-nohead.md"
printf '# Fixture\n\n[broken](zz-nohead.md#missing)\n' > "$TMP/docs/zz-anchor.md"
printf '# No frontmatter\n' > "$TMP/docs/zz-no-frontmatter.md"

cat >"$TMP/docs/zz-bad-type.md" <<'EOF'
---
title: "Bad type"
description: "Fixture."
audience: "user"
type: "invalid"
---

# Bad type
EOF

cat >"$TMP/docs/zz-orphan.md" <<'EOF'
---
title: "Orphan"
description: "Fixture."
audience: "user"
type: "reference"
---

# Orphan
EOF

cat >"$TMP/docs/zz-deprecated.md" <<'EOF'
---
title: "Deprecated"
description: "Fixture."
audience: "user"
type: "how-to"
status: "deprecated"
---

# Deprecated
EOF

cat >"$TMP/docs/zz-replaced.md" <<'EOF'
---
title: "Replaced"
description: "Fixture."
audience: "user"
type: "how-to"
status: "deprecated"
deprecated_in: "0.20.0"
removed_in: "0.21.0"
replaced_by: "/usage/nope"
---

# Replaced
EOF

cat >"$TMP/docs/zz-removed.md" <<'EOF'
---
title: "Removed"
description: "Fixture."
audience: "user"
type: "how-to"
status: "deprecated"
deprecated_in: "0.18.0"
removed_in: "0.19.0"
replaced_by: "none"
---

# Removed
EOF

mkdir -p "$TMP/docs/zz-dirlanding"
cat >"$TMP/docs/zz-dirlanding/index.md" <<'EOF'
---
title: "Directory landing"
description: "Fixture."
audience: "user"
type: "reference"
---

# Directory landing

## Anchor target
EOF

cat >"$TMP/docs/zz-dirlanding-link.md" <<'EOF'
---
title: "Directory link"
description: "Fixture."
audience: "user"
type: "reference"
---

# Directory link

[ok](/zz-dirlanding/#anchor-target)
EOF

printf "\n{ text: 'Missing', link: '/zz-missing' },\n" >>"$TMP/docs/.vitepress/config.mts"

cat >"$TMP/docs/public/zz-broken.html" <<'EOF'
<!doctype html>
<meta http-equiv="refresh" content="0; url=/zz-missing-page">
EOF

cat >"$TMP/docs/public/zz-collision.html" <<'EOF'
<!doctype html>
<meta http-equiv="refresh" content="0; url=/reference/cli">
EOF

cat >"$TMP/docs/zz-collision.md" <<'EOF'
---
title: "Collision"
description: "Fixture."
audience: "user"
type: "reference"
---

# Collision
EOF

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

check_report() {
    if grep -qF "$1" "$TMP/broken.log"; then
        pass "reports $2"
    else
        cat "$TMP/broken.log" >&2
        bad "missing $2"
    fi
}

check_report 'frontmatter block is missing' "missing frontmatter"
check_report "unknown type 'invalid'" "invalid type"
check_report 'not linked from docs/.vitepress/config.mts' "orphan page"
check_report "deprecated page needs 'removed_in'" "deprecated metadata"
check_report "'replaced_by' target not found" "broken replacement target"
check_report "'removed_in' 0.19.0 is not after current" "expired removal"
check_report 'link target not found: zz-missing.md' "broken sidebar link"
check_report 'redirect target not found: /zz-missing-page' "broken redirect target"
check_report 'stub collides with generated page zz-collision.md' "redirect/page collision"

if grep -q 'BROKEN.*zz-dirlanding' "$TMP/broken.log"; then
    cat "$TMP/broken.log" >&2
    bad "directory index anchor reported broken"
else
    pass "directory index anchor resolves"
fi

printf '\n'
if [ "$fail" -ne 0 ]; then
    echo "check-docs tests FAILED" >&2
    exit 1
fi
echo "check-docs tests passed"
