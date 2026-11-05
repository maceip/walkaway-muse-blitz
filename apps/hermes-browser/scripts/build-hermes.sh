#!/usr/bin/env bash
# Builds Static Hermes (branch static_h) into $HERMES_ROOT.
#
# Produces:
#   $HERMES_ROOT/build/bin/shermes          the AOT compiler (TS -> C)
#   $HERMES_ROOT/build/lib/libhermesvm.dylib
#   $HERMES_ROOT/build/jsi/libjsi.a
#
# build.rs consumes these via HERMES_SRC_DIR / HERMES_BUILD_DIR.
set -euo pipefail

HERMES_ROOT="${HERMES_ROOT:-$HOME/.cache/hermes-static_h}"
SRC="$HERMES_ROOT/src"
BUILD="$HERMES_ROOT/build"
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
  -DHERMES_ENABLE_DEBUGGER=OFF \
  -DHERMES_ENABLE_INTL=OFF \
  -DHERMES_ENABLE_TEST_SUITE=OFF \
  -DHERMES_BUILD_APPLE_FRAMEWORK=OFF

echo "==> building shermes + hermesvm + jsi (-j $JOBS)"
ninja -C "$BUILD" shermes hermesvm jsi -j "$JOBS"

echo "==> done"
"$BUILD/bin/shermes" --version
