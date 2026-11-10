#!/bin/sh
# Tests for scripts/run-runner with fake cgroup/setpriv/runner. No root required.
set -eu

here=$(cd -- "$(dirname -- "$0")" && pwd)
wrapper=$here/run-runner

root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT

uid=$(id -u)
[ "$uid" -ge 1000 ] || { echo "skip: needs a normal user"; exit 0; }
gid=$(id -g)
home="$root/home"
mkdir -p "$home"
export WHOISTHAT_TEST_MODE=1
export WHOISTHAT_CGROUP_ROOT="$root/cgroup"
export WHOISTHAT_RUNNER_BIN="$root/fake-runner"
mkdir -p "$WHOISTHAT_CGROUP_ROOT/whoisthat-$uid"
: > "$WHOISTHAT_CGROUP_ROOT/whoisthat-$uid/cgroup.procs"
socket_dir="$root/run/$uid/whoisthat"
mkdir -p "$socket_dir"
socket="$socket_dir/runner.sock"

bin="$root/bin"
mkdir -p "$bin"
log="$root/log"
cat > "$bin/setpriv" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >> "$log"
exit 0
EOF
chmod +x "$bin/setpriv"
cat > "$WHOISTHAT_RUNNER_BIN" <<EOF
#!/bin/sh
printf '%s\n' "runner \$*" >> "$log"
exit 0
EOF
chmod +x "$WHOISTHAT_RUNNER_BIN"
PATH="$bin:$PATH"
export PATH

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "ok    $*"; }

"$wrapper" "$uid" "$gid" "$home" "$socket" 2> "$root/wrap.log" &
wrapper_pid=$!
trap 'kill "$wrapper_pid" 2>/dev/null || true; rm -rf "$root"' EXIT

i=0
while [ "$i" -lt 50 ] && [ ! -s "$log" ]; do
    sleep 0.1
    i=$((i + 1))
done
[ -s "$log" ] || fail "wrapper never started the runner"
grep -q -- "--reuid $uid --regid $gid --init-groups" "$log" || fail "wrong setpriv args: $(cat "$log")"
grep -q -- "$WHOISTHAT_RUNNER_BIN --uid $uid --socket $socket" "$log" || fail "wrong runner args: $(cat "$log")"
grep -Eq '^[0-9]+$' "$WHOISTHAT_CGROUP_ROOT/whoisthat-$uid/cgroup.procs" \
    || fail "child pid was not written to cgroup.procs"
pass "migrates the runner child and drops to the target uid"

kill -TERM "$wrapper_pid" 2>/dev/null || true
wait "$wrapper_pid" 2>/dev/null || true
pass "TERM stops the wrapper"

if "$wrapper" "$((uid + 1))" "$gid" "$home" "$socket" >/dev/null 2>&1; then
    fail "uid mismatch must be refused"
fi
if "$wrapper" "$uid" "$((gid + 1))" "$home" "$socket" >/dev/null 2>&1; then
    fail "gid mismatch must be refused"
fi
if "$wrapper" 0 0 "$home" "$socket" >/dev/null 2>&1; then
    fail "uid 0 must be refused"
fi
if "$wrapper" "$uid" "$gid" "relative" "$socket" >/dev/null 2>&1; then
    fail "relative home must be refused"
fi
pass "validates uid/gid/home"

rm -rf "$WHOISTHAT_CGROUP_ROOT/whoisthat-$uid"
"$wrapper" "$uid" "$gid" "$home" "$socket" 2> "$root/missing.log" &
wrapper_pid=$!
sleep 1
grep -q "cgroup-setup" "$root/missing.log" || fail "missing cgroup warning absent"
kill -TERM "$wrapper_pid" 2>/dev/null || true
wait "$wrapper_pid" 2>/dev/null || true
pass "missing cgroup is reported"

echo "run-runner tests passed"
