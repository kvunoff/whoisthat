#!/usr/bin/env bash
# Cross-check documentation names against their sources of truth in the code.
#
# The docs are hand-written; VitePress verifies that linked files exist but not
# that IPC commands, notification names, config keys or version strings still
# match the code. This script catches that drift early.
#
# Run from anywhere: ./scripts/check-docs.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DOCS="${DOCS_DIR:-$ROOT/docs}"
PROTOCOL="$DOCS/reference/protocol.md"
CONFIGURATION="$DOCS/reference/configuration.md"
CLI_DOC="$DOCS/reference/cli.md"
DOCTOR_DOC="$DOCS/reference/doctor.md"
TCPSERVER="$ROOT/core/core/lib/TCPServer/dispatch.go"
APPCONFIG="$ROOT/core/core/lib/AppConfig/config.go"
ARGS="$ROOT/src/cli/args.rs"
CHECKS="$ROOT/src/doctor/checks"

fail=0
missing() {
    printf '  MISSING %s\n' "$*"
    fail=1
}
broken() {
    printf '  BROKEN  %s\n' "$*"
    fail=1
}
stale() {
    printf '  STALE   %s\n' "$*"
    fail=1
}

# ---------------------------------------------------------------------------
# 1. IPC commands: every `case "..."` must be documented in protocol.md
# ---------------------------------------------------------------------------
echo "docs: checking IPC commands..."
while IFS= read -r command; do
    [ -n "$command" ] || continue
    grep -qF "\`$command\`" "$PROTOCOL" || missing "$PROTOCOL: command \`$command\`"
done < <(
    sed -nE 's/^\tcase "([a-z0-9-]+)".*/\1/p' "$TCPSERVER" |
        sort -u
)

# ---------------------------------------------------------------------------
# 2. Core notifications: every CreateJsonNotification("...") name
# ---------------------------------------------------------------------------
echo "docs: checking IPC notifications..."
while IFS= read -r notification; do
    [ -n "$notification" ] || continue
    grep -qF "\`$notification\`" "$PROTOCOL" || missing "$PROTOCOL: notification \`$notification\`"
done < <(
    grep -rhoE --include='*.go' 'CreateJsonNotification\("[a-z0-9-]+"' "$ROOT/core/core" |
        sed -E 's/.*"//' |
        sort -u
)

# ---------------------------------------------------------------------------
# 3. Core config keys: json tags from AppConfig
# ---------------------------------------------------------------------------
echo "docs: checking core config keys..."
while IFS= read -r key; do
    [ -n "$key" ] || continue
    grep -qF -- "$key" "$CONFIGURATION" || missing "$CONFIGURATION: config key \`$key\`"
done < <(
    sed -n '/^type AppConfig struct {/,/^}/p' "$APPCONFIG" |
        grep -oE 'json:"[a-z0-9-]+"' |
        sed -E 's/json:"//; s/"//' |
        sort -u
)

# ---------------------------------------------------------------------------
# 4. Top-level CLI nouns: variants of `pub enum Command`
# ---------------------------------------------------------------------------
echo "docs: checking CLI commands..."
while IFS= read -r command; do
    [ -n "$command" ] || continue
    grep -qF "whoisthat $command" "$CLI_DOC" ||
        missing "$CLI_DOC: CLI command \`whoisthat $command\`"
done < <(
    sed -n '/^pub enum Command {/,/^}/p' "$ARGS" |
        grep -oE '^    [A-Z][A-Za-z]+' |
        awk '{print tolower($1)}' |
        sort -u
)

# ---------------------------------------------------------------------------
# 5. Doctor categories: CheckCategory::new("...")
# ---------------------------------------------------------------------------
echo "docs: checking doctor categories..."
while IFS= read -r category; do
    [ -n "$category" ] || continue
    grep -qF "$category" "$DOCTOR_DOC" || missing "$DOCTOR_DOC: category \`$category\`"
done < <(
    grep -rhoE 'CheckCategory::new\("[^"]+"' "$CHECKS" |
        sed -E 's/.*"//' |
        sort -u
)

# ---------------------------------------------------------------------------
# 6. Internal links and anchors
# ---------------------------------------------------------------------------
slugify() {
    tr '[:upper:]' '[:lower:]' |
        sed -E 's/<[^>]*>//g' |
        sed -E 's/[^a-z0-9 _-]//g' |
        sed -E 's/ +/-/g; s/^-+//; s/-+$//'
}

anchors_of() {
    grep -E '^#{1,6} ' "$1" 2>/dev/null |
        sed -E 's/^#{1,6} //' |
        while IFS= read -r heading; do
            printf '%s\n' "$heading" | slugify
        done
}

echo "docs: checking internal links and anchors..."
while IFS= read -r md; do
    while IFS= read -r link; do
        case "$link" in
            http://* | https://* | mailto:* | tel:*) continue ;;
            *.png | *.jpg | *.jpeg | *.svg | *.gif) continue ;;
        esac

        target="${link%%#*}"
        anchor=""
        if [ "$target" != "$link" ]; then
            anchor="${link#*#}"
        fi

        if [ -z "$target" ]; then
            target_file="$md"
        else
            case "$target" in
                /*) target_file="$DOCS${target}" ;;
                *) target_file="$(dirname "$md")/${target}" ;;
            esac
            [ -e "$target_file" ] || [ -e "${target_file}.md" ] || {
                broken "$md -> $link (file not found)"
                continue
            }
            [ -e "$target_file" ] || target_file="${target_file}.md"
        fi

        # A directory URL such as /developers/ resolves to its index page.
        if [ -d "$target_file" ]; then
            target_file="${target_file%/}/index.md"
        fi

        [ -n "$anchor" ] || continue
        anchors="$(anchors_of "$target_file" || true)"
        grep -Fxq "$anchor" <<<"$anchors" ||
            broken "$md -> $link (anchor not found)"
    done < <(
        grep -oE '\]\([^)]+\)' "$md" |
            sed -E 's/^\]\(//; s/\)$//'
    )
done < <(find "$DOCS" -name '*.md' -not -path '*/.vitepress/*' | sort)

# ---------------------------------------------------------------------------
# 7. Version and toolchain strings
# ---------------------------------------------------------------------------
echo "docs: checking version/toolchain strings..."
edition="$(sed -nE 's/^edition = "([^"]+)"/\1/p' Cargo.toml | head -1)"
msrv="$(sed -nE 's/^rust-version = "([^"]+)"/\1/p' Cargo.toml | head -1)"
protocol="$(tr -d '[:space:]' < core/core/lib/version/protocol.txt)"
pinned="$(sed -nE 's/^const PinnedVersion = "([^"]+)"/\1/p' core/core/lib/xraymgr/version.go)"
go_version="$(awk '/^go /{print $2; exit}' core/core/go.mod)"

if grep -rqF "edition 2021" "$DOCS"; then
    stale "$DOCS: \"edition 2021\" (Cargo.toml: edition $edition)"
fi
grep -rqF "edition $edition" "$DOCS" ||
    stale "$DOCS: no mention of edition $edition"
grep -rqF "$msrv" "$DOCS" ||
    stale "$DOCS: no mention of Rust $msrv"
grep -qF "**Protocol Version**: \`$protocol\`" "$PROTOCOL" ||
    stale "$PROTOCOL: protocol version is $protocol"
grep -rqF "$pinned" "$DOCS" ||
    stale "$DOCS: no mention of pinned Xray $pinned"
grep -rqF "${go_version%.*}" "$DOCS/guide/installation.md" ||
    stale "$DOCS/guide/installation.md: no mention of Go ${go_version%.*}"

# ---------------------------------------------------------------------------
# 8. Frontmatter and lifecycle metadata
# ---------------------------------------------------------------------------
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

internal_path_to_file() {
    local path="$1"
    path="${path%%#*}"
    case "$path" in
        /*) ;;
        *) return 1 ;;
    esac
    path="${path#/}"
    if [ -z "$path" ]; then
        printf 'index.md\n'
    elif [ "${path%/}" != "$path" ]; then
        printf '%s/index.md\n' "${path%/}"
    else
        printf '%s.md\n' "$path"
    fi
}

current_version="$(sed -nE 's/^version = "([^"]+)"/\1/p' Cargo.toml | head -1)"

echo "docs: checking frontmatter..."
while IFS= read -r md; do
    [ -n "$md" ] || continue
    rel="${md#"$DOCS"/}"

    if [ "$(head -n 1 "$md")" != "---" ]; then
        missing "$rel: frontmatter block is missing"
        continue
    fi
    if ! tail -n +2 "$md" | grep -qx -- '---'; then
        missing "$rel: frontmatter closing --- is missing"
        continue
    fi

    for field in title description audience type; do
        [ -n "$(frontmatter_value "$md" "$field")" ] ||
            missing "$rel: frontmatter field '$field' is missing"
    done

    audience="$(frontmatter_value "$md" audience)"
    case "$audience" in
        user | operator | integrator | developer) ;;
        *) missing "$rel: unknown audience '$audience'" ;;
    esac

    doc_type="$(frontmatter_value "$md" type)"
    case "$doc_type" in
        landing | tutorial | how-to | reference | explanation) ;;
        *) missing "$rel: unknown type '$doc_type'" ;;
    esac

    status="$(frontmatter_value "$md" status)"
    [ -n "$status" ] || status=current
    case "$status" in
        current | experimental | legacy | deprecated) ;;
        *) missing "$rel: unknown status '$status'" ;;
    esac

    for field in since changed_in deprecated_in removed_in; do
        value="$(frontmatter_value "$md" "$field")"
        [ -z "$value" ] && continue
        printf '%s' "$value" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' ||
            missing "$rel: '$field' is not semver: $value"
    done

    if [ "$status" = "deprecated" ]; then
        deprecated_in="$(frontmatter_value "$md" deprecated_in)"
        removed_in="$(frontmatter_value "$md" removed_in)"
        replaced_by="$(frontmatter_value "$md" replaced_by)"
        [ -n "$deprecated_in" ] ||
            missing "$rel: deprecated page needs 'deprecated_in'"
        [ -n "$removed_in" ] ||
            missing "$rel: deprecated page needs 'removed_in'"
        [ -n "$replaced_by" ] ||
            missing "$rel: deprecated page needs 'replaced_by'"

        if [ -n "$replaced_by" ] && [ "$replaced_by" != "none" ]; then
            if target="$(internal_path_to_file "$replaced_by")"; then
                [ -f "$DOCS/$target" ] ||
                    missing "$rel: 'replaced_by' target not found: $replaced_by"
            else
                missing "$rel: 'replaced_by' must be an internal path or none"
            fi
        fi

        if [ -n "$removed_in" ] && [ -n "$current_version" ]; then
            smallest="$(printf '%s\n%s\n' "$removed_in" "$current_version" | sort -V | head -n 1)"
            if [ "$smallest" = "$removed_in" ]; then
                missing "$rel: 'removed_in' $removed_in is not after current $current_version"
            fi
        fi
    fi
done < <(find "$DOCS" -name '*.md' -not -path '*/.vitepress/*' | sort)

# ---------------------------------------------------------------------------
# 9. Sidebar coverage: every page is linked, every link resolves
# ---------------------------------------------------------------------------
echo "docs: checking sidebar coverage..."
sidebar_links="$(grep -oE "link: ['\"][^'\"]+['\"]" "$DOCS/.vitepress/config.mts" |
    sed -E "s/^link: ['\"]//; s/['\"]$//" || true)"

mapped_links=""
if [ -n "$sidebar_links" ]; then
    mapped_links="$(printf '%s\n' "$sidebar_links" |
        while IFS= read -r link; do
            case "$link" in
                /*) ;;
                *) continue ;;
            esac
            target="$(internal_path_to_file "$link")" || continue
            printf '%s\n' "$target"
        done | sort -u)"
fi

while IFS= read -r rel; do
    [ -n "$rel" ] || continue
    grep -Fxq "$rel" <<<"$mapped_links" ||
        missing "$rel: not linked from docs/.vitepress/config.mts"
done < <(find "$DOCS" -name '*.md' -not -path '*/.vitepress/*' -printf '%P\n' | sort)

while IFS= read -r rel; do
    [ -n "$rel" ] || continue
    [ -f "$DOCS/$rel" ] ||
        missing "docs/.vitepress/config.mts: link target not found: $rel"
done <<<"$mapped_links"

# ---------------------------------------------------------------------------
# 10. Redirect stubs point at real pages and do not shadow generated pages
# ---------------------------------------------------------------------------
echo "docs: checking redirect stubs..."
while IFS= read -r stub; do
    [ -n "$stub" ] || continue
    target_url="$(grep -oE 'url=[^"]+' "$stub" | head -n 1 | sed 's/^url=//' || true)"
    if [ -z "$target_url" ]; then
        missing "$stub: redirect stub has no meta refresh url"
        continue
    fi

    if target="$(internal_path_to_file "$target_url")"; then
        [ -f "$DOCS/$target" ] ||
            missing "$stub: redirect target not found: $target_url"
    else
        missing "$stub: redirect target must be an internal path: $target_url"
    fi

    rel="${stub#"$DOCS"/public/}"
    rel="${rel%.html}.md"
    if [ -f "$DOCS/$rel" ]; then
        missing "$stub: stub collides with generated page $rel"
    fi
done < <(find "$DOCS/public" -name '*.html' 2>/dev/null | sort)

# ---------------------------------------------------------------------------
# 11. Heading numbering, fence language, terminology
# ---------------------------------------------------------------------------
echo "docs: checking headings, fences, and terminology..."
while IFS= read -r md; do
    [ -n "$md" ] || continue
    rel="${md#"$DOCS"/}"

    while IFS= read -r hit; do
        missing "$rel:$hit"
    done < <(awk '
        /^```/ { inf = !inf; next }
        !inf && /^#{1,6} [0-9]+\./ { print NR ": numbered heading: " $0 }
    ' "$md")

    while IFS= read -r hit; do
        missing "$rel:$hit"
    done < <(awk '
        /^```/ {
            if (!inf) {
                if ($0 == "```") print NR ": code fence without language"
                inf = 1
            } else {
                inf = 0
            }
        }
    ' "$md")

    if [ "$rel" != "reference/glossary.md" ]; then
        while IFS= read -r hit; do
            missing "$rel: use 'split tunneling': $hit"
        done < <(grep -in -- 'split tunnelling' "$md" || true)
    fi
done < <(find "$DOCS" -name '*.md' -not -path '*/.vitepress/*' | sort)

if [ "$fail" -ne 0 ]; then
    echo "docs: inconsistencies found"
    exit 1
fi

echo "docs: all checks passed"
