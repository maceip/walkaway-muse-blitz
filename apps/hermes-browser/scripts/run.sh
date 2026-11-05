#!/usr/bin/env bash
# Builds Hermes if needed, then runs the browser.
#
#   ./scripts/run.sh                 # open the window
#   ./scripts/run.sh --selftest      # headless end-to-end check
#   ./scripts/run.sh --render out.png [--click]
set -euo pipefail

cd "$(dirname "$0")/.."

HERMES_ROOT="${HERMES_ROOT:-$HOME/.cache/hermes-static_h}"
if [ ! -x "$HERMES_ROOT/build/bin/shermes" ]; then
  ./scripts/build-hermes.sh
fi

export HERMES_SRC_DIR="${HERMES_SRC_DIR:-$HERMES_ROOT/src}"
export HERMES_BUILD_DIR="${HERMES_BUILD_DIR:-$HERMES_ROOT/build}"

exec cargo run --release -- "$@"
