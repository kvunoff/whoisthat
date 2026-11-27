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

    # --- traffic contract -------------------------------------------------
    # In TEST_MODE the core runs a deterministic synthetic meter, so the
    # snapshot and the frame stream can be checked without Xray.
    state=$("$TUI_BIN" status --socket "$SOCKET" --json 2>/dev/null) || {
      echo "ipc-e2e: status --json failed" >&2
      exit 1
    }
    case "$state" in
      *'"rx_speed_bytes":'*) ;;
      *) echo "ipc-e2e: status --json missing speed fields: $state" >&2; exit 1 ;;
    esac

    # The first snapshot of a fresh session has no elapsed interval, so its
    # rates are legitimately zero. The stream must produce non-zero rates by
    # the second frame.
    watch=$(timeout 4 "$TUI_BIN" status --watch --socket "$SOCKET" --json 2>/dev/null || true)
    frames=$(printf '%s\n' "$watch" | grep -c 'rx_speed_bytes' || true)
    nonzero=$(printf '%s\n' "$watch" | grep -c '"rx_speed_bytes":[1-9]' || true)
    if [ "$frames" -lt 2 ] || [ "$nonzero" -lt 1 ]; then
      echo "ipc-e2e: expected a traffic-sample stream with rates, got $frames frames ($nonzero non-zero)" >&2
      exit 1
    fi

    # --- anti-DPI settings ------------------------------------------------
    "$TUI_BIN" settings set \
      anti-dpi.fragment.enabled=true \
      anti-dpi.fragment.packets=1-2 \
      anti-dpi.noise.type=hex \
      anti-dpi.noise.packet=00ff \
      --socket "$SOCKET" >/dev/null
    packets=$("$TUI_BIN" settings get anti-dpi.fragment.packets --socket "$SOCKET" --json)
    case "$packets" in
      *'"value":"1-2"'*) ;;
      *) echo "ipc-e2e: anti-dpi.fragment.packets not applied: $packets" >&2; exit 1 ;;
    esac
    noise_type=$("$TUI_BIN" settings get anti-dpi.noise.type --socket "$SOCKET" --json)
    case "$noise_type" in
      *'"value":"hex"'*) ;;
      *) echo "ipc-e2e: anti-dpi.noise.type not applied: $noise_type" >&2; exit 1 ;;
    esac
    noise_packet=$("$TUI_BIN" settings get anti-dpi.noise.packet --socket "$SOCKET" --json)
    case "$noise_packet" in
      *'"value":"00ff"'*) ;;
      *) echo "ipc-e2e: anti-dpi.noise.packet not applied: $noise_packet" >&2; exit 1 ;;
    esac
    grep -q '"anti-dpi"' "$XDG_CONFIG_HOME/whoisthat/config.json" ||
      { echo "ipc-e2e: anti-dpi missing from config.json" >&2; exit 1; }
    grep -q '"packets": "1-2"' "$XDG_CONFIG_HOME/whoisthat/config.json" ||
      { echo "ipc-e2e: anti-dpi fragment not persisted" >&2; exit 1; }

    # --- network settings -------------------------------------------------
    "$TUI_BIN" settings set tun-mtu=1280 dns-servers=9.9.9.9,2620:fe::fe \
      --socket "$SOCKET" >/dev/null
    mtu=$("$TUI_BIN" settings get tun-mtu --socket "$SOCKET" --json)
    case "$mtu" in
      *'"value":1280'*) ;;
      *) echo "ipc-e2e: tun-mtu not applied: $mtu" >&2; exit 1 ;;
    esac
    dns=$("$TUI_BIN" settings get dns-servers --socket "$SOCKET" --json)
    case "$dns" in
      *'"value":"9.9.9.9,2620:fe::fe"'*) ;;
      *) echo "ipc-e2e: dns-servers not applied: $dns" >&2; exit 1 ;;
    esac
    grep -q '"tun-mtu": 1280' "$XDG_CONFIG_HOME/whoisthat/config.json" ||
      { echo "ipc-e2e: tun-mtu missing from config.json" >&2; exit 1; }
    if "$TUI_BIN" settings set dns-servers=not-an-ip --socket "$SOCKET" >/dev/null 2>&1; then
      echo "ipc-e2e: invalid dns-servers was accepted" >&2
      exit 1
    fi
    dns_after=$("$TUI_BIN" settings get dns-servers --socket "$SOCKET" --json)
    case "$dns_after" in
      *'"value":"9.9.9.9,2620:fe::fe"'*) ;;
      *) echo "ipc-e2e: invalid dns-servers changed the stored value: $dns_after" >&2; exit 1 ;;
    esac

    if command -v python3 >/dev/null 2>&1; then
      python3 - "$SOCKET" <<'PY' || exit 1
import json, socket, struct, sys, time

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.settimeout(3)
sock.connect(sys.argv[1])
request = b'{"msg":"get-application-state","data":{}}'
sock.sendall(struct.pack(">I", len(request)) + request)
names = set()
traffic = None
samples = []
deadline = time.time() + 3
buf = b""
while time.time() < deadline:
    try:
        chunk = sock.recv(65536)
    except socket.timeout:
        break
    if not chunk:
        break
    buf += chunk
    while len(buf) >= 4:
        length = struct.unpack(">I", buf[:4])[0]
        if len(buf) < 4 + length:
            break
        payload, buf = buf[4:4 + length], buf[4 + length:]
        frame = json.loads(payload)
        name = frame.get("msg")
        names.add(name)
        if name == "application-state":
            traffic = frame.get("data", {}).get("traffic")
        elif name == "traffic-sample":
            samples.append(frame.get("data", {}))
if "traffic-sample" not in names:
    sys.exit("ipc-e2e: no traffic-sample frame on the raw socket")
if "traffic-stats" in names:
    sys.exit("ipc-e2e: removed traffic-stats frame is still broadcast")
if not isinstance(traffic, dict) or not traffic.get("session_id"):
    sys.exit("ipc-e2e: application-state has no traffic snapshot")
if not all(s.get("session_id") for s in samples):
    sys.exit("ipc-e2e: traffic-sample frames have no session_id")
print("ipc-e2e: traffic snapshots ok")
PY
    fi

    echo "ipc-e2e: ok (core v$WANT_VERSION, protocol $WANT_PROTOCOL, traffic frames)"
    exit 0
  fi
  sleep 0.2
done

echo "ipc-e2e: core did not answer on $SOCKET within 30s" >&2
exit 1
