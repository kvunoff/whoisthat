#!/bin/sh
# Tests for scripts/authorize-core against a fake etc/varlib/bin tree and
# stubbed setcap/getcap. No root required.
set -eu

here=$(cd -- "$(dirname -- "$0")" && pwd)
tool=$here/authorize-core
root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT

bin=$root/bin
mkdir -p "$bin"
cat > "$bin/setcap" <<EOF
#!/bin/sh
echo "setcap \$*" >> "$root/caps.log"
exit 0
EOF
cat > "$bin/getcap" <<EOF
#!/bin/sh
echo "\$1 cap_net_admin,cap_net_raw,cap_setpcap=ep"
EOF
cat > "$bin/whoisthat-core" <<'EOF'
#!/bin/sh
exit 0
EOF
chmod 0755 "$bin/setcap" "$bin/getcap" "$bin/whoisthat-core"

export WHOISTHAT_TEST_MODE=1
export WHOISTHAT_ETCDIR="$root/etc"
export WHOISTHAT_VARLIB="$root/var/lib/whoisthat"
export WHOISTHAT_BINDIR="$bin"
export SETCAP="$bin/setcap"
export GETCAP="$bin/getcap"
export EXPECTED_OWNER
EXPECTED_OWNER=$(id -u)

uid=$(id -u)
# GitHub runners use uid 1001; pick a different second uid either way.
other=1001
[ "$other" = "$uid" ] && other=1002
authdir=$root/etc/whoisthat/authorized.d

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "ok    $*"; }
run() { "$tool" "$@"; }

run --add "$uid" || fail "--add failed"
[ -f "$authdir/$uid" ] || fail "allowlist entry missing"
[ "$(wc -c < "$authdir/$uid")" -eq 0 ] || fail "allowlist entry must be empty"
[ "$(stat -c %a "$authdir/$uid")" = 644 ] || fail "allowlist entry mode must be 0644"
[ -f "$root/var/lib/whoisthat/run.lock" ] || fail "run.lock missing"
[ "$(stat -c %a "$root/var/lib/whoisthat/run.lock")" = 644 ] || fail "run.lock mode must be 0644"
grep -q "cap_net_admin,cap_net_raw,cap_setpcap=+ep $bin/whoisthat-core" "$root/caps.log" \
    || fail "setcap was not called with the expected caps"
pass "add writes allowlist and lock and applies caps"

run --add "$uid" || fail "idempotent --add failed"
[ "$(ls "$authdir" | wc -l)" -eq 1 ] || fail "idempotent --add duplicated the entry"
pass "add is idempotent"

run --add "$other" || fail "--add numeric uid failed"
[ -f "$authdir/$other" ] || fail "numeric uid entry missing"
run --list | grep -q "^$uid " || fail "--list does not show the current uid"
run --list | grep -q "^$other " || fail "--list does not show $other"
pass "list shows authorized uids"

run --remove "$other" || fail "--remove failed"
[ ! -e "$authdir/$other" ] || fail "removed uid still present"
[ -f "$authdir/$uid" ] || fail "removing one uid dropped another"
pass "remove drops only the requested uid"

if run --add 999 2>/dev/null; then fail "uid < 1000 must be refused"; fi
pass "uid below 1000 is refused"

if run --add 2>/dev/null; then fail "missing target must be refused"; fi
pass "missing target is refused"

if (unset PKEXEC_UID SUDO_UID; run --add) 2>/dev/null; then
    fail "missing target without env must be refused"
fi
pass "missing target without PKEXEC/SUDO_UID is refused"

if PKEXEC_UID=1000 run --add "$uid" --bin "$bin/whoisthat-core" 2>/dev/null; then
    fail "--bin through pkexec must be refused"
fi
pass "--bin through pkexec is refused"

ln -s "$bin/whoisthat-core" "$root/link"
if run --add "$uid" --bin "$root/link" 2>/dev/null; then
    fail "--bin symlink must be refused"
fi
pass "--bin symlink is refused"

if EXPECTED_OWNER=0 run --add "$uid" --bin "$bin/whoisthat-core" 2>/dev/null; then
    fail "user-owned bin must be refused"
fi
pass "user-owned bin is refused"

if run --remove "$uid" --bin "$bin/whoisthat-core" 2>/dev/null; then
    fail "--bin with --remove must be refused"
fi
pass "--bin with --remove is refused"

: > "$authdir/keepme"
run --remove-all || fail "--remove-all failed"
[ ! -e "$authdir/$uid" ] || fail "allowlist entry survived --remove-all"
[ ! -e "$root/var/lib/whoisthat/run.lock" ] || fail "run.lock survived --remove-all"
[ -e "$authdir/keepme" ] || fail "non-numeric entry must survive --remove-all"
grep -q "setcap -r $bin/whoisthat-core" "$root/caps.log" || fail "caps were not removed"
pass "remove-all clears entries, lock and caps; keeps foreign files"

pass "authorize-core"
