#!/bin/sh
# Tests for scripts/install-init against fake init directories and stubbed
# service managers. No root required.
set -eu

here=$(cd -- "$(dirname -- "$0")" && pwd)
install_init=$here/install-init

root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT

lib="$root/lib"
mkdir -p "$lib/init"
cp -R "$here/../pkg/init/." "$lib/init/"
cp "$here/cgroup-setup" "$here/run-core" "$here/init-hook" "$lib/"

bin="$root/bin"
mkdir -p "$bin"
cmdlog="$root/commands.log"

stub() {
    name=$1
    cat > "$bin/$name" <<EOF
#!/bin/sh
echo "$name \$*" >> "$cmdlog"
exit 0
EOF
    chmod +x "$bin/$name"
}
stub rc-update
stub rc-service
stub dinitctl
PATH="$bin:$PATH"
export PATH

export WHOISTHAT_TEST_MODE=1
export WHOISTHAT_LIBDIR="$lib"
export WHOISTHAT_SYSDIR="$root/etc"
export WHOISTHAT_ETCDIR="$root/etc"
export WHOISTHAT_RUN_ROOT="$root/run"

user=$(id -un)
uid=$(id -u)
gid=$(id -g)

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "ok    $*"; }
run() { "$install_init" "$@"; }

export WHOISTHAT_INIT=openrc

run cgroup enable || fail "openrc cgroup enable failed"
hook="$root/etc/init.d/whoisthat-cgroup"
[ -x "$hook" ] || fail "openrc cgroup hook missing/not executable"
grep -q "init-hook" "$hook" || fail "hook does not call init-hook"
grep -q "rc-update add whoisthat-cgroup boot" "$cmdlog" || fail "cgroup not enabled"
pass "openrc cgroup hook installed and enabled"

run core enable "$user" "$root/home" || fail "openrc core enable failed"
service="$root/etc/init.d/whoisthat-core"
[ -x "$service" ] || fail "openrc core service missing"
grep -q "command_user=\"$uid:$gid\"" "$service" || fail "wrong command_user"
grep -q "command_args=\"$root/home\"" "$service" || fail "wrong home"
marker="$root/etc/whoisthat/init-user-$uid"
[ -f "$marker" ] || fail "per-uid marker missing"
[ "$(stat -c %a "$marker")" = 644 ] || fail "marker mode must be 0644"
grep -q "rc-update add whoisthat-core default" "$cmdlog" || fail "core not enabled"
grep -q "rc-service whoisthat-core start" "$cmdlog" || fail "core not started"
pass "openrc core service installed, enabled, started"

export XDG_CONFIG_HOME="$root/xdg-config"
run core enable "$user" "$root/home" || fail "XDG reinstall failed"
grep -q "export XDG_CONFIG_HOME=\"$root/xdg-config\"" "$service" || fail "XDG_CONFIG_HOME not exported"
grep -q "export XDG_CONFIG_HOME" "$service" || fail "XDG line missing"
unset XDG_CONFIG_HOME
pass "custom XDG dirs are written into the service"

if run core enable "$user" "$root/home with space" >/dev/null 2>&1; then
    fail "home with a space must be rejected"
fi
pass "unsafe home rejected"

run core disable "$user" || fail "openrc core disable failed"
[ ! -e "$service" ] || fail "service not removed on disable"
[ ! -e "$marker" ] || fail "marker not removed on disable"
pass "core disable removes service and marker"

run runner enable "$user" "$root/home" || fail "openrc runner enable failed"
runner_service="$root/etc/init.d/whoisthat-runner"
[ -x "$runner_service" ] || fail "runner service missing"
grep -q "command_args=\"$uid $gid $root/home /run/whoisthat/$uid/whoisthat/runner.sock\"" "$runner_service" \
    || fail "runner command_args wrong"
grep -q "rc-update add whoisthat-runner default" "$cmdlog" || fail "runner not enabled"
run runner disable "$user" || fail "runner disable failed"
[ ! -e "$runner_service" ] || fail "runner service survived disable"
pass "openrc runner service installed and removed"

rm -f "$cmdlog"
export WHOISTHAT_INIT=dinit
run cgroup enable || fail "dinit cgroup enable failed"
dinit_cgroup="$root/etc/dinit.d/whoisthat-cgroup"
grep -q "^type = scripted" "$dinit_cgroup" || fail "dinit cgroup must be scripted"
run core enable "$user" "$root/home" || fail "dinit core enable failed"
dinit_core="$root/etc/dinit.d/whoisthat-core"
grep -q "^type = process" "$dinit_core" || fail "dinit core must be a process"
grep -q "^run-as = $user" "$dinit_core" || fail "dinit run-as wrong"
grep -q "^restart = false" "$dinit_core" || fail "dinit must not restart-loop"
grep -q "dinitctl enable whoisthat-core" "$cmdlog" || fail "dinit core not enabled"
pass "dinit templates rendered"

run runner enable "$user" "$root/home" || fail "dinit runner enable failed"
grep -q "depends-on = whoisthat-cgroup" "$root/etc/dinit.d/whoisthat-runner" \
    || fail "dinit runner dependency missing"
grep -q "^command = /usr/lib/whoisthat/run-runner $uid $gid $root/home" "$root/etc/dinit.d/whoisthat-runner" \
    || fail "dinit runner command/args wrong"
grep -q "^restart = true" "$root/etc/dinit.d/whoisthat-runner" || fail "dinit runner restart"
pass "dinit runner rendered"

rm -rf "${root:?}/etc"
export WHOISTHAT_INIT=runit
run cgroup enable || fail "runit cgroup enable failed"
[ -x "$root/etc/runit/core-services/10-whoisthat-cgroup.sh" ] || fail "runit cgroup hook missing"
run core enable "$user" "$root/home" || fail "runit core enable failed"
runfile="$root/etc/runit/sv/whoisthat-core/run"
[ -x "$runfile" ] || fail "runit run script missing"
grep -q "chpst -u \"$uid:$gid\"" "$runfile" || fail "runit chpst args wrong"
[ -L "$root/etc/runit/runsvdir/default/whoisthat-core" ] || fail "runit service not linked"
run runner enable "$user" "$root/home" || fail "runit runner enable failed"
runner_run="$root/etc/runit/sv/whoisthat-runner/run"
[ -x "$runner_run" ] || fail "runit runner run missing"
[ -L "$root/etc/runit/runsvdir/default/whoisthat-runner" ] || fail "runit runner not linked"
pass "runit templates rendered and linked"

export WHOISTHAT_INIT=openrc
rm -rf "${root:?}/etc"
run cgroup enable
run core enable "$user" "$root/home"
run runner enable "$user" "$root/home"
run remove || fail "remove failed"
[ ! -e "$root/etc/init.d/whoisthat-core" ] || fail "service survived remove"
[ ! -e "$root/etc/init.d/whoisthat-cgroup" ] || fail "cgroup hook survived remove"
[ ! -e "$root/etc/init.d/whoisthat-runner" ] || fail "runner survived remove"
[ ! -e "$marker" ] || fail "marker survived remove"
pass "remove cleans services, hooks, runner and markers"

echo "install-init tests passed"
