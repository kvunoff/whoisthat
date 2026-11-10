#!/bin/sh
# Tests for scripts/cgroup-run with stubbed sudo tooling. No root required.
set -eu

here=$(cd -- "$(dirname -- "$0")" && pwd)
helper=$here/cgroup-run

root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT

bin="$root/bin"
mkdir -p "$bin"
log="$root/calls.log"
for name in setpriv setsid; do
    cat > "$bin/$name" <<EOF
#!/bin/sh
printf '%s %s\n' "$name" "\$*" >> "$log"
exit 0
EOF
    chmod +x "$bin/$name"
done
PATH="$bin:$PATH"
export PATH

export WHOISTHAT_TEST_MODE=1
export WHOISTHAT_CGROUP_ROOT="$root/cg"
mkdir -p "$WHOISTHAT_CGROUP_ROOT/whoisthat-1000"
: > "$WHOISTHAT_CGROUP_ROOT/whoisthat-1000/cgroup.procs"

uid=$(id -u)
[ "$uid" -ge 1000 ] || skip=yes
fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "ok    $*"; }

if [ "${skip:-}" = yes ]; then
    echo "skip: needs a normal user (uid >= 1000)"
    exit 0
fi

# The fixture cgroup is named after this user only when uid == 1000; rename it.
if [ "$uid" != 1000 ]; then
    mv "$WHOISTHAT_CGROUP_ROOT/whoisthat-1000" "$WHOISTHAT_CGROUP_ROOT/whoisthat-$uid"
fi
cg="$WHOISTHAT_CGROUP_ROOT/whoisthat-$uid"
: > "$log"

SUDO_UID=$uid "$helper" "$uid" -- /bin/echo hi || fail "foreground run failed"
grep -q "^setpriv --reuid 0 --regid 0 --clear-groups true$" "$log" || fail "setpriv capability probe missing"
grep -q "^setpriv --reuid $uid --regid .* --init-groups -- /bin/echo hi$" "$log" || fail "wrong setpriv args"
[ -s "$cg/cgroup.procs" ] || fail "helper did not move itself into the cgroup"
pass "foreground run migrates and drops to the user"

: > "$log"
SUDO_UID=$uid "$helper" "$uid" --detach -- /bin/sleep 5 || fail "detach run failed"
sleep 0.3
grep -q "^setsid setpriv --reuid $uid" "$log" || fail "detach did not use setsid"
pass "detach run uses setsid"

if SUDO_UID=$((uid + 1)) "$helper" "$uid" -- /bin/true >/dev/null 2>&1; then
    fail "uid mismatch must be refused"
fi
if "$helper" "$uid" -- /bin/true >/dev/null 2>&1; then
    fail "missing SUDO_UID must be refused"
fi
for bad in 0 999 abc; do
    if SUDO_UID=$bad "$helper" "$bad" -- /bin/true >/dev/null 2>&1; then
        fail "uid $bad must be refused"
    fi
done
pass "caller and target uid are validated"

rm -rf "$cg"
if SUDO_UID=$uid "$helper" "$uid" -- /bin/true >/dev/null 2>&1; then
    fail "missing cgroup must be refused"
fi
pass "missing cgroup gives an actionable error"

echo "cgroup-run tests passed"
