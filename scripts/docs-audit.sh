#!/usr/bin/env bash
# Report documentation that needs attention: expired deprecations and overdue
# review dates. Always exits 0 so the scheduled workflow can publish a report.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOCS="${DOCS_DIR:-$ROOT/docs}"
CURRENT_VERSION="${CURRENT_VERSION:-$(sed -nE 's/^version = "([^"]+)"/\1/p' "$ROOT/Cargo.toml" | head -n 1)}"
TODAY="${TODAY:-$(date -u +%F)}"

frontmatter_value() {
    awk -v key="$2" '
        NR == 1 {
            if ($0 != "---") exit 0
            in_fm = 1
            next
        }
        in_fm && $0 == "---" { exit 0 }
        in_fm {
            if (index($0, key ":") == 1) {
                value = substr($0, length(key) + 2)
                sub(/^[ \t]+/, "", value)
                sub(/[ \t]+#.*$/, "", value)
                gsub(/^"|"$/, "", value)
                print value
                exit 0
            }
        }
    ' "$1"
}

pages=0
expired=""
missing_removed=""
overdue=""

while IFS= read -r md; do
    [ -n "$md" ] || continue
    rel="${md#"$DOCS"/}"
    first="$(head -n 1 "$md" || true)"
    [ "$first" = "---" ] || continue
    pages=$((pages + 1))

    status="$(frontmatter_value "$md" status)"
    removed_in="$(frontmatter_value "$md" removed_in)"
    review_by="$(frontmatter_value "$md" review_by)"

    if [ "$status" = "deprecated" ]; then
        if [ -z "$removed_in" ]; then
            missing_removed="${missing_removed}"$'\n'"- $rel"
        else
            smallest="$(printf '%s\n%s\n' "$removed_in" "$CURRENT_VERSION" | sort -V | head -n 1)"
            if [ "$smallest" = "$removed_in" ]; then
                expired="${expired}"$'\n'"- $rel (removed_in $removed_in, current $CURRENT_VERSION)"
            fi
        fi
    fi

    if [ -n "$review_by" ] && [[ "$review_by" < "$TODAY" ]]; then
        overdue="${overdue}"$'\n'"- $rel (review_by $review_by)"
    fi
done < <(find "$DOCS" -name '*.md' -not -path '*/.vitepress/*' | sort)

echo "## Documentation audit"
echo
echo "- Client version: $CURRENT_VERSION"
echo "- Audit date: $TODAY"
echo "- Pages: $pages"
echo
echo "### Deprecated past removal"
printf '%s\n' "${expired:-None.}"
echo
echo "### Deprecated without removed_in"
printf '%s\n' "${missing_removed:-None.}"
echo
echo "### review_by overdue"
printf '%s\n' "${overdue:-None.}"
echo
echo "_Run \`./scripts/check-docs.sh\` for link, frontmatter, and sidebar checks._"
