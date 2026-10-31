#!/usr/bin/env bash
#
# bump.sh — one-shot release driver for WhoisThat.
#
# It encodes the release policy from .local/UPDATE.md as a single validated
# command: bump versions, commit, tag, push and update GitHub releases.
# Version mirrors are written by scripts/check-versions.sh, which also verifies
# itself after every write, so this script never edits version files directly.
#
# Texts the agent/human fills before running:
#   .local/bump/commit.md    first line = commit summary, rest = body
#   .local/bump/release.md   release notes (bullets/prose)
# After a successful run with --push the texts move to *.md.backup and the
# working files are reset to an empty template.
#
# Kinds (see UPDATE.md):
#   tui          TUI-only release: bump TUI, pkgver, pkgrel=1, new v* tag
#   core         core fix, TUI unchanged: new core-v*, pkgrel+1, move v*
#   core-bundle  core + TUI: new core-v*, new v* tag, pkgrel=1
#   protocol     breaking IPC: core-bundle + protocol.txt/protocol.rs
#   packaging    pkgrel only: no new tag, move the existing v*, edit its notes
#
# Usage:
#   ./scripts/bump.sh --kind tui --bump-type 2
#   ./scripts/bump.sh --kind core --bump-type 3
#   ./scripts/bump.sh --kind core-bundle --bump-type 3 [--tui-bump-type 3]
#   ./scripts/bump.sh --kind protocol --bump-type 3 --protocol 4
#   ./scripts/bump.sh --kind packaging --bump-type 4
#   ./scripts/bump.sh --kind tui --bump-type 2 --dry-run
#
# Flags:
#   --bump-type 1|major  2|minor  3|patch  4|pkgrel
#   --protocol N         new IPC epoch (kind protocol only)
#   --dry-run            print the plan and diffs, write nothing
#   --fast               skip fmt/clippy/tests (check-versions always runs)
#   --push               push main/tags and create/edit GitHub releases
#   --no-release         with --push: push only, skip gh release
#   --allow-source       allow kind packaging with src/** or core/core/** changes
#   --allow-untracked    allow committing untracked files (default: refuse)
#   --branch NAME        branch to release from (default: main)
#
# Without --push everything stays local and the .md files are kept, so the
# printed commands can be run by hand.

set -uo pipefail

export LC_ALL=C

ROOT=$(cd "$(dirname "$0")/.." && pwd) || exit 1
cd "$ROOT" || exit 1

BUMP_DIR=".local/bump"
COMMIT_FILE="$BUMP_DIR/commit.md"
RELEASE_FILE="$BUMP_DIR/release.md"
SENTINEL="bump: fill in"

KIND=""
LEVEL=""
TUI_LEVEL=""
PROTOCOL_NEW=""
DRY=0
FAST=0
PUSH=0
NO_RELEASE=0
ALLOW_SOURCE=0
ALLOW_UNTRACKED=0
BRANCH=main

SNAPS=()

die()  { printf 'bump: %s\n' "$*" >&2; exit 1; }
warn() { printf 'bump: warning: %s\n' "$*" >&2; }
info() { printf 'bump: %s\n' "$*"; }
step() { printf '\n== %s ==\n' "$*"; }

usage() {
    cat <<'EOF'
Release driver for WhoisThat.

Usage:
  ./scripts/bump.sh --kind tui --bump-type 2
  ./scripts/bump.sh --kind core --bump-type 3
  ./scripts/bump.sh --kind core-bundle --bump-type 3 [--tui-bump-type 3]
  ./scripts/bump.sh --kind protocol --bump-type 3 --protocol 4
  ./scripts/bump.sh --kind packaging --bump-type 4

Kinds: tui, core, core-bundle, protocol, packaging.
Bump types: 1/major, 2/minor, 3/patch, 4/pkgrel.

Options:
  --bump-type TYPE       major|minor|patch|pkgrel (or 1..4)
  --tui-bump-type TYPE   TUI level for core-bundle/protocol (default: --bump-type)
  --protocol N           new IPC epoch (kind protocol only)
  --dry-run              print the plan and diffs, write nothing
  --fast                 skip fmt/clippy/tests
  --push                 push main/tags and create/edit GitHub releases
  --no-release           with --push: push only, skip gh release
  --allow-source         allow kind packaging with src/** or core/core/** changes
  --allow-untracked      allow committing untracked files (default: refuse)
  --branch NAME          branch to release from (default: main)
  -h, --help             show this help

Texts are read from .local/bump/commit.md and .local/bump/release.md.
EOF
}

tag_exists_local()  { git rev-parse --verify --quiet "refs/tags/$1" >/dev/null; }
tag_exists_remote() { [ -n "$(git ls-remote --tags --refs origin "refs/tags/$1" 2>/dev/null)" ]; }

semver_ok() { printf '%s' "$1" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; }
uint_ok()   { printf '%s' "$1" | grep -Eq '^[0-9]+$'; }

bump_semver() {
    local v=$1 level=$2 major minor patch
    IFS=. read -r major minor patch <<< "$v"
    case "$level" in
        major) printf '%s.0.0\n' "$((major + 1))" ;;
        minor) printf '%s.%s.0\n' "$major" "$((minor + 1))" ;;
        patch) printf '%s.%s.%s\n' "$major" "$minor" "$((patch + 1))" ;;
        *)     return 1 ;;
    esac
}

normalize_level() {
    case "$1" in
        1|major) echo major ;;
        2|minor) echo minor ;;
        3|patch) echo patch ;;
        4|pkgrel) echo pkgrel ;;
        *) return 1 ;;
    esac
}

# --- arguments ---------------------------------------------------------------

while [ $# -gt 0 ]; do
    case "$1" in
        --kind)          KIND="${2:-}"; shift 2 ;;
        --bump-type)     LEVEL="$(normalize_level "${2:-}")" || die "bad --bump-type: ${2:-}"; shift 2 ;;
        --tui-bump-type) TUI_LEVEL="$(normalize_level "${2:-}")" || die "bad --tui-bump-type: ${2:-}"; shift 2 ;;
        --protocol)      PROTOCOL_NEW="${2:-}"; shift 2 ;;
        --branch)        BRANCH="${2:-}"; shift 2 ;;
        --dry-run)       DRY=1; shift ;;
        --fast)          FAST=1; shift ;;
        --push)          PUSH=1; shift ;;
        --no-release)    NO_RELEASE=1; shift ;;
        --allow-source)  ALLOW_SOURCE=1; shift ;;
        --allow-untracked) ALLOW_UNTRACKED=1; shift ;;
        -h|--help)       usage; exit 0 ;;
        *)               die "unknown argument: $1" ;;
    esac
done

if [ -z "$LEVEL" ] && [ "$KIND" = packaging ]; then
    LEVEL=pkgrel
fi
[ -n "$LEVEL" ] || die "--bump-type is required"

case "$KIND" in
    tui|core|core-bundle|protocol|packaging) ;;
    "") die "--kind is required (tui, core, core-bundle, protocol, packaging)" ;;
    *)  die "unknown --kind: $KIND" ;;
esac

if [ "$LEVEL" = pkgrel ]; then
    [ "$KIND" = packaging ] || die "--bump-type pkgrel only works with --kind packaging"
else
    [ "$KIND" != packaging ] || die "--kind packaging needs --bump-type pkgrel"
fi

if [ "$KIND" = protocol ]; then
    [ -n "$PROTOCOL_NEW" ] || die "--kind protocol needs --protocol N"
else
    [ -z "$PROTOCOL_NEW" ] || die "--protocol is only valid with --kind protocol"
fi

case "$KIND" in
    core-bundle|protocol) TUI_LEVEL=${TUI_LEVEL:-$LEVEL} ;;
esac
if [ -n "$TUI_LEVEL" ]; then
    case "$TUI_LEVEL" in
        major|minor|patch) ;;
        *) die "--tui-bump-type must be major, minor or patch" ;;
    esac
fi

# --- current versions --------------------------------------------------------

tui_old=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n1)
core_old=$(sed -n 's/^const CoreVersion = "\([^"]*\)"$/\1/p' core/core/lib/version/version.go | head -n1)
protocol_old=$(tr -d '[:space:]' < core/core/lib/version/protocol.txt)
pkgrel_old=$(sed -n 's/^pkgrel=//p' pkg/aur/PKGBUILD)

for pair in "Cargo.toml:$tui_old" "CoreVersion:$core_old" "protocol.txt:$protocol_old" "pkgrel:$pkgrel_old"; do
    [ -n "${pair#*:}" ] || die "cannot read ${pair%%:*} from the tree"
done
semver_ok "$tui_old" || die "Cargo.toml version is not X.Y.Z"
semver_ok "$core_old" || die "CoreVersion is not X.Y.Z"
uint_ok "$protocol_old" || die "protocol.txt is not an integer"
uint_ok "$pkgrel_old" || die "PKGBUILD pkgrel is not an integer"

# --- target versions ---------------------------------------------------------

tui_new=$tui_old
core_new=$core_old
protocol_new=$protocol_old
pkgrel_new=$pkgrel_old

case "$KIND" in
    tui)
        tui_new=$(bump_semver "$tui_old" "$LEVEL") || die "cannot bump TUI"
        pkgrel_new=1
        ;;
    core)
        core_new=$(bump_semver "$core_old" "$LEVEL") || die "cannot bump core"
        pkgrel_new=$((pkgrel_old + 1))
        ;;
    core-bundle)
        core_new=$(bump_semver "$core_old" "$LEVEL") || die "cannot bump core"
        tui_new=$(bump_semver "$tui_old" "$TUI_LEVEL") || die "cannot bump TUI"
        pkgrel_new=1
        ;;
    protocol)
        core_new=$(bump_semver "$core_old" "$LEVEL") || die "cannot bump core"
        tui_new=$(bump_semver "$tui_old" "$TUI_LEVEL") || die "cannot bump TUI"
        pkgrel_new=1
        uint_ok "$PROTOCOL_NEW" || die "--protocol must be an integer"
        [ "$PROTOCOL_NEW" -gt "$protocol_old" ] || die "--protocol must be > $protocol_old"
        protocol_new=$PROTOCOL_NEW
        ;;
    packaging)
        pkgrel_new=$((pkgrel_old + 1))
        ;;
esac

v_new="v$tui_new"
core_new_tag="core-v$core_new"
v_old_tag="v$tui_old"
pkgrel_label="v${tui_old}-${pkgrel_new}"

# --- preflight ---------------------------------------------------------------

[ "$(git rev-parse --is-inside-work-tree 2>/dev/null)" = "true" ] || die "not a git repository"
[ "$(git rev-parse --abbrev-ref HEAD)" = "$BRANCH" ] || die "must run on branch $BRANCH (now: $(git rev-parse --abbrev-ref HEAD))"

for marker in MERGE_HEAD REBASE_HEAD CHERRY_PICK_HEAD rebase-merge rebase-apply; do
    [ -e "$(git rev-parse --git-dir)/$marker" ] && die "git operation in progress ($marker); finish it first"
done

git diff --cached --quiet || die "there are staged changes; commit or unstage them first"

untracked=$(git status --porcelain | sed -n 's/^?? //p')
if [ -n "$untracked" ] && [ "$ALLOW_UNTRACKED" -eq 0 ]; then
    die "untracked files present; commit/remove them or pass --allow-untracked: $(printf '%s\n' "$untracked" | head -n3 | tr '\n' ' ')"
fi

./scripts/check-versions.sh || die "tree is already inconsistent; fix it first"

if [ "$KIND" = packaging ] && [ "$ALLOW_SOURCE" -eq 0 ]; then
    source_changes=$(git status --porcelain | sed -e 's/^...//' -e 's/.* -> //' | grep -E '^(src/|core/core/)' || true)
    [ -z "$source_changes" ] || die "--kind packaging only touches packaging; source changed: $(echo "$source_changes" | head -n3 | tr '\n' ' ')(use --allow-source to override)"
fi

if [ "$KIND" = core ]; then
    tui_changes=$(git status --porcelain | sed -e 's/^...//' -e 's/.* -> //' | grep -E '^src/' || true)
    [ -z "$tui_changes" ] || warn "TUI sources changed; did you mean --kind core-bundle? They will land in the packaging commit"
fi

new_tags=()
case "$KIND" in
    tui)                 new_tags+=("$v_new") ;;
    core)                new_tags+=("$core_new_tag") ;;
    core-bundle|protocol) new_tags+=("$core_new_tag" "$v_new") ;;
esac

if [ "${#new_tags[@]}" -gt 0 ]; then
    for tag in "${new_tags[@]}"; do
        tag_exists_local "$tag" && die "tag $tag already exists locally"
        if [ "$PUSH" -eq 1 ] && tag_exists_remote "$tag"; then
            die "tag $tag already exists on origin"
        fi
    done
fi

case "$KIND" in
    core|packaging) tag_exists_local "$v_old_tag" || die "existing tag $v_old_tag not found; cannot move it" ;;
esac

if [ "$PUSH" -eq 1 ] && [ "$NO_RELEASE" -eq 0 ]; then
    gh auth status >/dev/null 2>&1 || die "gh is not authenticated; run gh auth login or pass --no-release"
    if [ "${#new_tags[@]}" -gt 0 ]; then
        for tag in "${new_tags[@]}"; do
            if gh release view "$tag" >/dev/null 2>&1; then
                die "GitHub release $tag already exists"
            fi
        done
    fi
    case "$KIND" in
        core|packaging)
            gh release view "$v_old_tag" >/dev/null 2>&1 || die "GitHub release $v_old_tag not found; cannot update it"
            ;;
    esac
fi

# --- texts -------------------------------------------------------------------

write_commit_template() {
    mkdir -p "$BUMP_DIR"
    printf '<!-- %s: first non-empty line = commit summary, rest = body -->\n' "$SENTINEL" > "$COMMIT_FILE"
}

write_release_template() {
    mkdir -p "$BUMP_DIR"
    printf '<!-- %s: release notes (bullets or prose) -->\n' "$SENTINEL" > "$RELEASE_FILE"
}

[ -f "$COMMIT_FILE" ]  || { write_commit_template;  die "created $COMMIT_FILE; fill it in and rerun"; }
[ -f "$RELEASE_FILE" ] || { write_release_template; die "created $RELEASE_FILE; fill it in and rerun"; }

grep -q "$SENTINEL" "$COMMIT_FILE"  && die "$COMMIT_FILE is still the template"
grep -q "$SENTINEL" "$RELEASE_FILE" && die "$RELEASE_FILE is still the template"

summary=$(awk 'NF { print; exit }' "$COMMIT_FILE")
[ -n "$summary" ] || die "$COMMIT_FILE is empty"
if printf '%s' "$summary" | grep -Eq '^(v|core-v)[0-9]+\.[0-9]+\.[0-9]+'; then
    die "commit.md must not start with a tag; the script adds it"
fi

first_line=$(awk 'NF { print NR; exit }' "$COMMIT_FILE")
commit_body=$(tail -n +$((first_line + 1)) "$COMMIT_FILE" | sed -e '/./,$!d')
release_text=$(sed -e '/./,$!d' "$RELEASE_FILE")
[ -n "$release_text" ] || die "$RELEASE_FILE is empty"
if printf '%s\n' "$release_text" | grep -qi 'Full Changelog'; then
    die "$RELEASE_FILE must not contain a Full Changelog line; the script adds it"
fi
if printf '%s\n' "$release_text" | grep -qi '^### What'; then
    die "$RELEASE_FILE must not contain a What's Changed header; the script adds it"
fi

# --- changelog base ----------------------------------------------------------

oldest_tag() {
    git tag --list "$1" --sort=version:refname | head -n1
}

resolve_base() {
    local prefix=$1 level=$2 old=$3
    local major minor patch candidate="" pattern=""
    IFS=. read -r major minor patch <<< "$old"
    case "$level" in
        patch)
            candidate="${prefix}${old}"
            ;;
        minor)
            candidate="${prefix}${major}.${minor}.0"
            pattern="${prefix}${major}.${minor}.*"
            ;;
        major)
            candidate="${prefix}${major}.0.0"
            pattern="${prefix}${major}.*"
            ;;
    esac

    if [ -n "$candidate" ] && tag_exists_local "$candidate"; then
        printf '%s\n' "$candidate"
        return 0
    fi
    if [ -n "$pattern" ]; then
        candidate=$(git tag --list "$pattern" --sort=version:refname | head -n1)
        if [ -n "$candidate" ]; then
            warn "changelog base $candidate (the exact $level base does not exist)"
            printf '%s\n' "$candidate"
            return 0
        fi
    fi
    if [ "$level" = major ] || [ "$level" = minor ]; then
        candidate=$(oldest_tag "${prefix}*")
        if [ -n "$candidate" ]; then
            warn "using the oldest tag $candidate as changelog base"
            printf '%s\n' "$candidate"
            return 0
        fi
    fi
    return 1
}

bundle_base=""
core_base=""

case "$KIND" in
    tui)
        bundle_base=$(resolve_base "v" "$LEVEL" "$tui_old") || die "cannot resolve the changelog base for $v_old_tag"
        ;;
    core-bundle|protocol)
        bundle_base=$(resolve_base "v" "$TUI_LEVEL" "$tui_old") || die "cannot resolve the changelog base for $v_old_tag"
        ;;
esac

case "$KIND" in
    core|core-bundle|protocol)
        if [ -z "$(git tag --list 'core-v*' | head -n1)" ]; then
            core_base="$v_old_tag"
            tag_exists_local "$core_base" || die "cannot resolve the first core changelog base ($core_base)"
        else
            core_base=$(resolve_base "core-v" "$LEVEL" "$core_old") || die "cannot resolve the changelog base for core-v$core_old"
        fi
        ;;
esac

# --- plan output -------------------------------------------------------------

info "kind:        $KIND"
info "bump:        $LEVEL${TUI_LEVEL:+ (tui: $TUI_LEVEL)}"
info "TUI:         $tui_old -> $tui_new"
info "core:        $core_old -> $core_new"
info "protocol:    $protocol_old -> $protocol_new"
info "pkgrel:      $pkgrel_old -> $pkgrel_new"
info "summary:     $summary"
[ -n "$bundle_base" ] && info "bundle base: $bundle_base"
[ -n "$core_base" ] && info "core base:   $core_base"

minor_range_hint() {
    local base=$1 label=$2
    step "minor release: $label notes must cover every change since $base"
    printf 'Committed changes:\n'
    git log --format='  %h %s' "$base..HEAD" 2>/dev/null | head -n 40
    printf 'Working tree vs %s:\n' "$base"
    git diff --stat "$base" 2>/dev/null | tail -n 20
}

bundle_minor=0
case "$KIND" in
    tui)                  [ "$LEVEL" = minor ] && bundle_minor=1 ;;
    core-bundle|protocol) [ "$TUI_LEVEL" = minor ] && bundle_minor=1 ;;
esac

if [ "$bundle_minor" -eq 1 ] && [ -n "$bundle_base" ]; then
    minor_range_hint "$bundle_base" "bundle"
fi
if [ -n "$core_base" ] && [ "$LEVEL" = minor ]; then
    info "core notes are generated over $core_base..$core_new_tag and already cover the whole minor cycle"
fi

if [ "$DRY" -eq 1 ]; then
    step "version diff (nothing is written)"
    case "$KIND" in
        core|core-bundle|protocol)
            ./scripts/check-versions.sh --set "tui=$tui_old" "core=$core_new" "protocol=$protocol_new" "pkgrel=$pkgrel_new" --dry-run || die "dry-run failed"
            if [ "$KIND" != core ]; then
                ./scripts/check-versions.sh --set "tui=$tui_new" "core=$core_new" "protocol=$protocol_new" "pkgrel=$pkgrel_new" --dry-run || die "dry-run failed"
            fi
            ;;
        *)
            ./scripts/check-versions.sh --set "tui=$tui_new" "core=$core_new" "protocol=$protocol_new" "pkgrel=$pkgrel_new" --dry-run || die "dry-run failed"
            ;;
    esac
    step "plan"
    case "$KIND" in
        tui)
            echo "  commit   $v_new: $summary"
            echo "  tag      $v_new (new)"
            echo "  release  create $v_new"
            ;;
        core)
            echo "  commit   $core_new_tag: $summary"
            echo "  tag      $core_new_tag (new)"
            echo "  commit   $pkgrel_label: $summary"
            echo "  tag      $v_old_tag (force-move)"
            echo "  release  create $core_new_tag, edit $v_old_tag"
            ;;
        core-bundle|protocol)
            echo "  commit   $core_new_tag: $summary"
            echo "  tag      $core_new_tag (new)"
            echo "  commit   $v_new: $summary"
            echo "  tag      $v_new (new)"
            echo "  release  create $core_new_tag, then $v_new"
            ;;
        packaging)
            echo "  commit   $pkgrel_label: $summary"
            echo "  tag      $v_old_tag (force-move, no new tag)"
            echo "  release  edit $v_old_tag (no new release)"
            ;;
    esac
    exit 0
fi

# --- pre-release checks ------------------------------------------------------

if [ "$FAST" -eq 0 ]; then
    step "pre-release checks"
    cargo fmt --all --check        || die "cargo fmt failed"
    cargo clippy --all-targets --no-deps || die "cargo clippy failed"
    cargo test                     || die "cargo test failed"
    ( cd core/core && go vet ./... && go build ./... && go test ./... ) || die "go checks failed"
    git diff --check               || die "whitespace errors (git diff --check)"
else
    step "checks skipped (--fast)"
fi

# --- apply versions ----------------------------------------------------------

snapshot_files_from_dryrun() {
    local dry=$1 snap=$2 f
    while IFS= read -r f; do
        [ -n "$f" ] || continue
        mkdir -p "$snap/$(dirname "$f")"
        cp -p "$f" "$snap/$f"
    done < <(sed -n 's/^  would update //p' "$dry")
}

restore_snapshot() {
    local snap=$1 f
    while IFS= read -r -d '' f; do
        cp -p "$snap/$f" "$f"
    done < <(cd "$snap" && find . -type f -print0)
}

rollback_all() {
    local i
    for ((i = ${#SNAPS[@]} - 1; i >= 0; i--)); do
        restore_snapshot "${SNAPS[i]}"
        rm -rf "${SNAPS[i]}"
    done
    SNAPS=()
}

abort_bump() {
    rollback_all
    die "$* (version files restored)"
}

apply_versions() {
    local t=$1 c=$2 p=$3 r=$4 dry snap
    dry=$(mktemp) || die "mktemp failed"
    if ! ./scripts/check-versions.sh --set "tui=$t" "core=$c" "protocol=$p" "pkgrel=$r" --dry-run > "$dry" 2>&1; then
        cat "$dry" >&2
        rm -f "$dry"
        abort_bump "version dry-run failed"
    fi
    snap=$(mktemp -d) || die "mktemp failed"
    snapshot_files_from_dryrun "$dry" "$snap"
    if ! ./scripts/check-versions.sh --set "tui=$t" "core=$c" "protocol=$p" "pkgrel=$r"; then
        restore_snapshot "$snap"
        rm -rf "$snap" "$dry"
        abort_bump "version update failed"
    fi
    SNAPS+=("$snap")
    rm -f "$dry"
}

step "applying versions"
case "$KIND" in
    core)
        apply_versions "$tui_old" "$core_new" "$protocol_old" "$pkgrel_old"
        apply_versions "$tui_old" "$core_new" "$protocol_old" "$pkgrel_new"
        ;;
    core-bundle|protocol)
        apply_versions "$tui_old" "$core_new" "$protocol_new" "$pkgrel_old"
        apply_versions "$tui_new" "$core_new" "$protocol_new" "$pkgrel_new"
        ;;
    *)
        apply_versions "$tui_new" "$core_new" "$protocol_new" "$pkgrel_new"
        ;;
esac

# Regenerate AUR metadata when the tool exists; check-versions already kept
# pkgver/pkgrel/source in sync, but a manual PKGBUILD edit (deps) needs this.
if command -v makepkg >/dev/null 2>&1; then
    ( cd pkg/aur && makepkg --printsrcinfo > .SRCINFO ) || abort_bump "makepkg --printsrcinfo failed"
else
    warn "makepkg not found; .SRCINFO was updated for version fields only"
fi

step "post-bump checks"
./scripts/check-versions.sh || abort_bump "version check failed after the bump"
if [ "$FAST" -eq 0 ]; then
    cargo test --quiet protocol_version_matches_shared_file || abort_bump "protocol mirror test failed"
    ( cd core/core && go build ./... ) || abort_bump "go build failed"
fi

# --- commit and tag ----------------------------------------------------------

commit_now() {
    local label=$1
    if [ -n "$commit_body" ]; then
        git commit -m "$label: $summary" -m "$commit_body" || die "commit failed"
    else
        git commit -m "$label: $summary" || die "commit failed"
    fi
}

commit_core() {
    local label=$1
    git add -- core/core
    git add -- src/core_client/protocol.rs 2>/dev/null || true
    [ -n "$(git diff --cached --name-only)" ] || die "nothing to commit for $label"
    commit_now "$label"
}

commit_all() {
    local label=$1
    git add -A
    [ -n "$(git diff --cached --name-only)" ] || die "nothing to commit for $label"
    commit_now "$label"
}

tag_new()      { git tag -a "$1" -m "$1: $summary" || die "cannot create tag $1"; }
tag_move()     { git tag -f -a "$1" -m "$2: $summary" || die "cannot move tag $1"; }

step "committing"
case "$KIND" in
    tui)
        commit_all "$v_new"
        tag_new "$v_new"
        ;;
    packaging)
        commit_all "$pkgrel_label"
        tag_move "$v_old_tag" "$pkgrel_label"
        ;;
    core)
        commit_core "$core_new_tag"
        tag_new "$core_new_tag"
        commit_all "$pkgrel_label"
        tag_move "$v_old_tag" "$pkgrel_label"
        ;;
    core-bundle|protocol)
        commit_core "$core_new_tag"
        tag_new "$core_new_tag"
        commit_all "$v_new"
        tag_new "$v_new"
        ;;
esac

# From here on the tree is committed; do not roll back version files silently.
for snap in "${SNAPS[@]}"; do rm -rf "$snap"; done
SNAPS=()

# --- publish -----------------------------------------------------------------

run_push() {
    [ "$PUSH" -eq 1 ] || return 0

    step "pushing"
    git push origin "$BRANCH" || die "git push $BRANCH failed"
    case "$KIND" in
        core|core-bundle|protocol)
            git push origin "$core_new_tag" || die "push $core_new_tag failed"
            ;;
    esac
    case "$KIND" in
        core|packaging)
            git push --force origin "$v_old_tag" || die "force-push $v_old_tag failed"
            ;;
        *)
            git push origin "$v_new" || die "push $v_new failed"
            ;;
    esac
}

notes_file() {
    local out=$1
    shift
    {
        printf '### What'\''s Changed\n\n'
        printf '%s\n\n' "$release_text"
        printf '**Full Changelog**: https://github.com/kvunoff/whoisthat/compare/%s...%s\n' "$1" "$2"
    } > "$out"
}

pkgrel_notes_file() {
    local out=$1 old_body
    old_body=$(gh release view "$v_old_tag" --json body -q .body) || die "cannot read release $v_old_tag; pass --no-release"
    {
        printf '## Package revision %s\n\n' "$pkgrel_new"
        printf '%s\n\n' "$release_text"
        printf '%s\n' "$old_body"
    } > "$out"
}

run_releases() {
    [ "$PUSH" -eq 1 ] || return 0
    [ "$NO_RELEASE" -eq 0 ] || { info "releases skipped (--no-release)"; return 0; }

    step "GitHub releases"
    local tmp
    tmp=$(mktemp) || die "mktemp failed"

    case "$KIND" in
        core|core-bundle|protocol)
            local core_args=(--title "$core_new_tag: $summary" --generate-notes)
            [ -n "$core_base" ] && core_args+=(--notes-start-tag "$core_base")
            gh release create "$core_new_tag" "${core_args[@]}" \
                || die "gh release create $core_new_tag failed"
            ;;
    esac

    case "$KIND" in
        tui|core-bundle|protocol)
            notes_file "$tmp" "$bundle_base" "$v_new"
            gh release create "$v_new" \
                --title "$v_new: $summary" \
                --notes-file "$tmp" \
                || die "gh release create $v_new failed"
            ;;
        core|packaging)
            pkgrel_notes_file "$tmp"
            gh release edit "$v_old_tag" --notes-file "$tmp" \
                || die "gh release edit $v_old_tag failed"
            ;;
    esac
    rm -f "$tmp"
}

run_push
run_releases

# --- rotate texts ------------------------------------------------------------

if [ "$PUSH" -eq 1 ] && [ "$NO_RELEASE" -eq 0 ]; then
    mv "$COMMIT_FILE" "$COMMIT_FILE.backup" || die "cannot back up $COMMIT_FILE"
    mv "$RELEASE_FILE" "$RELEASE_FILE.backup" || die "cannot back up $RELEASE_FILE"
    write_commit_template
    write_release_template
    info "texts moved to *.md.backup; working files reset"
elif [ "$PUSH" -eq 1 ]; then
    info "texts kept (--no-release)"
else
    info "texts kept; run git push / gh release manually when ready"
fi

step "done"
info "TUI $tui_old -> $tui_new"
info "core $core_old -> $core_new"
info "protocol $protocol_old -> $protocol_new"
info "pkgrel $pkgrel_old -> $pkgrel_new"
printf '\nVerify after publishing:\n'
printf '  gh release list --limit 4\n'
printf '  git ls-remote --tags --sort=-version:refname origin "refs/tags/v*" | head -3\n'
