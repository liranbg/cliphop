#!/bin/bash
set -euo pipefail

# ─── Cliphop Installer ──────────────────────────────────────────────────────
# Usage: curl -fsSL https://raw.githubusercontent.com/liranbg/cliphop/main/scripts/install.sh | sh

REPO="liranbg/cliphop"
APP_NAME="Cliphop"
INSTALL_DIR="/Applications"

# ─── Helpers ─────────────────────────────────────────────────────────────────

info()  { printf '  \033[1;34m==>\033[0m %s\n' "$*"; }
error() { printf '  \033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

# ─── Checks ──────────────────────────────────────────────────────────────────

[ "$(uname)" = "Darwin" ] || error "Cliphop only runs on macOS."
command -v curl  >/dev/null || error "curl is required."
command -v hdiutil >/dev/null || error "hdiutil is required."

# ─── Resolve latest version ─────────────────────────────────────────────────

info "Fetching latest release..."
DOWNLOAD_URL=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"browser_download_url".*\.dmg"' \
  | head -1 \
  | sed 's/.*"browser_download_url": *"\(.*\)"/\1/')

[ -n "$DOWNLOAD_URL" ] || error "Could not find a DMG in the latest release. Check https://github.com/${REPO}/releases"

VERSION=$(echo "$DOWNLOAD_URL" | sed 's|.*/download/\(v[^/]*\)/.*|\1|')
info "Latest version: ${VERSION}"

# ─── Download ────────────────────────────────────────────────────────────────

TMPDIR_INSTALL=$(mktemp -d)
trap 'rm -rf "$TMPDIR_INSTALL"' EXIT

DMG_PATH="${TMPDIR_INSTALL}/${APP_NAME}.dmg"
info "Downloading ${APP_NAME}.dmg..."
curl -fSL --progress-bar -o "$DMG_PATH" "$DOWNLOAD_URL"

# ─── Mount & Install ────────────────────────────────────────────────────────

info "Installing to ${INSTALL_DIR}/${APP_NAME}.app..."

MOUNT_POINT="${TMPDIR_INSTALL}/mount"
mkdir -p "$MOUNT_POINT"
hdiutil attach -nobrowse -mountpoint "$MOUNT_POINT" "$DMG_PATH" -quiet

if [ -d "${INSTALL_DIR}/${APP_NAME}.app" ]; then
    rm -rf "${INSTALL_DIR}/${APP_NAME}.app"
fi
cp -R "${MOUNT_POINT}/${APP_NAME}.app" "${INSTALL_DIR}/"

hdiutil detach "$MOUNT_POINT" -quiet

# ─── Done ────────────────────────────────────────────────────────────────────

info "${APP_NAME} ${VERSION} installed to ${INSTALL_DIR}/${APP_NAME}.app"
echo ""
echo "  To launch:  open -a Cliphop"
echo "  To uninstall: rm -rf /Applications/Cliphop.app"
echo ""
echo "  Note: On first launch, macOS will prompt for Accessibility permissions."
echo "  Grant access in System Settings > Privacy & Security > Accessibility."
