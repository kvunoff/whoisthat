#!/usr/bin/env bash
#
# Tests for scripts/bump.sh. Everything runs in a throwaway clone with a local
# bare origin and a fake gh, so nothing touches the real repository or GitHub.

set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd) || exit 1
TMP=$(mktemp -d "${TMPDIR:-/tmp}/bump-test.XXXXXX") || exit 1
trap 'rm -rf "$TMP"' EXIT

fail=0
pass()      { printf '  ok    %s\n' "$*"; }
bad()       { printf '  FAIL  %s\n' "$*" >&2; fail=1; }
assert_eq() { if [ "$1" = "$2" ]; then pass "$3"; else bad "$3: '$1' != '$2'"; fi; }
assert_grep() { if grep -q -- "$2" "$1"; then pass "$3"; else bad "$3 (missing '$2')"; fi; }
assert_no_grep() { if grep -q -- "$2" "$1"; then bad "$3 (unexpected '$2')"; else pass "$3"; fi; }

# --- fixtures ----------------------------------------------------------------

build_base() {
    local dst="$TMP/base"
    mkdir -p "$dst"
    ( cd "$ROOT" && git ls-files -z | tar --null -T - -cf - ) | tar -xf - -C "$dst"
    cp "$ROOT/scripts/bump.sh" "$dst/scripts/bump.sh"
    chmod +x "$dst/scripts/bump.sh" "$dst/scripts/check-versions.sh"
    # Pin the fixture: the assertions below use fixed versions, so the suite
    # must not follow the released versions in the live tree.
    ( cd "$dst" && ./scripts/check-versions.sh --set tui=0.15.1 core=0.3.8 protocol=3 pkgrel=1 >/dev/null ) \
        || { bad "build_base: cannot pin fixture versions"; return 1; }
    git -C "$dst" init -q -b main
    git -C "$dst" config user.email test@example.com
    git -C "$dst" config user.name Test
    git -C "$dst" add -A
    git -C "$dst" commit -q -m base
    git -C "$dst" tag v0.0.0
    git -C "$dst" tag v0.14.0
    git -C "$dst" tag v0.15.0
    git -C "$dst" tag v0.15.1
    git -C "$dst" tag core-v0.2.0
    git -C "$dst" tag core-v0.3.8
}

make_fake_gh() {
    mkdir -p "$TMP/bin"
    cat > "$TMP/bin/gh" <<'EOF'
#!/usr/bin/env bash
printf 'gh %s\n' "$*" >> "$GH_LOG"
args=("$@")
for ((i = 0; i < ${#args[@]}; i++)); do
    if [ "${args[i]}" = "--notes-file" ]; then
        printf -- '--- notes ---\n' >> "$GH_LOG"
        cat "${args[i + 1]}" >> "$GH_LOG"
    fi
done
if [ "${1:-}" = auth ]; then exit 0; fi
if [ "${1:-}" = release ] && [ "${2:-}" = view ]; then
    if [ "${3:-}" = v0.15.1 ]; then printf 'old body\n'; exit 0; fi
    exit 1
fi
exit 0
EOF
    chmod +x "$TMP/bin/gh"
}

CASE_DIR=""
CASE_LOG=""

setup_case() {
    local name=$1
    CASE_DIR="$TMP/$name"
    CASE_LOG="$TMP/gh-$name.log"
    : > "$CASE_LOG"
    git clone -q --bare "$TMP/base" "$TMP/$name-origin.git" || { bad "$name: clone --bare"; return 1; }
    git clone -q "$TMP/$name-origin.git" "$CASE_DIR" || { bad "$name: clone"; return 1; }
    git -C "$CASE_DIR" config user.email test@example.com
    git -C "$CASE_DIR" config user.name Test
    mkdir -p "$CASE_DIR/.local/bump"
    cat > "$CASE_DIR/.local/bump/commit.md" <<'EOF'
Test summary

Test body.
EOF
    cat > "$CASE_DIR/.local/bump/release.md" <<'EOF'
- Test bullet.
EOF
}

run_bump() {
    local out
    out="$TMP/out-$(basename "$CASE_DIR").log"
    ( cd "$CASE_DIR" && GH_LOG="$CASE_LOG" PATH="$TMP/bin:$PATH" ./scripts/bump.sh "$@" --fast --push ) > "$out" 2>&1
    local rc=$?
    if [ "$rc" -ne 0 ]; then
        cat "$out" >&2
        bad "$(basename "$CASE_DIR"): exit $rc"
    fi
    return "$rc"
}

tui_ver()  { sed -n 's/^version = "\([^"]*\)"$/\1/p' "$CASE_DIR/Cargo.toml" | head -n1; }
core_ver() { sed -n 's/^const CoreVersion = "\([^"]*\)"$/\1/p' "$CASE_DIR/core/core/lib/version/version.go" | head -n1; }
pkgrel()   { sed -n 's/^pkgrel=//p' "$CASE_DIR/pkg/aur/PKGBUILD"; }
proto_ver(){ tr -d '[:space:]' < "$CASE_DIR/core/core/lib/version/protocol.txt"; }
head_rev() { git -C "$CASE_DIR" rev-parse HEAD; }
tag_rev()  { git -C "$CASE_DIR" rev-parse "$1^{commit}" 2>/dev/null; }
commits()  { git -C "$CASE_DIR" log --format=%s -n "$1"; }
check_ok() { ( cd "$CASE_DIR" && ./scripts/check-versions.sh >/dev/null 2>&1 ); }

# --- tests -------------------------------------------------------------------

build_base
make_fake_gh

printf '\ntui minor\n'
setup_case tui-minor
run_bump --kind tui --bump-type 2
assert_eq "$(tui_ver)"   "0.16.0" "tui version"
assert_eq "$(pkgrel)"    "1"      "pkgrel reset"
assert_eq "$(tag_rev v0.16.0)" "$(head_rev)" "v0.16.0 on HEAD"
assert_eq "$(commits 1)" "v0.16.0: Test summary" "commit subject"
assert_grep "$CASE_LOG" "release create v0.16.0" "bundle release created"
assert_grep "$CASE_LOG" "compare/v0.15.0...v0.16.0" "minor changelog base"
assert_grep "$CASE_LOG" "### What's Changed" "release body wrapper"
assert_grep "$CASE_DIR/.local/bump/commit.md.backup" "Test summary" "commit backup"
assert_grep "$CASE_DIR/.local/bump/commit.md" "bump: fill in" "template restored"
if check_ok; then pass "check-versions passes"; else bad "check-versions fails"; fi

printf '\ntui major\n'
setup_case tui-major
run_bump --kind tui --bump-type 1
assert_eq "$(tui_ver)" "1.0.0" "tui version"
assert_grep "$CASE_LOG" "compare/v0.0.0...v1.0.0" "major changelog base"

printf '\ntui patch\n'
setup_case tui-patch
run_bump --kind tui --bump-type 3
assert_eq "$(tui_ver)" "0.15.2" "tui version"
assert_grep "$CASE_LOG" "compare/v0.15.1...v0.15.2" "patch changelog base"

printf '\npackaging (pkgrel only, no new tag/release)\n'
setup_case packaging
run_bump --kind packaging --bump-type 4
assert_eq "$(tui_ver)" "0.15.1" "tui unchanged"
assert_eq "$(pkgrel)"  "2"      "pkgrel bumped"
assert_eq "$(tag_rev v0.15.1)" "$(head_rev)" "v0.15.1 moved to HEAD"
assert_eq "$(commits 1)" "v0.15.1-2: Test summary" "commit subject"
assert_no_grep "$CASE_LOG" "release create" "no new release"
assert_grep "$CASE_LOG" "release edit v0.15.1" "existing release edited"
assert_grep "$CASE_LOG" "## Package revision 2" "package revision section"
assert_grep "$CASE_LOG" "old body" "old notes preserved"

printf '\ncore (pkgrel ship)\n'
setup_case core
run_bump --kind core --bump-type 3
assert_eq "$(core_ver)" "0.3.9"  "core version"
assert_eq "$(tui_ver)"  "0.15.1" "tui unchanged"
assert_eq "$(pkgrel)"   "2"      "pkgrel bumped"
assert_eq "$(tag_rev core-v0.3.9)" "$(git -C "$CASE_DIR" rev-parse HEAD~1)" "core tag on core commit"
assert_eq "$(tag_rev v0.15.1)" "$(head_rev)" "v0.15.1 moved to HEAD"
assert_eq "$(commits 2 | tr '\n' '|')" "v0.15.1-2: Test summary|core-v0.3.9: Test summary|" "commit subjects"
assert_grep "$CASE_LOG" "release create core-v0.3.9" "core release created"
assert_grep "$CASE_LOG" "release edit v0.15.1" "bundle release edited"
assert_no_grep "$CASE_LOG" "release create v0.15.1" "no new bundle release"

printf '\ncore-bundle\n'
setup_case core-bundle
run_bump --kind core-bundle --bump-type 3 --tui-bump-type 3
assert_eq "$(core_ver)" "0.3.9"  "core version"
assert_eq "$(tui_ver)"  "0.15.2" "tui version"
assert_eq "$(pkgrel)"   "1"      "pkgrel reset"
assert_eq "$(tag_rev core-v0.3.9)" "$(git -C "$CASE_DIR" rev-parse HEAD~1)" "core tag on core commit"
assert_eq "$(tag_rev v0.15.2)" "$(head_rev)" "bundle tag on HEAD"
assert_grep "$CASE_LOG" "release create core-v0.3.9" "core release created"
assert_grep "$CASE_LOG" "release create v0.15.2" "bundle release created"
assert_grep "$CASE_LOG" "compare/v0.15.1...v0.15.2" "bundle changelog base"

printf '\nprotocol\n'
setup_case protocol
run_bump --kind protocol --bump-type 3 --protocol 4
assert_eq "$(proto_ver)" "4" "protocol.txt"
assert_eq "$(core_ver)"  "0.3.9" "core version"
assert_eq "$(tui_ver)"   "0.15.2" "tui version"
assert_grep "$CASE_DIR/src/core_client/protocol.rs" "EXPECTED_CORE_PROTOCOL_VERSION: u32 = 4;" "rust mirror"
if check_ok; then pass "check-versions passes"; else bad "check-versions fails"; fi

printf '\ndry run\n'
setup_case dryrun
( cd "$CASE_DIR" && GH_LOG="$CASE_LOG" PATH="$TMP/bin:$PATH" ./scripts/bump.sh --kind tui --bump-type 2 --dry-run ) > "$TMP/out-dryrun.log" 2>&1
assert_eq "$?" "0" "dry run exits 0"
assert_eq "$(tui_ver)" "0.15.1" "dry run writes nothing"
assert_eq "$(git -C "$CASE_DIR" rev-list --count HEAD)" "1" "dry run creates no commit"
assert_grep "$TMP/out-dryrun.log" "would update Cargo.toml" "dry run shows diff"
assert_grep "$TMP/out-dryrun.log" "release  create v0.16.0" "dry run shows plan"
assert_grep "$TMP/out-dryrun.log" "minor release: bundle notes must cover every change since v0.15.0" "dry run prints minor hint"

printf '\nnegatives\n'
setup_case neg-untracked
printf 'junk\n' > "$CASE_DIR/junk.txt"
( cd "$CASE_DIR" && GH_LOG="$CASE_LOG" PATH="$TMP/bin:$PATH" ./scripts/bump.sh --kind tui --bump-type 3 --fast --push ) >/dev/null 2>&1
assert_eq "$?" "1" "untracked files rejected"

printf '\nmissing changelog base\n'
setup_case neg-base
git -C "$CASE_DIR" tag -d v0.15.1 >/dev/null
out="$TMP/out-neg-base.log"
( cd "$CASE_DIR" && GH_LOG="$CASE_LOG" PATH="$TMP/bin:$PATH" ./scripts/bump.sh --kind tui --bump-type 3 --fast --push ) > "$out" 2>&1
assert_eq "$?" "1" "missing patch base rejected"
assert_grep "$out" "cannot resolve the changelog base" "missing patch base message"

setup_case neg-changelog
printf '**Full Changelog**: https://example.com\n' >> "$CASE_DIR/.local/bump/release.md"
( cd "$CASE_DIR" && GH_LOG="$CASE_LOG" PATH="$TMP/bin:$PATH" ./scripts/bump.sh --kind tui --bump-type 3 --fast --push ) >/dev/null 2>&1
assert_eq "$?" "1" "release.md with Full Changelog rejected"

setup_case neg-sentinel
printf '<!-- bump: fill in -->\n' > "$CASE_DIR/.local/bump/commit.md"
( cd "$CASE_DIR" && GH_LOG="$CASE_LOG" PATH="$TMP/bin:$PATH" ./scripts/bump.sh --kind tui --bump-type 3 --fast --push ) >/dev/null 2>&1
assert_eq "$?" "1" "template sentinel rejected"

setup_case neg-protocol
( cd "$CASE_DIR" && GH_LOG="$CASE_LOG" PATH="$TMP/bin:$PATH" ./scripts/bump.sh --kind protocol --bump-type 3 --fast --push ) >/dev/null 2>&1
assert_eq "$?" "1" "protocol without --protocol rejected"

setup_case neg-packaging
printf '\n' >> "$CASE_DIR/src/main.rs"
( cd "$CASE_DIR" && GH_LOG="$CASE_LOG" PATH="$TMP/bin:$PATH" ./scripts/bump.sh --kind packaging --bump-type 4 --fast --push ) >/dev/null 2>&1
assert_eq "$?" "1" "packaging with source changes rejected"

setup_case neg-missing
rm -f "$CASE_DIR/.local/bump/commit.md"
( cd "$CASE_DIR" && GH_LOG="$CASE_LOG" PATH="$TMP/bin:$PATH" ./scripts/bump.sh --kind tui --bump-type 3 --fast --push ) >/dev/null 2>&1
assert_eq "$?" "1" "missing commit.md rejected"
assert_grep "$CASE_DIR/.local/bump/commit.md" "bump: fill in" "commit template created"

printf '\n'
if [ "$fail" -ne 0 ]; then
    echo "bump tests FAILED" >&2
    exit 1
fi
echo "bump tests passed"
