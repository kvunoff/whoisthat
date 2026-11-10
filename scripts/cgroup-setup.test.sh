#!/bin/sh
# Tests for scripts/cgroup-setup. No root required: every path is redirected
# with WHOISTHAT_TEST_MODE and chown is stubbed.
set -eu

here=$(cd -- "$(dirname -- "$0")" && pwd)
setup=$here/cgroup-setup

root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT

bin="$root/bin"
mkdir -p "$bin"
chown_log="$root/chown.log"
cat > "$bin/chown" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >> "$chown_log"
exit 0
EOF
chmod +x "$bin/chown"
PATH="$bin:$PATH"
export PATH

export WHOISTHAT_TEST_MODE=1
export WHOISTHAT_CGROUP_ROOT="$root/cgroup"
export WHOISTHAT_RUN_ROOT="$root/run"
mkdir -p "$WHOISTHAT_CGROUP_ROOT"
: > "$WHOISTHAT_CGROUP_ROOT/cgroup.controllers"

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "ok    $*"; }

uid=$(id -u)
[ "$uid" -ge 1000 ] || fail "this test must run as a normal user (uid >= 1000)"

# A pre-existing root-owned cgroup with root-owned files is the real-world case.
mkdir -p "$WHOISTHAT_CGROUP_ROOT/whoisthat-$uid"
: > "$WHOISTHAT_CGROUP_ROOT/whoisthat-$uid/cgroup.procs"
: > "$WHOISTHAT_CGROUP_ROOT/whoisthat-$uid/cgroup.threads"

"$setup" --uid "$uid" || fail "create failed"
[ -d "$WHOISTHAT_CGROUP_ROOT/whoisthat-$uid" ] || fail "cgroup dir not created"
[ -d "$WHOISTHAT_RUN_ROOT/$uid" ] || fail "runtime dir not created"
[ "$(stat -c %a "$WHOISTHAT_RUN_ROOT/$uid")" = 700 ] || fail "runtime dir mode"
[ -d "$WHOISTHAT_RUN_ROOT/$uid/whoisthat" ] || fail "whoisthat dir not created"
[ "$(stat -c %a "$WHOISTHAT_RUN_ROOT/$uid/whoisthat")" = 700 ] || fail "whoisthat dir mode"
grep -q " $WHOISTHAT_CGROUP_ROOT/whoisthat-$uid\$" "$chown_log" || fail "cgroup dir not chowned"
grep -q " $WHOISTHAT_CGROUP_ROOT/whoisthat-$uid/cgroup.procs\$" "$chown_log" || fail "cgroup.procs not chowned"
grep -q " $WHOISTHAT_CGROUP_ROOT/whoisthat-$uid/cgroup.threads\$" "$chown_log" || fail "cgroup.threads not chowned"
pass "creates and chowns cgroup + runtime dir"

"$setup" --uid "$uid" || fail "second create must be idempotent"
pass "idempotent"

# Remove: known files go, unknown files and a still-live socket are reported.
mkdir -p "$WHOISTHAT_RUN_ROOT/$uid/whoisthat"
: > "$WHOISTHAT_RUN_ROOT/$uid/whoisthat/core.sock"
: > "$WHOISTHAT_RUN_ROOT/$uid/whoisthat/ipc.token"
"$setup" --remove --uid "$uid" || fail "remove failed"
[ ! -e "$WHOISTHAT_RUN_ROOT/$uid/whoisthat/core.sock" ] || fail "core.sock not removed"
[ ! -e "$WHOISTHAT_RUN_ROOT/$uid/whoisthat/ipc.token" ] || fail "ipc.token not removed"
[ ! -d "$WHOISTHAT_RUN_ROOT/$uid/whoisthat" ] || fail "whoisthat dir not removed"
[ ! -d "$WHOISTHAT_RUN_ROOT/$uid" ] || fail "runtime dir not removed"
pass "removes known files and empty dirs"

# Unknown file: keep it, warn, non-zero exit, never recurse.
mkdir -p "$WHOISTHAT_RUN_ROOT/$uid/whoisthat"
: > "$WHOISTHAT_RUN_ROOT/$uid/whoisthat/unknown"
if "$setup" --remove --uid "$uid" 2> "$root/remove.err"; then
    fail "remove must fail on a non-empty dir"
fi
grep -q "not empty" "$root/remove.err" || fail "missing not-empty warning"
[ -e "$WHOISTHAT_RUN_ROOT/$uid/whoisthat/unknown" ] || fail "unknown file must survive"
rm -rf "${WHOISTHAT_RUN_ROOT:?}/$uid"
pass "non-empty dir is a warning, not a silent success"

# --all-users also drops the global active marker.
mkdir -p "$WHOISTHAT_RUN_ROOT"
: > "$WHOISTHAT_RUN_ROOT/active"
"$setup" --remove --all-users >/dev/null 2>&1 || true
[ ! -e "$WHOISTHAT_RUN_ROOT/active" ] || fail "active marker not removed"
pass "--remove --all-users clears the active marker"

# Bad uid values are refused before touching anything.
for bad in 0 999 abc -1; do
    if "$setup" --uid "$bad" >/dev/null 2>&1; then
        fail "--uid $bad must be refused"
    fi
done
pass "rejects uid 0, <1000 and non-numeric"

# cgroup v1 (no controllers file) is a warning, not a crash.
rm -f "$WHOISTHAT_CGROUP_ROOT/cgroup.controllers"
if "$setup" --uid "$uid" 2> "$root/v1.err"; then
    fail "cgroup v1 must not report success"
fi
grep -q "cgroup v2 not mounted" "$root/v1.err" || fail "missing cgroup v1 warning"
pass "cgroup v1 reported clearly"

echo "cgroup-setup tests passed"
