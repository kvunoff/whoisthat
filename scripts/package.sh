#!/usr/bin/env bash
# Publish a release to one distribution channel.
#
#   ./scripts/package.sh aur     [--tag vX.Y.Z] [--publish] [--dry-run]
#   ./scripts/package.sh aur-bin [...]
#   ./scripts/package.sh ppa     [... --series noble,resolute]
#
# Without --publish the target only builds its metadata in a temporary
# directory, so the same command is safe locally and in CI.

set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)

# shellcheck source=scripts/package/_common.sh
source "$ROOT/scripts/package/_common.sh"

TARGET=${1:-}
if [ -z "$TARGET" ]; then
    usage >&2
    exit 2
fi
shift

if [ ! -f "$ROOT/scripts/package/${TARGET}.sh" ]; then
    printf 'error: unknown target: %s\n' "$TARGET" >&2
    exit 2
fi

parse_args "$@"

# shellcheck disable=SC1090
source "$ROOT/scripts/package/${TARGET}.sh"

for hook in prepare build publish verify; do
    declare -F "$hook" >/dev/null || die "$TARGET.sh must define $hook"
done

log "prepare"
prepare

log "build"
build

if [ "$PUBLISH" = 1 ]; then
    log "publish"
    publish
fi

log "verify"
verify
