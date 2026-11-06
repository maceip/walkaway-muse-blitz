#!/usr/bin/env bash
set -euo pipefail

BROWSER_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE="${BROWSER_DIR}/resources/icons/default.png"
OUTPUT="${1:?Usage: build-app-icon.sh /absolute/path/AppIcon.icns}"

for tool in sips iconutil; do
  command -v "${tool}" >/dev/null 2>&1 || { printf 'Missing required icon tool: %s\n' "${tool}" >&2; exit 1; }
done
[[ -f "${SOURCE}" ]] || { printf 'Missing browser icon: %s\n' "${SOURCE}" >&2; exit 1; }
[[ "${OUTPUT}" == /* && "${OUTPUT}" == *.icns ]] || { printf 'Output must be an absolute .icns path\n' >&2; exit 1; }

SIZE="$(sips -g pixelWidth -g pixelHeight "${SOURCE}")"
[[ "${SIZE}" == *'pixelWidth: 1024'* && "${SIZE}" == *'pixelHeight: 1024'* ]] || {
  printf 'Browser icon must be a 1024x1024 image\n' >&2
  exit 1
}

WORK="$(mktemp -d "${TMPDIR:-/tmp}/walkaway-app-icon.XXXXXX")"
trap 'rm -rf "${WORK}"' EXIT
ICONSET="${WORK}/AppIcon.iconset"
mkdir "${ICONSET}"
for size in 16 32 128 256 512; do
  sips -z "${size}" "${size}" "${SOURCE}" --out "${ICONSET}/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "${double}" "${double}" "${SOURCE}" --out "${ICONSET}/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "${ICONSET}" -o "${WORK}/AppIcon.icns"
[[ -s "${WORK}/AppIcon.icns" ]]
iconutil -c iconset "${WORK}/AppIcon.icns" -o "${WORK}/Verified.iconset"
cmp "${SOURCE}" "${WORK}/Verified.iconset/icon_512x512@2x.png"
cp "${WORK}/AppIcon.icns" "${OUTPUT}"
printf 'Browser app icon verified: %s\n' "${OUTPUT}"
