#!/usr/bin/env bash
# Detached driver: adding a direct rule while the switch is armed must reach the
# firewall immediately, not on the next connect.
#
# Same detached contract as arm-test.sh (see its header): run it as a Hermes
# background command, poll the verdict file, never inline.
#
# Env: WT_OUT verdict path (default .../wt-refresh.json), WT_KEEP=1 leave armed.

set -uo pipefail

WT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/target/release/whoisthat"
PROFILE="${1:?usage: refresh-test.sh <profile-selector>}"
OUT="${WT_OUT:-/home/a03/.hermes/cache/scratch/wt-refresh.json}"
PROBE=refresh-probe.example
PROBE_IP=192.0.2.53
started=$(date -u +%FT%TZ)

verdict() {
  printf '{"result":"%s","step":"%s","note":"%s","profile":"%s","started":"%s","finished":"%s"}\n' \
    "$1" "$2" "${3:-}" "$PROFILE" "$started" "$(date -u +%FT%TZ)" > "$OUT.tmp"
  mv "$OUT.tmp" "$OUT"
}
step() { echo "== $*" >&2; }

# Root-only nft listing; an unprivileged call errors, which reads as "absent".
nftok() { sudo -n "$@" 2>/dev/null || timeout 20 pkexec "$@" 2>/dev/null; }

exceptions() {
  grep -o 'kill-switch: [0-9]* direct-rule accept exceptions' "$HOME/.config/whoisthat/core.log" \
    | tail -1 | grep -o '[0-9]*' | head -1
}

cleanup_pin() {
  if grep -q "$PROBE" /etc/hosts; then
    sudo -n sed -i "/[[:space:]]$PROBE\$/d" /etc/hosts 2>/dev/null \
      || timeout 25 pkexec sed -i "/[[:space:]]$PROBE\$/d" /etc/hosts 2>/dev/null
  fi
}

teardown() {
  # Route indices shift; remove by matching the rule text instead.
  "$WT" route list -j 2>/dev/null | python3 -c '
import json,sys
try: rules = json.load(sys.stdin).get("rules", [])
except Exception: sys.exit(0)
for i, r in enumerate(rules):
    if "refresh-probe.example" in json.dumps(r):
        print(i)
' 2>/dev/null | while read -r idx; do "$WT" route rm "$idx" -j >/dev/null 2>&1; done
  cleanup_pin
  [ "${WT_KEEP:-0}" = "1" ] && return
  step "teardown: disarm + disconnect"
  timeout 30 "$WT" settings set kill-switch=false -j >/dev/null 2>&1
  timeout 30 "$WT" disconnect -j >/dev/null 2>&1
}
trap teardown EXIT

mkdir -p "$(dirname "$OUT")"
verdict running start

step "pre-flight"
"$WT" status -j >/dev/null 2>&1 || { verdict fail pre-flight "core unreachable"; exit 1; }
if ! grep -q "$PROBE" /etc/hosts; then
  sudo -n sh -c "printf '192.0.2.53 $PROBE\n' >> /etc/hosts" 2>/dev/null \
    || timeout 25 pkexec sh -c "printf '192.0.2.53 $PROBE\n' >> /etc/hosts" 2>/dev/null \
    || { verdict fail pre-flight "cannot pin $PROBE"; exit 1; }
fi

step "connect $PROFILE"
# --wait can hang forever when the core drops the CLI as a slow client; the
# connection itself still completes, so bound the wait and fall back to status.
timeout 90 "$WT" connect "$PROFILE" --wait -j >/dev/null 2>&1 || true
for i in $(seq 1 30); do
  "$WT" status 2>/dev/null | grep -q '\[Connected\]' && break
  sleep 1
done
"$WT" status 2>/dev/null | grep -q '\[Connected\]' || { verdict fail connect "connect failed"; exit 1; }
step "mode tun"
timeout 60 "$WT" mode tun -j >/dev/null 2>&1 || { verdict fail mode-tun "switch refused"; exit 1; }
step "wait for tun"
for i in $(seq 1 90); do ip -4 route show | grep -q whoisthattun && break; sleep 1; done
ip -4 route show | grep -q whoisthattun || { verdict fail tun-up "tun never appeared"; exit 1; }
verdict running tun-ready

step "arm"
"$WT" settings set kill-switch=true -j >/dev/null 2>&1 || { verdict fail arm "arm failed"; exit 1; }
for i in $(seq 1 20); do
  [ "$("$WT" settings get kill-switch-armed -j 2>/dev/null | grep -c true)" = "1" ] && break
  sleep 1
done
[ "$("$WT" settings get kill-switch-armed -j 2>/dev/null | grep -c true)" = "1" ] || { verdict fail arm "not armed"; exit 1; }
before=$(exceptions)
[ "${before:-0}" -gt 0 ] || { verdict fail arm "no exceptions at arm"; exit 1; }

step "add a direct rule while armed"
"$WT" route add --type domain --value "$PROBE" --outbound direct -j >/dev/null 2>&1 || { verdict fail rule-add "route add failed"; exit 1; }

step "wait for the exception set to grow"
after=$before
for i in $(seq 1 20); do
  after=$(exceptions)
  [ "${after:-0}" -gt "$before" ] && break
  sleep 1
done

note="before=$before after=$after"
[ "${after:-0}" -gt "$before" ] || { verdict fail refresh "$note (exception set never refreshed)"; exit 1; }

step "the new host must be accepted by the live ruleset"
if nftok nft list table inet whoisthat_ks >/dev/null 2>&1; then
  nftok nft list table inet whoisthat_ks 2>/dev/null | grep -q "$PROBE_IP" \
    || { verdict fail refresh "$note (address not in nft ruleset)"; exit 1; }
  note="$note nft=present"
else
  note="$note nft=unreadable"
fi

verdict pass done "$note"
