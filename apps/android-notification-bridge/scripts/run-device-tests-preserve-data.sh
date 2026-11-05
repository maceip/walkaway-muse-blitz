#!/usr/bin/env bash
# Run on-device instrumentation WITHOUT wiping the installed (Meta-DAT-registered) app.
#
# Why this exists:
#   `./gradlew :app:connectedDebugAndroidTest` on this project replaces the installed app
#   (PackageManager firstInstallTime reset to 2026-11-05 07:25:19), which destroys the Meta
#   DAT registration token stored in app-private storage (shared_prefs/dat_registration.xml).
#   Meta DAT registration is a per-install consent (Meta AI app flow keyed on package +
#   signing cert + the locally stored token). A destroyed install therefore needs a human
#   re-approval in the Meta AI app — the repeated "approve in Meta AI" prompts were caused
#   by our test workflow, NOT by signing-key or applicationId changes (both verified
#   unchanged 2026-11-05: applicationId com.example.walkawaybridge, cert SHA-256 abdf2c3a...).
#
# This script instead:
#   1. Snapshots app-private data (run-as tar) as a safety net.
#   2. Installs app + test APK with `adb install -r` (in-place upgrade, data retained).
#   3. Runs instrumentation directly with `am instrument`.
#   4. Leaves the app installed; never uninstalls and never clears data.
#   5. Asserts PackageManager firstInstallTime is unchanged and dat_registration.xml survives.
#
# Usage:
#   scripts/run-device-tests-preserve-data.sh [fully.qualified.TestClass]
set -euo pipefail

PKG="com.example.walkawaybridge"
TEST_PKG="com.example.walkawaybridge.test"
RUNNER="androidx.test.runner.AndroidJUnitRunner"
APP_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BACKUP_DIR="/tmp/walkaway-appdata-backup"
CLASS="${1:-}"

cd "$APP_DIR"

serial="$(adb get-serialno)"
echo "device: $serial"

pm_field() { adb shell dumpsys package "$PKG" | grep -m1 "$1" | sed 's/^[[:space:]]*//'; }
have_dat_reg() { adb shell run-as "$PKG" ls shared_prefs/dat_registration.xml >/dev/null 2>&1; }

before_install="$(pm_field firstInstallTime || true)"
have_dat_reg && before_reg="present" || before_reg="absent"
echo "before: $before_install dat_registration=$before_reg"

# Safety-net snapshot of app-private data (debug build is debuggable -> run-as works).
mkdir -p "$BACKUP_DIR"
if adb shell run-as "$PKG" tar -cf - shared_prefs files 2>/dev/null > "$BACKUP_DIR/appdata.tar"; then
  echo "backup: $BACKUP_DIR/appdata.tar ($(wc -c < "$BACKUP_DIR/appdata.tar") bytes)"
else
  echo "backup: skipped (run-as tar failed; app may be absent)"
fi

./gradlew :app:assembleDebug :app:assembleDebugAndroidTest --console=plain -q

APP_APK="app/build/outputs/apk/debug/app-debug.apk"
TEST_APK="$(find app/build/outputs/apk/androidTest -name '*.apk' | head -1)"
[ -n "$TEST_APK" ] || { echo "test APK not found"; exit 1; }

# In-place upgrades only. NEVER `adb uninstall`, NEVER `pm clear`.
adb install -r "$APP_APK" >/dev/null
adb install -r "$TEST_APK" >/dev/null
echo "installed (upgrade-in-place): $APP_APK + $TEST_APK"

args=(-w -r)
[ -n "$CLASS" ] && args+=(-e class "$CLASS")
args+=("$TEST_PKG/$RUNNER")
echo "am instrument ${args[*]}"
set +e
out="$(adb shell am instrument "${args[@]}" 2>&1)"
rc=$?
set -e
echo "$out"

after_install="$(pm_field firstInstallTime || true)"
have_dat_reg && after_reg="present" || after_reg="absent"
echo "after: $after_install dat_registration=$after_reg"

fail=0
if [ "$before_install" != "$after_install" ]; then
  echo "FAIL: firstInstallTime changed ($before_install -> $after_install); app data was reset."
  echo "Restore attempt: adb shell run-as $PKG tar -xf - < $BACKUP_DIR/appdata.tar"
  fail=1
fi
if [ "$before_reg" = "present" ] && [ "$after_reg" != "present" ]; then
  echo "FAIL: dat_registration.xml lost; restore $BACKUP_DIR/appdata.tar before next glasses use."
  fail=1
fi
case "$out" in *"OK ("*) : ;; *) echo "FAIL: instrumentation did not report OK"; fail=1;; esac
[ "$rc" -eq 0 ] || fail=1

if [ "$fail" -eq 0 ]; then echo "PASS preserve-data device tests (firstInstallTime + DAT registration intact)"; fi
exit "$fail"
