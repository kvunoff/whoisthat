#!/usr/bin/env bash
#
# IPC smoke test: start a real whoisthat-core in a throwaway XDG sandbox and
# read its state through the TUI's CLI. Catches transport/handshake drift that
# the struct-level contract snapshot cannot see.
#
# The core runs with WHOISTHAT_TEST_MODE=1, which skips the geo/Xray downloads,
# startup firewall reconcile and subscription updates — those touch the network
# and global nftables state.
#
# Inputs (all optional):
#   WHOISTHAT_E2E_CORE  core binary (default: core/core/whoisthat-core)
#   WHOISTHAT_BIN       TUI/CLI binary (default: target/debug/whoisthat)

set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd) || exit 1
CORE_BIN=${WHOISTHAT_E2E_CORE:-$ROOT/core/core/whoisthat-core}
TUI_BIN=${WHOISTHAT_BIN:-$ROOT/target/debug/whoisthat}

if [ "$(id -u)" = "0" ]; then
  echo "ipc-e2e: refusing to run as root (the core touches firewall state)" >&2
  exit 1
fi

for bin in "$CORE_BIN" "$TUI_BIN"; do
  if [ ! -x "$bin" ]; then
    echo "ipc-e2e: missing $bin (build it first: make core && cargo build)" >&2
    exit 1
  fi
done

TMP=$(mktemp -d "${TMPDIR:-/tmp}/whoisthat-ipc-e2e.XXXXXX") || exit 1
PID=""
cleanup() {
  if [ -n "$PID" ]; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
  fi
  rm -rf "$TMP"
}
trap cleanup EXIT

export XDG_CONFIG_HOME="$TMP/config"
export XDG_DATA_HOME="$TMP/data"
export XDG_RUNTIME_DIR="$TMP/run"
export HOME="$TMP/home"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_RUNTIME_DIR" "$HOME"
chmod 700 "$XDG_RUNTIME_DIR"

SOCKET="$XDG_RUNTIME_DIR/whoisthat/core.sock"
WANT_PROTOCOL=$(tr -d '[:space:]' < "$ROOT/core/core/lib/version/protocol.txt")
WANT_VERSION=$(sed -n 's/^const CoreVersion = "\(.*\)"$/\1/p' "$ROOT/core/core/lib/version/version.go")
if [ -z "$WANT_PROTOCOL" ] || [ -z "$WANT_VERSION" ]; then
  echo "ipc-e2e: cannot read the expected version from the core sources" >&2
  exit 1
fi

WHOISTHAT_TEST_MODE=1 WHOISTHAT_LOG_LEVEL=error "$CORE_BIN" &
PID=$!

deadline=$((SECONDS + 30))
while [ "$SECONDS" -lt "$deadline" ]; do
  if ! kill -0 "$PID" 2>/dev/null; then
    echo "ipc-e2e: core exited during startup" >&2
    exit 1
  fi
  if state=$("$TUI_BIN" core status --socket "$SOCKET" --json 2>/dev/null); then
    case "$state" in
      *'"running":true'*) ;;
      *) sleep 0.2; continue ;;
    esac

    case "$state" in
      *"\"protocol\":$WANT_PROTOCOL"*) ;;
      *) echo "ipc-e2e: protocol mismatch, the CLI saw: $state" >&2; exit 1 ;;
    esac
    case "$state" in
      *"\"version\":\"$WANT_VERSION\""*) ;;
      *) echo "ipc-e2e: core version mismatch, the CLI saw: $state" >&2; exit 1 ;;
    esac

    echo "ipc-e2e: ok (core v$WANT_VERSION, protocol $WANT_PROTOCOL)"
    exit 0
  fi
  sleep 0.2
done

echo "ipc-e2e: core did not answer on $SOCKET within 30s" >&2
exit 1
