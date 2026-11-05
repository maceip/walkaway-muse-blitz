#!/usr/bin/env bash
# Deliver a public HTTPS debug web app to Meta AI using Meta's
# fb-viewapp://web_app_deep_link redirect. The user still confirms inside
# Meta AI unless --tap-confirm is passed after visually checking the prompt.
set -euo pipefail

APP_NAME="${WALKAWAY_GLASSES_APP_NAME:-Walkaway}"
APP_URL="${WALKAWAY_GLASSES_APP_URL:-}"
SERIAL="${ADB_SERIAL:-}"
TAP_CONFIRM=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --name) APP_NAME="$2"; shift 2 ;;
    --url) APP_URL="$2"; shift 2 ;;
    --serial) SERIAL="$2"; shift 2 ;;
    --tap-confirm) TAP_CONFIRM=true; shift ;;
    *) echo "Unknown argument: $1" >&2; exit 64 ;;
  esac
done

if [[ ! "$APP_URL" =~ ^https:// ]]; then
  echo "WALKAWAY_GLASSES_APP_URL or --url must be a public https URL" >&2
  exit 64
fi

run_adb() {
  if [[ -n "$SERIAL" ]]; then
    adb -s "$SERIAL" "$@"
  else
    adb "$@"
  fi
}

if [[ -z "$SERIAL" ]]; then
  SERIAL="$(adb devices | awk '$2 == "device" {print $1}')"
  count="$(printf '%s\n' "$SERIAL" | sed '/^$/d' | wc -l | tr -d ' ')"
  if [[ "$count" != "1" ]]; then
    echo "Expected exactly one ADB device; found: ${SERIAL:-none}" >&2
    exit 69
  fi
fi

run_adb shell pm path com.facebook.stella >/dev/null

encoded_name="$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$APP_NAME")"
encoded_url="$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$APP_URL")"
deeplink="fb-viewapp://web_app_deep_link?appName=${encoded_name}&appUrl=${encoded_url}"

echo "Launching Meta AI web-app deep link on $SERIAL"
echo "App name: $APP_NAME"
echo "App URL:  $APP_URL"
run_adb shell am start -W -a android.intent.action.VIEW -d "$deeplink"
sleep 2

window_state="$(run_adb shell dumpsys window | grep -E 'mCurrentFocus|mFocusedApp|mDreamingLockscreen|mShowingLockscreen' || true)"
printf '%s\n' "$window_state"
if ! printf '%s\n' "$window_state" | grep -q 'com.facebook.stella'; then
  echo "Warning: Meta AI (com.facebook.stella) is not focused." >&2
fi

if [[ "$TAP_CONFIRM" != true ]]; then
  echo "Deep link delivered. Confirm inside Meta AI, or rerun with --tap-confirm."
  exit 0
fi

remote_dump=/sdcard/walkaway-webapp-install.xml
run_adb shell uiautomator dump "$remote_dump" >/dev/null
run_adb exec-out cat "$remote_dump" >/tmp/walkaway-webapp-install.xml
coords="$(python3 - <<'PY'
import re
import xml.etree.ElementTree as ET

root = ET.parse('/tmp/walkaway-webapp-install.xml').getroot()
for node in root.iter('node'):
    if node.attrib.get('clickable') != 'true':
        continue
    label = node.attrib.get('text') or node.attrib.get('content-desc') or ''
    if label not in {'Connect', 'Add', 'Add app', 'Install', 'Continue'}:
        continue
    match = re.fullmatch(r'\[(\d+),(\d+)\]\[(\d+),(\d+)\]', node.attrib.get('bounds', ''))
    if match:
        x1, y1, x2, y2 = map(int, match.groups())
        print(f'{(x1+x2)//2} {(y1+y2)//2} {label}')
        break
PY
)"
if [[ -z "$coords" ]]; then
  echo "No exact Connect/Add/Install/Continue confirmation button is visible" >&2
  exit 3
fi
read -r tap_x tap_y tap_label <<<"$coords"
echo "Tapping '$tap_label' at $tap_x,$tap_y"
run_adb shell input tap "$tap_x" "$tap_y"
sleep 2
run_adb shell dumpsys window | grep -E 'mCurrentFocus|mFocusedApp|mDreamingLockscreen|mShowingLockscreen' || true
