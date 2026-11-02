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

DOCS="$ROOT/docs"
PROTOCOL="$DOCS/reference/protocol.md"
CONFIGURATION="$DOCS/reference/configuration.md"
CLI_DOC="$DOCS/integrations/cli.md"
DOCTOR_DOC="$DOCS/reference/doctor.md"
TCPSERVER="$ROOT/core/core/lib/TCPServer/TCPServer.go"
APPCONFIG="$ROOT/core/core/lib/AppConfig/app-config.go"
ARGS="$ROOT/src/cli/args.rs"
CHECKS="$ROOT/src/doctor/checks.rs"

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
    sed -nE 's/^\t\tcase "([a-z0-9-]+)".*/\1/p' "$TCPSERVER" |
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
    grep -oE 'CheckCategory::new\("[^"]+"' "$CHECKS" |
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

        [ -n "$anchor" ] || continue
        anchors="$(anchors_of "$target_file")"
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

if [ "$fail" -ne 0 ]; then
    echo "docs: inconsistencies found"
    exit 1
fi

echo "docs: all checks passed"
