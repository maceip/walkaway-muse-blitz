#!/usr/bin/env bash
# Build, sign, notarize, and package Walkaway Browser (apps/hermes-browser) into
# a standalone macOS application bundle (Walkaway.app), disk image (Walkaway.dmg),
# and archive (Walkaway.zip).
#
# Usage:
#   scripts/build-macos-dmg.sh [--release|--debug] [--no-notarize] [--identity "Developer ID Application: ..."]
#
# Environment variables (picked up automatically in CI and local release builds):
#   APP_VERSION                  Marketing version string (default: 0.1.0)
#   BUILD_NUMBER                 CFBundleVersion (default: GITHUB_RUN_NUMBER or 1)
#   WALKAWAY_CODESIGN_IDENTITY   Full name or SHA-1 of the "Developer ID Application: ..." certificate
#   WALKAWAY_KEYCHAIN_PROFILE    Keychain profile created via `xcrun notarytool store-credentials`
#   APPLE_API_KEY_ID             App Store Connect API Key ID (10 chars)
#   APPLE_API_ISSUER_ID          App Store Connect API Issuer UUID
#   APPLE_API_PRIVATE_KEY        App Store Connect API Private Key (.p8 contents) or APPLE_API_KEY_PATH
#   APPLE_ID                     Apple ID email for notarytool
#   APPLE_APP_SPECIFIC_PASSWORD  App-specific password for APPLE_ID
#   APPLE_TEAM_ID                10-character Apple Developer Team ID

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BROWSER_DIR="${REPO_ROOT}/apps/hermes-browser"
ENTITLEMENTS="${BROWSER_DIR}/Walkaway.entitlements"
BUILD_ROOT="${REPO_ROOT}/build"
HERMES_ROOT="${HERMES_ROOT:-${HOME}/.cache/hermes-static_h}"
HERMES_SRC_DIR="${HERMES_SRC_DIR:-${HERMES_ROOT}/src}"
HERMES_BUILD_DIR="${HERMES_BUILD_DIR:-${HERMES_ROOT}/build-walkaway-debugger}"

BUILD_PROFILE="release"
DO_NOTARIZE=1
IDENTITY="${WALKAWAY_CODESIGN_IDENTITY:-${MUSE_CODESIGN_IDENTITY:-}}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release)
      BUILD_PROFILE="release"
      shift
      ;;
    --debug)
      BUILD_PROFILE="debug"
      shift
      ;;
    --no-notarize)
      DO_NOTARIZE=0
      shift
      ;;
    --identity)
      IDENTITY="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 1
      ;;
  esac
done

APP_VERSION="${APP_VERSION:-0.1.0}"
BUILD_NUMBER="${BUILD_NUMBER:-${GITHUB_RUN_NUMBER:-1}}"

echo "==> Walkaway Browser macOS DMG packaging (v${APP_VERSION}, build ${BUILD_NUMBER}, profile ${BUILD_PROFILE})"

# ---------------------------------------------------------------------------
# 1. Discover signing identity if not explicitly set
# ---------------------------------------------------------------------------
if [[ -z "${IDENTITY}" ]]; then
  IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
    | sed -n 's/.*\([0-9A-F]\{40\}\) "Developer ID Application: [^"]*".*/\1/p' \
    | head -n 1 || true)"
fi

if [[ -z "${IDENTITY}" ]]; then
  IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
    | sed -n 's/.*\([0-9A-F]\{40\}\) "Apple Development: [^"]*".*/\1/p' \
    | head -n 1 || true)"
  if [[ -n "${IDENTITY}" ]]; then
    DO_NOTARIZE=0
  fi
fi

if [[ -n "${IDENTITY}" ]]; then
  echo "==> Using codesigning identity: ${IDENTITY}"
else
  echo "==> No 'Developer ID Application' or 'Apple Development' certificate found in keychain; using ad-hoc Hardened Runtime signing (--sign -)"
  DO_NOTARIZE=0
fi

TEMP_API_KEY=""
cleanup_temp_key() {
  if [[ -n "${TEMP_API_KEY}" && -f "${TEMP_API_KEY}" ]]; then
    rm -f "${TEMP_API_KEY}"
  fi
}
trap cleanup_temp_key EXIT

if [[ -z "${APPLE_API_KEY_PATH:-}" && -n "${APPLE_API_PRIVATE_KEY:-}" ]]; then
  TEMP_API_KEY="$(mktemp /tmp/walkaway-notary-key.XXXXXX.p8)"
  chmod 600 "${TEMP_API_KEY}"
  printf '%s\n' "${APPLE_API_PRIVATE_KEY}" > "${TEMP_API_KEY}"
  APPLE_API_KEY_PATH="${TEMP_API_KEY}"
fi

APPLE_ISSUER="${APPLE_API_ISSUER_ID:-${APPLE_API_ISSUER:-}}"
KEYCHAIN_PROFILE="${WALKAWAY_KEYCHAIN_PROFILE:-${MUSE_KEYCHAIN_PROFILE:-}}"

run_notarytool() {
  if [[ -n "${KEYCHAIN_PROFILE}" ]]; then
    xcrun notarytool "$@" --keychain-profile "${KEYCHAIN_PROFILE}"
  elif [[ -n "${APPLE_API_KEY_PATH:-}" && -n "${APPLE_API_KEY_ID:-}" && -n "${APPLE_ISSUER}" ]]; then
    xcrun notarytool "$@" \
      --key "${APPLE_API_KEY_PATH}" \
      --key-id "${APPLE_API_KEY_ID}" \
      --issuer "${APPLE_ISSUER}"
  elif [[ -n "${APPLE_ID:-}" && -n "${APPLE_APP_SPECIFIC_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
    xcrun notarytool "$@" \
      --apple-id "${APPLE_ID}" \
      --password "${APPLE_APP_SPECIFIC_PASSWORD}" \
      --team-id "${APPLE_TEAM_ID}"
  else
    echo "ERROR: Notarization requested, but neither WALKAWAY_KEYCHAIN_PROFILE, APPLE_API_KEY_*, nor APPLE_ID/APPLE_APP_SPECIFIC_PASSWORD/APPLE_TEAM_ID are set." >&2
    return 1
  fi
}

has_notary_credentials() {
  [[ -n "${KEYCHAIN_PROFILE}" ]] \
    || [[ -n "${APPLE_API_KEY_PATH:-}" && -n "${APPLE_API_KEY_ID:-}" && -n "${APPLE_ISSUER}" ]] \
    || [[ -n "${APPLE_ID:-}" && -n "${APPLE_APP_SPECIFIC_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]
}

if [[ "${DO_NOTARIZE}" -eq 1 ]] && ! has_notary_credentials; then
  echo "==> No notarization credentials found in environment; skipping notarization step"
  DO_NOTARIZE=0
fi

# ---------------------------------------------------------------------------
# 2. Ensure Static Hermes is built, then build apps/hermes-browser
# ---------------------------------------------------------------------------
if [[ ! -x "${HERMES_BUILD_DIR}/bin/shermes" || ! -f "${HERMES_BUILD_DIR}/lib/libhermesvm.dylib" ]] || ! grep -q '^HERMES_ENABLE_DEBUGGER:BOOL=ON$' "${HERMES_BUILD_DIR}/CMakeCache.txt"; then
  echo "==> Building Static Hermes toolchain into ${HERMES_ROOT}"
  HERMES_ROOT="${HERMES_ROOT}" HERMES_SRC_DIR="${HERMES_SRC_DIR}" HERMES_BUILD_DIR="${HERMES_BUILD_DIR}" "${BROWSER_DIR}/scripts/build-hermes.sh"
fi

echo "==> Building hermes-blitz-browser (${BUILD_PROFILE})"
CARGO_ARGS=(--manifest-path "${BROWSER_DIR}/Cargo.toml")
if [[ "${BUILD_PROFILE}" == "release" ]]; then
  CARGO_ARGS+=(--release)
fi
HERMES_SRC_DIR="${HERMES_SRC_DIR}" HERMES_BUILD_DIR="${HERMES_BUILD_DIR}" cargo build "${CARGO_ARGS[@]}"

BUILT_BIN="${BROWSER_DIR}/target/${BUILD_PROFILE}/hermes-blitz-browser"
if [[ ! -x "${BUILT_BIN}" ]]; then
  echo "ERROR: Expected compiled browser binary at ${BUILT_BIN}" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# 3. Assemble Walkaway.app bundle + self-contained Frameworks dylibs
# ---------------------------------------------------------------------------
mkdir -p "${BUILD_ROOT}"
STAGE_APP="${BUILD_ROOT}/Walkaway.app"
rm -rf "${STAGE_APP}"
mkdir -p \
  "${STAGE_APP}/Contents/MacOS" \
  "${STAGE_APP}/Contents/Frameworks" \
  "${STAGE_APP}/Contents/Resources"

cp -f "${BUILT_BIN}" "${STAGE_APP}/Contents/MacOS/Walkaway"
chmod 755 "${STAGE_APP}/Contents/MacOS/Walkaway"
ln -s Walkaway "${STAGE_APP}/Contents/MacOS/hermes-blitz-browser"
cp "${BROWSER_DIR}/THIRD_PARTY_NOTICES.txt" "${STAGE_APP}/Contents/Resources/THIRD_PARTY_NOTICES.txt"
cp "${BROWSER_DIR}/native-tabs/LICENSE" "${STAGE_APP}/Contents/Resources/CompactTabStrip-LICENSE.txt"
cp "${BROWSER_DIR}/native-tabs/VENDOR.json" "${STAGE_APP}/Contents/Resources/CompactTabStrip-VENDOR.json"

xcrun swift-stdlib-tool --copy \
  --scan-executable "${STAGE_APP}/Contents/MacOS/Walkaway" \
  --destination "${STAGE_APP}/Contents/Frameworks" \
  --platform macosx

bash "${BROWSER_DIR}/scripts/build-app-icon.sh" "${STAGE_APP}/Contents/Resources/AppIcon.icns"

cat > "${STAGE_APP}/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleDisplayName</key>
	<string>Walkaway</string>
	<key>CFBundleExecutable</key>
	<string>Walkaway</string>
	<key>CFBundleIconFile</key>
	<string>AppIcon</string>
	<key>CFBundleIdentifier</key>
	<string>build.secure.walkaway.browser</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>Walkaway</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>${APP_VERSION}</string>
	<key>CFBundleVersion</key>
	<string>${BUILD_NUMBER}</string>
	<key>LSMinimumSystemVersion</key>
	<string>14.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSSupportsAutomaticGraphicsSwitching</key>
	<true/>
</dict>
</plist>
EOF

plutil -lint "${STAGE_APP}/Contents/Info.plist"
plutil -lint "${ENTITLEMENTS}"

# Bundle libhermesvm.dylib + any non-system dylibs (e.g. Homebrew OpenSSL) into Contents/Frameworks
echo "==> Bundling dynamic libraries into Walkaway.app/Contents/Frameworks"
cp -f "${HERMES_BUILD_DIR}/lib/libhermesvm.dylib" "${STAGE_APP}/Contents/Frameworks/libhermesvm.dylib"
chmod 755 "${STAGE_APP}/Contents/Frameworks/libhermesvm.dylib"
install_name_tool -id "@rpath/libhermesvm.dylib" "${STAGE_APP}/Contents/Frameworks/libhermesvm.dylib"

bundle_non_system_dylibs() {
  local target_bin="$1"
  while IFS= read -r dep; do
    [[ -n "${dep}" ]] || continue
    case "${dep}" in
      /System/*|/usr/lib/*|@rpath/*|@executable_path/*|@loader_path/*)
        continue
        ;;
    esac
    if [[ -f "${dep}" ]]; then
      local base
      base="$(basename "${dep}")"
      local dst="${STAGE_APP}/Contents/Frameworks/${base}"
      if [[ ! -f "${dst}" ]]; then
        echo "    bundling ${dep} -> Contents/Frameworks/${base}"
        cp -L -f "${dep}" "${dst}"
        chmod 755 "${dst}"
        install_name_tool -id "@rpath/${base}" "${dst}"
      fi
      install_name_tool -change "${dep}" "@rpath/${base}" "${target_bin}"
    fi
  done < <(otool -L "${target_bin}" | tail -n +2 | awk '{print $1}')
}

bundle_non_system_dylibs "${STAGE_APP}/Contents/MacOS/Walkaway"
for fw_dylib in "${STAGE_APP}/Contents/Frameworks/"*.dylib; do
  [[ -f "${fw_dylib}" ]] || continue
  bundle_non_system_dylibs "${fw_dylib}"
done

# Ensure @executable_path/../Frameworks is on LC_RPATH and remove build-machine absolute rpaths
if ! otool -l "${STAGE_APP}/Contents/MacOS/Walkaway" | grep -A2 LC_RPATH | grep -q "@executable_path/../Frameworks"; then
  install_name_tool -add_rpath "@executable_path/../Frameworks" "${STAGE_APP}/Contents/MacOS/Walkaway"
fi
while IFS= read -r rpath_entry; do
  [[ -n "${rpath_entry}" ]] || continue
  if [[ "${rpath_entry}" == /* ]]; then
    install_name_tool -delete_rpath "${rpath_entry}" "${STAGE_APP}/Contents/MacOS/Walkaway" || true
  fi
done < <(otool -l "${STAGE_APP}/Contents/MacOS/Walkaway" | awk '/cmd LC_RPATH/{getline; getline; print $2}')

echo "==> Verifying bundled dylib linkage on Walkaway.app/Contents/MacOS/Walkaway"
otool -L "${STAGE_APP}/Contents/MacOS/Walkaway"

# ---------------------------------------------------------------------------
# 4. Code-sign Walkaway.app with Hardened Runtime + Entitlements
# ---------------------------------------------------------------------------
if [[ -n "${IDENTITY}" ]]; then
  echo "==> Signing Walkaway.app with Developer ID + Hardened Runtime"
  for fw_dylib in "${STAGE_APP}/Contents/Frameworks/"*.dylib; do
    [[ -f "${fw_dylib}" ]] || continue
    codesign --force --options runtime --timestamp --sign "${IDENTITY}" "${fw_dylib}"
  done
  codesign --force --options runtime --timestamp \
    --entitlements "${ENTITLEMENTS}" \
    --sign "${IDENTITY}" \
    "${STAGE_APP}"
else
  echo "==> Ad-hoc signing Walkaway.app with Hardened Runtime entitlements"
  for fw_dylib in "${STAGE_APP}/Contents/Frameworks/"*.dylib; do
    [[ -f "${fw_dylib}" ]] || continue
    codesign --force --options runtime --sign - "${fw_dylib}"
  done
  codesign --force --deep --options runtime \
    --entitlements "${ENTITLEMENTS}" \
    --sign - \
    "${STAGE_APP}"
fi

echo "==> Verifying Walkaway.app signature"
codesign --verify --deep --strict --verbose=2 "${STAGE_APP}"

# ---------------------------------------------------------------------------
# 5. Notarize & staple Walkaway.app before packaging into DMG + ZIP
# ---------------------------------------------------------------------------
NOTARIZED_STATUS="not-notarized"
if [[ "${DO_NOTARIZE}" -eq 1 ]]; then
  APP_NOTARY_ZIP="${BUILD_ROOT}/Walkaway-notarize.zip"
  rm -f "${APP_NOTARY_ZIP}"
  echo "==> Creating archive ${APP_NOTARY_ZIP} for app notarization"
  ditto -c -k --keepParent "${STAGE_APP}" "${APP_NOTARY_ZIP}"

  echo "==> Submitting Walkaway.app to Apple notary service"
  run_notarytool submit "${APP_NOTARY_ZIP}" --wait
  rm -f "${APP_NOTARY_ZIP}"

  echo "==> Stapling notarization ticket to Walkaway.app"
  xcrun stapler staple "${STAGE_APP}"
  xcrun stapler validate "${STAGE_APP}"
  spctl --assess --type execute --verbose=4 "${STAGE_APP}"
  NOTARIZED_STATUS="notarized"
fi

# ---------------------------------------------------------------------------
# 6. Create Walkaway.zip and Walkaway.dmg
# ---------------------------------------------------------------------------
ZIP_OUT="${BUILD_ROOT}/Walkaway.zip"
VERSIONED_ZIP_OUT="${BUILD_ROOT}/walkaway-macos-${APP_VERSION}-${BUILD_PROFILE}.zip"
rm -f "${ZIP_OUT}" "${VERSIONED_ZIP_OUT}"
echo "==> Creating ${ZIP_OUT}"
ditto -c -k --sequesterRsrc --keepParent "${STAGE_APP}" "${ZIP_OUT}"
cp -f "${ZIP_OUT}" "${VERSIONED_ZIP_OUT}"

DMG_STAGE="${BUILD_ROOT}/dmg-root"
VERSIONED_DMG_OUT="${BUILD_ROOT}/walkaway-macos-${APP_VERSION}-${BUILD_PROFILE}.dmg"
DMG_OUT="${BUILD_ROOT}/Walkaway.dmg"
rm -rf "${DMG_STAGE}" "${VERSIONED_DMG_OUT}" "${DMG_OUT}"
mkdir -p "${DMG_STAGE}"
ditto "${STAGE_APP}" "${DMG_STAGE}/Walkaway.app"
ln -s /Applications "${DMG_STAGE}/Applications"

echo "==> Building compressed disk image ${VERSIONED_DMG_OUT}"
sync
DMG_CREATED=0
for attempt in 1 2 3 4 5; do
  rm -f "${VERSIONED_DMG_OUT}"
  if hdiutil create \
    -volname "Walkaway" \
    -srcfolder "${DMG_STAGE}" \
    -ov \
    -format UDZO \
    -imagekey zlib-level=9 \
    "${VERSIONED_DMG_OUT}"; then
    DMG_CREATED=1
    break
  fi
  echo "WARN: hdiutil create attempt ${attempt}/5 failed; retrying in $((attempt * 2))s..." >&2
  sync
  sleep $((attempt * 2))
done
if [[ "${DMG_CREATED}" -ne 1 ]]; then
  echo "ERROR: Failed to create DMG after 5 attempts." >&2
  exit 1
fi
rm -rf "${DMG_STAGE}"

if [[ -n "${IDENTITY}" ]]; then
  echo "==> Signing DMG with Developer ID"
  codesign --force --timestamp --sign "${IDENTITY}" "${VERSIONED_DMG_OUT}"
else
  echo "==> Ad-hoc signing DMG"
  codesign --force --sign - "${VERSIONED_DMG_OUT}"
fi

if [[ "${DO_NOTARIZE}" -eq 1 ]]; then
  echo "==> Submitting DMG to Apple notary service"
  run_notarytool submit "${VERSIONED_DMG_OUT}" --wait

  echo "==> Stapling notarization ticket to DMG"
  xcrun stapler staple "${VERSIONED_DMG_OUT}"
  xcrun stapler validate "${VERSIONED_DMG_OUT}"
  spctl --assess --type open --context context:primary-signature --verbose=4 "${VERSIONED_DMG_OUT}"
fi

cp -f "${VERSIONED_DMG_OUT}" "${DMG_OUT}"
printf '%s\n' "${NOTARIZED_STATUS}" > "${BUILD_ROOT}/notarization.status"

# ---------------------------------------------------------------------------
# 7. Mount the built DMG headlessly and execute Walkaway.app self-tests
# ---------------------------------------------------------------------------
VERIFY_MOUNT="$(mktemp -d /tmp/walkaway-dmg-verify.XXXXXX)"
cleanup_mount() {
  hdiutil detach "${VERIFY_MOUNT}" -force >/dev/null 2>&1 || true
  rmdir "${VERIFY_MOUNT}" >/dev/null 2>&1 || true
}
trap cleanup_mount EXIT

echo "==> Mounting ${VERSIONED_DMG_OUT} headlessly to verify packaged Walkaway.app"
hdiutil attach "${VERSIONED_DMG_OUT}" -mountpoint "${VERIFY_MOUNT}" -nobrowse -readonly -quiet
test -d "${VERIFY_MOUNT}/Walkaway.app"
test -L "${VERIFY_MOUNT}/Applications"
test -s "${VERIFY_MOUNT}/Walkaway.app/Contents/Resources/AppIcon.icns"
cmp "${STAGE_APP}/Contents/Resources/AppIcon.icns" "${VERIFY_MOUNT}/Walkaway.app/Contents/Resources/AppIcon.icns"
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIconFile' "${VERIFY_MOUNT}/Walkaway.app/Contents/Info.plist")" == "AppIcon" ]]
codesign --verify --deep --strict --verbose=2 "${VERIFY_MOUNT}/Walkaway.app"
HERMES_HIDDEN_WINDOW=1 "${VERIFY_MOUNT}/Walkaway.app/Contents/MacOS/Walkaway" --selftest
HERMES_HIDDEN_WINDOW=1 "${VERIFY_MOUNT}/Walkaway.app/Contents/MacOS/Walkaway" --supervisortest
HERMES_HIDDEN_WINDOW=1 "${VERIFY_MOUNT}/Walkaway.app/Contents/MacOS/Walkaway" --uitest
HERMES_HIDDEN_WINDOW=1 "${VERIFY_MOUNT}/Walkaway.app/Contents/MacOS/Walkaway" --debuggertest
HERMES_HIDDEN_WINDOW=1 "${VERIFY_MOUNT}/Walkaway.app/Contents/MacOS/Walkaway" --nativetabtest
HERMES_HIDDEN_WINDOW=1 "${VERIFY_MOUNT}/Walkaway.app/Contents/MacOS/Walkaway" --tabhosttest
cleanup_mount
trap cleanup_temp_key EXIT

echo "==> Done:"
echo "    DMG (versioned): ${VERSIONED_DMG_OUT}"
echo "    DMG (canonical): ${DMG_OUT}"
echo "    ZIP (archive):   ${ZIP_OUT}"
echo "    Notarization:    ${NOTARIZED_STATUS}"
