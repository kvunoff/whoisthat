#!/usr/bin/env bash
# Privileged TUN/firewall scenarios in a throwaway network namespace.
# Requires root (or CAP_NET_ADMIN) and re-executes itself through sudo.
#
# REQUIRE_NETNS=1 turns an all-SKIP run into a failure. CI sets it so that a
# renamed or broken TestNetns* cannot pass as a green job.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ "$(id -u)" -ne 0 ]; then
    exec sudo -E env "PATH=$PATH" "HOME=$HOME" "REQUIRE_NETNS=${REQUIRE_NETNS:-0}" "$0" "$@"
fi

LOG="${TMPDIR:-/tmp}/whoisthat-netns.log"
cd "$ROOT/core/core"
go test -tags=integration ./lib/proxy/tun/ -run '^TestNetns' -v | tee "$LOG"

grep -Eq -- '--- (PASS|SKIP): TestNetns' "$LOG" ||
    { echo 'netns: no TestNetns* discovered' >&2; exit 1; }
if [ "${REQUIRE_NETNS:-0}" = 1 ]; then
    grep -Eq -- '--- PASS: TestNetns' "$LOG" ||
        { echo 'netns: every TestNetns* was skipped' >&2; exit 1; }
fi
