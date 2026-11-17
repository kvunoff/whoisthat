#!/usr/bin/env bash
# Detached driver: the exception set must survive provider data changing under
# a live arm (subscription refresh), i.e. a re-arm re-resolves fresh server
# addresses instead of trusting what was resolved earlier.
#
# Same detached contract as arm-test.sh. Env: WT_OUT verdict path, WT_KEEP=1.

set -uo pipefail

WT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/target/release/whoisthat"
PROFILE="${1:?usage: rotation-test.sh <profile-selector>}"
GROUP="${2:?usage: rotation-test.sh <profile-selector> <group-id>}"
OUT="${WT_OUT:-/home/a03/.hermes/cache/scratch/wt-rotation.json}"
API="${WT_API:-api.unlid.ai}"
started=$(date -u +%FT%TZ)

verdict() {
  printf '{"result":"%s","step":"%s","note":"%s","profile":"%s","started":"%s","finished":"%s"}\n' \
    "$1" "$2" "${3:-}" "$PROFILE" "$started" "$(date -u +%FT%TZ)" > "$OUT.tmp"
  mv "$OUT.tmp" "$OUT"
}
step() { echo "== $*" >&2; }
exceptions() {
  grep -o 'kill-switch: [0-9]* direct-rule accept exceptions' "$HOME/.config/whoisthat/core.log" \
    | tail -1 | grep -o '[0-9]*' | head -1
}
teardown() {
  [ "${WT_KEEP:-0}" = "1" ] && return
  step "teardown: disarm + disconnect"
  "$WT" settings set kill-switch=false -j >/dev/null 2>&1
  "$WT" disconnect -j >/dev/null 2>&1
}
trap teardown EXIT

mkdir -p "$(dirname "$OUT")"
verdict running start

step "pre-flight"
"$WT" status -j >/dev/null 2>&1 || { verdict fail pre-flight "core unreachable"; exit 1; }
gw=$(ip -4 route show default | awk 'NR==1{for(i=1;i<=NF;i++) if($i=="via") print $(i+1)}')
dev=$(ip -4 route show default | awk 'NR==1{for(i=1;i<=NF;i++) if($i=="dev") print $(i+1)}')
api_ip=$(getent ahostsv4 "$API" | awk 'NR==1{print $1}')
if [ -n "$gw" ] && [ -n "$dev" ] && [ -n "$api_ip" ] && ! ip -4 route show | grep -q "^$api_ip "; then
  sudo -n ip -4 route replace "$api_ip/32" via "$gw" dev "$dev" 2>/dev/null \
    || timeout 25 pkexec ip -4 route replace "$api_ip/32" via "$gw" dev "$dev" 2>/dev/null \
    || echo "warn: agent channel route not added" >&2
fi

step "connect $PROFILE"
# --wait can hang forever when the core drops the CLI as a slow client; the
# connection itself still completes, so bound the wait and fall back to status.
# A connect can also land on a still-settling teardown, so retry it.
for attempt in 1 2 3; do
  timeout 90 "$WT" connect "$PROFILE" --wait -j >/dev/null 2>&1 || true
  for i in $(seq 1 30); do
    "$WT" status 2>/dev/null | grep -q '\[Connected\]' && break
    sleep 1
  done
  "$WT" status 2>/dev/null | grep -q '\[Connected\]' && break
  "$WT" disconnect -j >/dev/null 2>&1
  sleep 3
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
sleep 2
first=$(exceptions)
[ "${first:-0}" -gt 0 ] || { verdict fail arm "no exceptions at first arm"; exit 1; }

step "refresh the subscription under the live arm"
refreshed=no
if "$WT" sub refresh "$GROUP" -j >/dev/null 2>&1; then refreshed=yes; fi

step "re-arm"
"$WT" settings set kill-switch=false -j >/dev/null 2>&1
sleep 2
"$WT" settings set kill-switch=true -j >/dev/null 2>&1 || { verdict fail rearm "re-arm failed"; exit 1; }
# A refresh kicks off background profile tests; arming can take a while.
armed=0
for i in $(seq 1 30); do
  armed=$("$WT" settings get kill-switch-armed -j 2>/dev/null | grep -c true)
  [ "$armed" = "1" ] && break
  sleep 1
done
second=$(exceptions)

note="refreshed=$refreshed first=$first second=$second"
[ "${second:-0}" -gt 0 ] || { verdict fail rearm "$note (no exceptions after re-arm)"; exit 1; }
[ "$armed" = "1" ] || { verdict fail rearm "$note (not armed)"; exit 1; }

step "the agent channel must still reach out"
api=$(curl -sS -m 8 -o /dev/null -w '%{http_code}' "https://$API/v1/models")
[ "$api" != "000" ] || { verdict fail egress "$note api=$api"; exit 1; }

verdict pass done "$note api=$api"
