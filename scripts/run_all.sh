#!/bin/sh
# Walkaway demo stack: dedicated Chrome -> companion relay -> connector -> glasses app.
# Every process is foreground-managed by this script; Ctrl-C stops the stack.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ULTRAFAST="${WALKAWAY_ULTRAFAST_DIR:-$HOME/jev-ultrafast}"
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"

PIDS=""
cleanup() { for pid in $PIDS; do kill "$pid" 2>/dev/null || true; done; }
trap cleanup EXIT INT TERM

if ! curl -s http://127.0.0.1:9223/json/version >/dev/null 2>&1; then
  "$CHROME" --headless=new --remote-debugging-port=9223 --remote-debugging-address=127.0.0.1 \
    --user-data-dir=/tmp/walkaway-chrome --no-first-run --disable-gpu about:blank >/tmp/walkaway-chrome.log 2>&1 &
  PIDS="$PIDS $!"; sleep 2
fi

(cd "$ULTRAFAST" && BU_CDP_URL=http://127.0.0.1:9223 uv run python "$ROOT/companion/walkaway_relay.py") &
PIDS="$PIDS $!"

WALKAWAY_CONNECTOR_TOKEN="${WALKAWAY_CONNECTOR_TOKEN:-}" \
  python3 "$ROOT/connector/mcp_server.py" &
PIDS="$PIDS $!"

cd "$ROOT/glasses"
if [ ! -d node_modules ]; then yarn install --mode=skip-build; fi
echo "Relay http://127.0.0.1:8790 · Connector http://127.0.0.1:8890/mcp · Glasses http://127.0.0.1:5173/"
exec yarn dev --host 0.0.0.0 --port 5173
