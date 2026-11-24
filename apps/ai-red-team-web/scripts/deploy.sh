#!/usr/bin/env bash
#
# Deploy the AI Red Team web app to the host.
#
#   ./scripts/deploy.sh <host> [user]
#
# The build is done locally, uploaded as a release directory, and the
# `current` symlink is flipped. The systemd unit is installed once and then
# restarted, so a failed release never takes the site down: the old symlink
# stays in place until the new one is live.
set -euo pipefail

HOST="${1:-axp.computer}"
USER="${2:-azureuser}"
APP="redteam-web"
ROOT="/opt/${APP}"
# This script lives at apps/ai-red-team-web/scripts/, so the monorepo root is
# three levels up, not two.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
APP_DIR="${REPO_ROOT}/apps/ai-red-team-web"
KEY="${DEPLOY_KEY:-${HOME}/Downloads/coprocessor_keycopro.pem}"
RELEASE="$(date +%Y%m%d-%H%M%S)"

SSH=(ssh -i "${KEY}" -o StrictHostKeyChecking=accept-new -o ConnectTimeout=25 "${USER}@${HOST}")
SCP=(scp -i "${KEY}" -o StrictHostKeyChecking=accept-new -o ConnectTimeout=25)

echo "==> building ${APP}"
cd "${APP_DIR}"
if [ ! -d node_modules ]; then
  npm ci --no-audit --no-fund
fi
npm run build

echo "==> uploading release ${RELEASE}"
RELEASE_TGZ="$(mktemp -t redteam-web-XXXXXX.tar.gz)"
# COPYFILE_DISABLE keeps macOS resource forks (._*) out of the release and
# --no-xattrs keeps the extended attributes out of it: they are meaningless
# on Linux and only make the tar warn on every file. The build already ran
# the media pipeline, so the tarball carries the scenes, the films and the
# space plate with it.
COPYFILE_DISABLE=1 tar --no-xattrs --exclude='._*' --exclude='.DS_Store' \
  -czf "${RELEASE_TGZ}" -C "${APP_DIR}" dist server package.json
"${SCP[@]}" "${RELEASE_TGZ}" "${USER}@${HOST}:/tmp/${APP}-${RELEASE}.tar.gz"

echo "==> installing release on ${HOST}"
"${SSH[@]}" bash -s "${APP}" "${RELEASE}" <<'REMOTE'
set -euo pipefail
APP="$1"
RELEASE="$2"
ROOT="/opt/${APP}"
# The release directory is made first and the archive is unpacked straight
# into it. The archive holds dist/, server/ and package.json at its root, so
# they land as /opt/<app>/releases/<stamp>/dist and so on, which is what the
# `current` symlink points at. Moving the pieces into place afterwards needs
# the directory to exist too, and skipping that step is what used to fail
# the release with "chown: cannot access /opt/<app>/releases/<stamp>".
sudo mkdir -p "${ROOT}/releases/${RELEASE}"
sudo tar -xzf "/tmp/${APP}-${RELEASE}.tar.gz" -C "${ROOT}/releases/${RELEASE}"
sudo ln -sfn "${ROOT}/releases/${RELEASE}" "${ROOT}/current"
sudo chown -R root:root "${ROOT}/releases/${RELEASE}"
sudo rm -f "/tmp/${APP}-${RELEASE}.tar.gz"
# keep the last few releases, drop the rest
sudo ls -1dt "${ROOT}"/releases/* 2>/dev/null | tail -n +4 | xargs -r sudo rm -rf
echo "current -> $(readlink "${ROOT}/current")"
REMOTE

echo "==> restarting ${APP}.service"
"${SSH[@]}" "sudo systemctl restart ${APP}.service && sudo systemctl is-active ${APP}.service"

echo "==> done"
