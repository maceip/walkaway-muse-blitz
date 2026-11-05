#!/bin/sh
# Walkaway demo transport freeze (Partner Lane P, P3).
#
# Frozen demo path: USB cable + `adb reverse tcp:19099 tcp:19099`.
# The Android bridge polls http://127.0.0.1:19099 *on the phone*, which adb
# reverse tunnels to the laptop message bus. This is the only transport
# verified end-to-end with physical glasses taps (P1/P2). LAN is documented
# as an unverified fallback in README.md and is NOT the demo path.
#
# Idempotent. Never touches port 8899 (a foreign pre-existing server).
# Usage: services/hermes-host/start-transport.sh [--check-only]
set -eu

PORT=19099
TOKEN="${WALKAWAY_BUS_TOKEN:-walkaway-dev-bus-token}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LOG="${WALKAWAY_BUS_LOG:-/tmp/walkaway-bus.log}"
CHECK_ONLY=0
[ "${1:-}" = "--check-only" ] && CHECK_ONLY=1

fail() { echo "TRANSPORT FAIL: $*" >&2; exit 1; }

# 1. Message bus on 19099 (start only if the Walkaway bus is not healthy).
if curl -s -m 2 "http://127.0.0.1:$PORT/health" | grep -q walkaway-message-bus; then
  echo "bus: already healthy on $PORT"
else
  [ "$CHECK_ONLY" = 1 ] && fail "bus not healthy on $PORT (check-only)"
  echo "bus: starting on $PORT (log: $LOG)"
  (cd "$ROOT" && WALKAWAY_BUS_TOKEN="$TOKEN" nohup python3 \
    services/hermes-host/message_bus.py --host 127.0.0.1 --port "$PORT" \
    >>"$LOG" 2>&1 &)
  i=0
  until curl -s -m 2 "http://127.0.0.1:$PORT/health" | grep -q walkaway-message-bus; do
    i=$((i + 1)); [ "$i" -gt 20 ] && fail "bus did not become healthy; see $LOG"
    sleep 0.5
  done
  echo "bus: healthy on $PORT"
fi

# 2. Exactly one authorized Android device over USB.
DEVICES="$(adb devices | awk 'NR>1 && $2=="device" {print $1}')"
[ "$(printf '%s\n' "$DEVICES" | grep -c .)" = 1 ] || fail "expected exactly one adb device, got: $DEVICES"
echo "adb device: $DEVICES"

# 3. USB reverse tunnel for the bus port (idempotent).
adb reverse "tcp:$PORT" "tcp:$PORT" >/dev/null
adb reverse --list | grep -q "tcp:$PORT tcp:$PORT" || fail "adb reverse tcp:$PORT missing"
echo "adb reverse: tcp:$PORT -> tcp:$PORT"

# 4. Walkaway Bridge installed.
adb shell pm list packages | grep -q com.example.walkawaybridge \
  || fail "Walkaway Bridge not installed on the phone"
echo "android app: com.example.walkawaybridge installed"

cat <<EOF
transport frozen: USB + adb reverse tcp:$PORT (bus state: ${WALKAWAY_BUS_STATE:-/tmp/walkaway-bus-state.json})
phone steps: open Walkaway Bridge -> Connect -> status "Connected", DAT registration=REGISTERED.
glasses steps: worn, neural band connected. Decisions count only from glasses taps.
EOF
