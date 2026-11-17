#!/usr/bin/env bash
# Detached arm/leak test driver.
#
# Why this exists: driving a tunnel test from an agent turn means the turn (and
# the model's own HTTP request) is in flight exactly while routing flips and the
# TUN takes 50-100s to converge. When that request dies, the session is orphaned
# with the kill-switch armed and a human has to disarm by hand.
#
# So: everything happens here, in one detached process, and the agent only polls
# the verdict file. Nothing that matters depends on the agent staying alive --
# the EXIT trap disarms even if it never comes back.
#
# Run it as a Hermes background command with persist_on_release, never inline:
#   terminal(background=true, persist_on_release=true, notify_on_complete=true,
#            command="scripts/arm-test.sh <profile>")
#
# Env:
#   WT_OUT      verdict JSON path (default /home/a03/.hermes/cache/scratch/wt-armtest.json)
#   WT_KEEP=1   leave it armed + connected on exit (default: tear down)
#   WT_API      excluded test host (default api.unlid.ai)

set -uo pipefail

WT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/target/release/whoisthat"
PROFILE="${1:?usage: arm-test.sh <profile-selector>}"
OUT="${WT_OUT:-/home/a03/.hermes/cache/scratch/wt-armtest.json}"
API="${WT_API:-api.unlid.ai}"
TUN=whoisthattun
started=$(date -u +%FT%TZ)

verdict() { # verdict <result> <step> [note]
  printf '{"result":"%s","step":"%s","note":"%s","profile":"%s","started":"%s","finished":"%s"}\n' \
    "$1" "$2" "${3:-}" "$PROFILE" "$started" "$(date -u +%FT%TZ)" > "$OUT.tmp"
  mv "$OUT.tmp" "$OUT"
}
step() { echo "== $*" >&2; }

teardown() {
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
gw=$(ip -4 route show default | awk 'NR==1{for(i=1;i<=NF;i++) if($i=="via") print $(i+1)}')
dev=$(ip -4 route show default | awk 'NR==1{for(i=1;i<=NF;i++) if($i=="dev") print $(i+1)}')
api_ip=$(getent ahostsv4 "$API" | awk 'NR==1{print $1}')
if [ -n "$gw" ] && [ -n "$dev" ] && [ -n "$api_ip" ]; then
  if ip -4 route show | grep -q "^$api_ip "; then
    step "host route for $API already present"
  else
    # Detached, sudo never has a cached ticket; pkexec reaches the polkit
    # agent instead and is bounded so it cannot hang the cycle.
    sudo -n ip -4 route replace "$api_ip/32" via "$gw" dev "$dev" 2>/dev/null \
      || timeout 25 pkexec ip -4 route replace "$api_ip/32" via "$gw" dev "$dev" 2>/dev/null \
      || echo "warn: no host route for $API, agent channel may drop on the flip" >&2
  fi
else
  echo "warn: no default route parsed, skipping host route" >&2
fi

step "connect $PROFILE"
# --wait can hang forever when the core drops the CLI as a slow client; the
# connection itself still completes, so bound the wait and fall back to status.
timeout 90 "$WT" connect "$PROFILE" --wait -j >/dev/null 2>&1 || true
for _ in $(seq 1 30); do
  "$WT" status 2>/dev/null | grep -q '\[Connected\]' && break
  sleep 1
done
"$WT" status 2>/dev/null | grep -q '\[Connected\]' || { verdict fail connect "connect failed"; exit 1; }

step "mode tun"
timeout 60 "$WT" mode tun -j >/dev/null 2>&1 || { verdict fail mode-tun "switch refused"; exit 1; }

step "wait for $TUN (poll, never sleep)"
for _ in $(seq 1 90); do
  ip -4 route show | grep -q "$TUN" && break
  sleep 1
done
ip -4 route show | grep -q "$TUN" || { verdict fail tun-up "tun never appeared"; exit 1; }

step "wait for egress through $TUN"
for _ in $(seq 1 60); do
  [ "$(ip -4 route get 1.1.1.1 2>/dev/null | grep -c "$TUN")" -ge 1 ] && break
  sleep 1
done
verdict running tun-ready

step "arm"
"$WT" settings set kill-switch=true -j >/dev/null 2>&1 || { verdict fail arm "arm failed"; exit 1; }
sleep 2
armed=$("$WT" settings get kill-switch-armed -j 2>/dev/null | grep -o 'true')
[ "$armed" = "true" ] || { verdict fail arm "not armed"; exit 1; }

step "assertions while armed"
fail=0
exc=$(grep -o 'kill-switch: [0-9]* direct-rule accept exceptions' "$HOME/.config/whoisthat/core.log" | tail -1 | grep -o '[0-9]*' | head -1)
[ "${exc:-0}" -gt 0 ] || { fail=1; note="exceptions=$exc"; }
api=$(curl -sS -m 8 -o /dev/null -w '%{http_code}' "https://$API/v1/models")
[ "$api" != "000" ] || { fail=1; note="${note:-} api=$api"; }
web=$(curl -sS -m 8 -o /dev/null -w '%{http_code}' https://example.org)
[ "$web" = "200" ] || { fail=1; note="${note:-} web=$web"; }
egress=$(curl -4 -sS -m 10 https://api.ipify.org 2>/dev/null)
[ -n "$egress" ] || { fail=1; note="${note:-} egress empty"; }
leak6=$(curl -6 -sS -m 6 https://api64.ipify.org 2>/dev/null | grep -o ':' >/dev/null && echo v6-up || echo v6-blocked)
dns=$(getent ahostsv4 example.com >/dev/null 2>&1 && echo ok || echo fail)

if [ "$fail" -eq 0 ]; then
  verdict pass 'done' "exceptions=$exc api=$api web=$web egress=$egress dns=$dns $leak6"
else
  verdict fail assertions "${note# } exceptions=$exc api=$api web=$web egress=${egress:-none} dns=$dns $leak6"
  exit 1
fi
