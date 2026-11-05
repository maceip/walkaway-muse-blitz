#!/usr/bin/env bash
# Fail if the Android app's identity (applicationId or signing certificate)
# drifts from the identity Meta DAT registration was proven against.
#
# Pinned 2026-11-05 after verifying, against false suspicion of churn:
#   applicationId = com.example.walkawaybridge (build.gradle.kts, unchanged in git history)
#   cert SHA-256  = abdf2c3a584b9accbd1b51ae474aa840becc46aa5c5f669bccf422dc501acf39
#                   ($HOME/.android/debug.keystore, created 2026-05-02, unmodified)
# Meta DAT re-prompts were caused by app uninstall/data wipe by connected-test runs,
# never by identity drift. This check keeps that provable in one command.
#
# Usage: scripts/verify-app-identity.sh [apk]
set -euo pipefail

APP_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APK="${1:-$APP_DIR/app/build/outputs/apk/debug/app-debug.apk}"
EXPECTED_ID="com.example.walkawaybridge"
EXPECTED_SHA256="abdf2c3a584b9accbd1b51ae474aa840becc46aa5c5f669bccf422dc501acf39"

grep -q "applicationId = \"$EXPECTED_ID\"" "$APP_DIR/app/build.gradle.kts" \
  || { echo "FAIL applicationId drift in build.gradle.kts"; exit 1; }

SDK="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
APKSIGNER="$(find "$SDK/build-tools" -name apksigner -type f | sort -V | tail -1)"
[ -n "$APKSIGNER" ] || { echo "FAIL apksigner not found under $SDK"; exit 1; }
[ -f "$APK" ] || { echo "APK missing, building..."; (cd "$APP_DIR" && ./gradlew :app:assembleDebug -q --console=plain); }

actual="$("$APKSIGNER" verify --print-certs "$APK" | awk '/SHA-256 digest/{print $NF; exit}')"
[ "$actual" = "$EXPECTED_SHA256" ] \
  || { echo "FAIL signing cert drift: got $actual want $EXPECTED_SHA256"; exit 1; }

echo "PASS app identity pinned: $EXPECTED_ID / SHA-256 $actual"
