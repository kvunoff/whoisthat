#!/bin/sh
# Tests for scripts/run-core with a fake core binary. No root required.
set -eu

here=$(cd -- "$(dirname -- "$0")" && pwd)
wrapper=$here/run-core

root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT

home="$root/home"
mkdir -p "$home"
export HOME="$home"
export WHOISTHAT_TEST_MODE=1
export WHOISTHAT_RUN_ROOT="$root/run"
export WHOISTHAT_CORE="$root/fake-core"
export MODE_FILE="$root/mode"
export RUNS="$root/runs"
export PIDS="$root/pids"

mkdir -p "$WHOISTHAT_RUN_ROOT"
uid=$(id -u)
test_uid=$uid
if [ "$uid" -eq 0 ]; then
    test_uid=1000
fi
export WHOISTHAT_TEST_UID=$test_uid
mkdir -p "$WHOISTHAT_RUN_ROOT/$test_uid"
chmod 0700 "$WHOISTHAT_RUN_ROOT/$test_uid"
if [ "$uid" -eq 0 ]; then
    chown "$test_uid" "$WHOISTHAT_RUN_ROOT/$test_uid"
fi

cat > "$WHOISTHAT_CORE" <<'EOF'
#!/bin/sh
echo x >> "$RUNS"
echo $$ >> "$PIDS"
case "$(cat "$MODE_FILE")" in
    clean) exit 0 ;;
    crash) sleep 1; exit 1 ;;
    hold) sleep 2; exit 0 ;;
    hang) sleep 300 ;;
esac
exit 0
EOF
chmod +x "$WHOISTHAT_CORE"
: > "$RUNS"
: > "$PIDS"

conf="$home/.config/whoisthat"
mkdir -p "$conf"
pidfile="$conf/core.pid"

fail() {
    echo "FAIL: $*" >&2
    if [ -f "$root/wrapper.log" ]; then
        sed 's/^/wrapper: /' "$root/wrapper.log" >&2
    fi
    if [ -x "$wrapper" ]; then
        echo "debug: run dirs" >&2
        ls -la "$conf" "$WHOISTHAT_RUN_ROOT/$test_uid" >&2 || true
        echo "debug: runs" >&2
        cat "$RUNS" >&2 || true
        echo "debug: wrapper trace" >&2
        timeout 3 sh -x "$wrapper" "$HOME" >&2 2>&1 || true
    fi
    exit 1
}
pass() { echo "ok    $*"; }

wait_for() { # timeout-in-tenths command...
    timeout=$1
    shift
    i=0
    while [ "$i" -lt "$timeout" ]; do
        if "$@"; then
            return 0
        fi
        sleep 0.2
        i=$((i + 1))
    done
    return 1
}

has_pidfile() { [ -s "$pidfile" ]; }
no_pidfile() { [ ! -s "$pidfile" ]; }
runs() { wc -l < "$RUNS" | tr -d ' '; }
runs_gt() { [ "$(runs)" -gt "$1" ]; }

start_wrapper() {
    "$wrapper" "$HOME" 2> "$root/wrapper.log" &
    wrapper_pid=$!
}

stop_wrapper() {
    kill -TERM "$wrapper_pid" 2>/dev/null || true
    wait "$wrapper_pid" 2>/dev/null || true
}

echo crash > "$MODE_FILE"
: > "$conf/autostart"
start_wrapper
wait_for 40 runs_gt 0 || fail "boot did not start the core"
first=$(tail -n 1 "$PIDS")
pass "boot starts the core when autostart exists"

kill -9 "$first" 2>/dev/null || true
wait_for 40 runs_gt 1 || fail "core was not restarted after a crash"
second=$(tail -n 1 "$PIDS")
[ "$second" != "$first" ] || fail "PID did not change after crash restart"
pass "crash restarts the core"

# core stop: no restart until a new request, even with autostart present.
rm -f "$conf/core-run"
: > "$conf/core-stop"
wait_for 40 no_pidfile || fail "core-stop did not stop the core"
sleep 2.5
no_pidfile || fail "wrapper restarted the core despite core-stop"
pass "core-stop keeps the core down"

rm -f "$conf/core-stop"
: > "$conf/core-start"
before=$(runs)
wait_for 40 runs_gt "$before" || fail "core-start did not start the core"
pass "core-start starts the core again"

# Wrapper TERM must take the child down with it.
pid=$(tail -n 1 "$PIDS")
stop_wrapper
sleep 0.5
if [ -d "/proc/$pid" ] && [ "$(cat "/proc/$pid/comm" 2>/dev/null)" = "sh" ]; then
    fail "core $pid survived wrapper TERM"
fi
pass "TERM stops the wrapper and the core"

# Clean exit pauses; a restart request then starts a fresh process.
echo clean > "$MODE_FILE"
: > "$conf/core-run"
rm -f "$conf/core-start" "$conf/core-stop"
before=$(runs)
start_wrapper
wait_for 40 runs_gt "$before" || fail "clean-exit run never started"
sleep 3
after=$(runs)
[ "$after" -le $((before + 1)) ] || fail "clean exit restarted in a loop ($before -> $after)"
no_pidfile || fail "clean exit left a pidfile"
: > "$conf/core-restart"
wait_for 40 runs_gt "$after" || fail "core-restart did not start the core"
pass "clean exit pauses; core-restart starts again"

: > "$conf/core-stop"
wait_for 40 no_pidfile || fail "cleanup stop failed"
stop_wrapper
grep -q "missing/invalid" "$root/wrapper.log" && fail "unexpected runtime-dir error: $(cat "$root/wrapper.log")"
echo "run-core tests passed"
