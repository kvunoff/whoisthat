#!/usr/bin/env bash
# Host-acceptance scenarios that the plan lists as pending (T5 / T7 / T8).
#
# Same discipline as arm-test.sh: detached, poll a verdict file, disarm on the
# way out. These deliberately kill the core, so the EXIT trap restarts the
# service before trying to disarm -- a dead core cannot disarm anything and the
# nft ruleset would otherwise survive in the kernel.
#
#   scripts/arm-survive.sh dns-kill9   # T5: resolver answers, then kill -9
#   scripts/arm-survive.sh sigterm     # T8: SIGTERM leaves no ruleset behind
#   scripts/arm-survive.sh rearm       # T7: a re-arm re-resolves the exceptions

set -uo pipefail

WT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/target/release/whoisthat"
SCENARIO="${1:?usage: arm-survive.sh <dns-kill9|sigterm|rearm>}"
OUT="${WT_OUT:-/home/a03/.hermes/cache/scratch/wt-survive.json}"
PROFILE="${WT_PROFILE:-1:23}"
TUN=whoisthattun
TABLE=whoisthat_ks
LOG="$HOME/.config/whoisthat/core.log"
started=$(date -u +%FT%TZ)
fail=0
note=""

verdict() {
  printf '{"result":"%s","scenario":"%s","note":"%s","started":"%s","finished":"%s"}\n' \
    "$1" "$SCENARIO" "$note" "$started" "$(date -u +%FT%TZ)" > "$OUT.tmp"
  mv "$OUT.tmp" "$OUT"
}
step() { echo "== $*" >&2; }
bad() { fail=1; note="${note:+$note }$1"; }

# nft needs root to list; sudo has no ticket in a detached run, so fall back
# to a bounded pkexec. Unprivileged nft errors out and would read as "absent".
nftok() {
  sudo -n nft list table "$1" "$2" >/dev/null 2>&1 && return 0
  timeout 20 pkexec nft list table "$1" "$2" >/dev/null 2>&1
}
core_up() { systemctl --user is-active --quiet whoisthat-core.service; }
restart_core() {
  systemctl --user restart whoisthat-core.service
  for _ in $(seq 1 20); do core_up && return 0; sleep 1; done
  return 1
}
armed() { [ "$("$WT" settings get kill-switch-armed -j 2>/dev/null | grep -o true)" = true ]; }

cleanup() {
  core_up || restart_core || true
  "$WT" settings set kill-switch=false -j >/dev/null 2>&1
  "$WT" disconnect -j >/dev/null 2>&1
  # A killed core can leave the table if the trap could not reach it.
  sudo -n nft delete table inet $TABLE 2>/dev/null \
    || timeout 20 pkexec nft delete table inet $TABLE 2>/dev/null || true
  ip link show "$TUN" >/dev/null 2>&1 && echo "warn: $TUN still present" >&2
}
trap cleanup EXIT

mkdir -p "$(dirname "$OUT")"
verdict running
step "$SCENARIO: bring up"
for a in 1 2 3; do
  err=$("$WT" connect "$PROFILE" --wait -j 2>&1) && break
  [ "$a" = 3 ] && { note="connect: $(printf '%s' "$err" | tail -c 160)"; verdict fail; exit 1; }
  "$WT" disconnect -j >/dev/null 2>&1; sleep 2
done
err=$(timeout 60 "$WT" mode tun -j 2>&1) || { note="mode tun: $(printf '%s' "$err" | tail -c 160)"; verdict fail; exit 1; }
for _ in $(seq 1 90); do ip -4 route show | grep -q "$TUN" && break; sleep 1; done
"$WT" settings set kill-switch=true -j >/dev/null 2>&1
sleep 2
armed || { verdict fail; note="arm"; exit 1; }

case "$SCENARIO" in
  dns-kill9)
    step "T5: allowlisted resolver while armed"
    a=$(getent ahostsv4 example.org >/dev/null 2>&1 && echo ok || echo fail)
    [ "$a" = ok ] || bad "dns-before=$a"
    pid=$(pgrep -x whoisthat-core | head -1)
    step "kill -9 $pid"
    kill -9 "$pid" 2>/dev/null || bad "no-core-pid"
    # Hard kill: the ruleset may or may not survive, and the restart's reconcile
    # clears stale state before re-arming. Measure the fail-open window rather
    # than assume either way.
    gap=0
    for _ in $(seq 1 20); do
      nftok inet $TABLE && break
      gap=$((gap + 1)); sleep 1
    done
    a2=$(getent ahostsv4 example.org >/dev/null 2>&1 && echo ok || echo fail)
    [ "$a2" = ok ] || bad "dns-after-kill9=$a2"
    step "restart core"
    restart_core || bad "restart-failed"
    for _ in $(seq 1 20); do
      nftok inet $TABLE && armed && break
      sleep 1
    done
    nftok inet $TABLE || bad "ruleset-not-restored"
    armed || bad "not-rearmed-after-restart"
    note="${note:+$note }fail-open=${gap}s"
    a3=$(getent ahostsv4 example.org >/dev/null 2>&1 && echo ok || echo fail)
    [ "$a3" = ok ] || bad "dns-after-reconcile=$a3"
    note="${note:+$note }dns=$a/$a2/$a3"
    ;;
  sigterm)
    step "T8: SIGTERM must remove the ruleset"
    kill -TERM "$(pgrep -x whoisthat-core | head -1)" 2>/dev/null || bad "no-core-pid"
    for _ in $(seq 1 15); do core_up || break; sleep 1; done
    sleep 1
    nftok inet $TABLE && bad "table-survived-sigterm"
    nftok ip whoisthat_mangle && bad "mangle-survived-sigterm"
    step "restart + verify reconcile"
    restart_core || bad "restart-failed"
    sleep 2
    "$WT" settings set kill-switch=true -j >/dev/null 2>&1
    sleep 2
    armed || bad "not-armed-after-restart"
    ;;
  rearm)
    step "T7: a re-arm re-resolves the pinned exception address"
    # probe2 is pinned in /etc/hosts and the pin is flipped between arms; the
    # address need not be routable, only the ruleset is under test.
    setpin() {
      sudo -n sed -i "/probe2.example/d" /etc/hosts 2>/dev/null \
        || timeout 25 pkexec sed -i "/probe2.example/d" /etc/hosts 2>/dev/null
      printf '%s probe2.example\n' "$1" | sudo -n tee -a /etc/hosts >/dev/null 2>&1 \
        || printf '%s probe2.example\n' "$1" | timeout 25 pkexec tee -a /etc/hosts >/dev/null 2>&1
    }
    setpin 192.0.2.10 || bad "pin-a-failed"
    "$WT" route add --type domain --value probe2.example --outbound direct -j >/dev/null 2>&1
    "$WT" settings set kill-switch=false -j >/dev/null 2>&1
    sleep 1
    "$WT" settings set kill-switch=true -j >/dev/null 2>&1
    sleep 2
    first=$(grep -o 'kill-switch: direct-rule targets .*' "$LOG" | tail -1)
    case "$first" in *192.0.2.10*) : ;; *) bad "pin-a-not-accepted" ;; esac

    step "flip the pin and re-arm"
    setpin 192.0.2.11 || bad "pin-b-failed"
    "$WT" settings set kill-switch=false -j >/dev/null 2>&1
    sleep 1
    "$WT" settings set kill-switch=true -j >/dev/null 2>&1
    sleep 2
    armed || bad "rearm-failed"
    second=$(grep -o 'kill-switch: direct-rule targets .*' "$LOG" | tail -1)
    case "$second" in *192.0.2.11*) : ;; *) bad "new-address-not-resolved" ;; esac
    case "$second" in *192.0.2.10*) bad "old-address-still-accepted" ;; *) : ;; esac
    exc=$(grep -o 'kill-switch: [0-9]* direct-rule accept exceptions' "$LOG" | tail -1 | grep -o '[0-9]*' | head -1)
    [ "${exc:-0}" -gt 0 ] || bad "exceptions=$exc"
    note="${note:+$note }exceptions=$exc"
    "$WT" route rm 3 -j >/dev/null 2>&1
    (sudo -n sed -i "/probe2.example/d" /etc/hosts 2>/dev/null \
      || timeout 25 pkexec sed -i "/probe2.example/d" /etc/hosts 2>/dev/null) || true
    ;;
  *) verdict fail; note="unknown scenario"; exit 1 ;;
esac

if [ "$fail" -eq 0 ]; then verdict pass; else verdict fail; exit 1; fi
