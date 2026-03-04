#!/bin/bash
set -euo pipefail

# ─── Configuration ────────────────────────────────────────────────────────────

APP_NAME="Cliphop"
BUNDLE_DIR="target/${APP_NAME}.app"
DMG_DIR="target/dmg"
DMG_NAME="target/${APP_NAME}.dmg"

# ─── 1. Build ─────────────────────────────────────────────────────────────────

echo "==> Building release binary..."
cargo build --release

# ─── 2. App Bundle ────────────────────────────────────────────────────────────

echo "==> Creating app bundle..."
rm -rf "$BUNDLE_DIR"
mkdir -p "${BUNDLE_DIR}/Contents/MacOS"
mkdir -p "${BUNDLE_DIR}/Contents/Resources"

# Copy the compiled binary
cp target/release/cliphop "${BUNDLE_DIR}/Contents/MacOS/"

# Generate Info.plist, pulling the version directly from Cargo.toml
BUNDLE_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
cat > "${BUNDLE_DIR}/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Cliphop</string>
    <key>CFBundleIdentifier</key>
    <string>com.cliphop.app</string>
    <key>CFBundleVersion</key>
    <string>${BUNDLE_VERSION}</string>
    <key>CFBundleIconFile</key>
    <string>Cliphop</string>
    <key>LSUIElement</key>
    <true/>
</dict>
</plist>
EOF

# ─── 3. App Icon ──────────────────────────────────────────────────────────────

echo "==> Generating app icon..."
# Render all required PNG sizes from the SF Symbol into a temporary .iconset
swift scripts/generate-icon.swift target
# Convert the .iconset folder into a single .icns file, then clean up
iconutil -c icns target/Cliphop.iconset -o "${BUNDLE_DIR}/Contents/Resources/Cliphop.icns"
rm -rf target/Cliphop.iconset

# ─── 4. Code Sign ─────────────────────────────────────────────────────────────
#
# TCC (Accessibility permission) ties access to the app's code-signing identity.
# With a real Developer ID the identity is "team_id + bundle_id", so permissions
# survive rebuilds.  With ad-hoc ("-") they are tied to the binary hash and will
# be revoked on every new build — set SIGNING_IDENTITY to your cert to fix this.
#
#   export SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
#
SIGNING_IDENTITY="${SIGNING_IDENTITY:--}"   # default: ad-hoc

echo "==> Signing bundle (identity: ${SIGNING_IDENTITY})..."
codesign --force --deep --sign "$SIGNING_IDENTITY" "$BUNDLE_DIR"

# ─── 5. DMG ───────────────────────────────────────────────────────────────────

echo "==> Creating DMG..."
rm -rf "$DMG_DIR"
mkdir -p "$DMG_DIR"

# Place the app bundle and an /Applications symlink so users can drag-install
cp -r "$BUNDLE_DIR" "$DMG_DIR/"
ln -s /Applications "$DMG_DIR/Applications"

# Build a compressed read-only disk image (UDZO = zlib-compressed UDIF)
rm -f "$DMG_NAME"
hdiutil create -volname "$APP_NAME" \
    -srcfolder "$DMG_DIR" \
    -ov -format UDZO \
    "$DMG_NAME"

rm -rf "$DMG_DIR"

# ─── Done ─────────────────────────────────────────────────────────────────────

echo "==> Done: $DMG_NAME"
