#!/usr/bin/env bash
# Builds Static Hermes (branch static_h) into $HERMES_ROOT.
#
# Produces:
#   $HERMES_BUILD_DIR/bin/shermes          the AOT compiler (TS -> C)
#   $HERMES_BUILD_DIR/lib/libhermesvm.dylib (one VM, debugger enabled)
#   $HERMES_BUILD_DIR/jsi/libjsi.a
#
# Default to an isolated debugger build; do not mutate the frozen demo cache.
#
# build.rs consumes these via HERMES_SRC_DIR / HERMES_BUILD_DIR.
set -euo pipefail

HERMES_ROOT="${HERMES_ROOT:-$HOME/.cache/hermes-static_h}"
SRC="${HERMES_SRC_DIR:-$HERMES_ROOT/src}"
BUILD="${HERMES_BUILD_DIR:-$HERMES_ROOT/build-walkaway-debugger}"
BRANCH="${HERMES_BRANCH:-static_h}"
JOBS="${JOBS:-$(sysctl -n hw.ncpu)}"

if [ ! -d "$SRC/.git" ]; then
  echo "==> cloning facebook/hermes ($BRANCH) into $SRC"
  mkdir -p "$HERMES_ROOT"
  git clone --depth 1 --branch "$BRANCH" https://github.com/facebook/hermes.git "$SRC"
else
  echo "==> reusing existing checkout at $SRC"
fi

echo "==> configuring"
cmake -S "$SRC" -B "$BUILD" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DHERMES_ENABLE_DEBUGGER=ON \
  -DHERMES_ENABLE_INTL=OFF \
  -DHERMES_ENABLE_TEST_SUITE=OFF \
  -DHERMES_BUILD_APPLE_FRAMEWORK=OFF

echo "==> building shermes + hermesvm + jsi (-j $JOBS)"
ninja -C "$BUILD" shermes hermesvm jsi -j "$JOBS"

echo "==> done"
"$BUILD/bin/shermes" --version
