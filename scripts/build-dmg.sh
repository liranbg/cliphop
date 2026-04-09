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
    <key>CFBundleShortVersionString</key>
    <string>${BUNDLE_VERSION}</string>
    <key>CFBundleExecutable</key>
    <string>cliphop</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleIconFile</key>
    <string>Cliphop</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
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
# A stable code-signing identity is required for Keychain access and
# Accessibility permissions to persist across rebuilds.
#
# Priority:
#   1. $SIGNING_IDENTITY env var (e.g. "Developer ID Application: ...")
#   2. "Cliphop Signing" self-signed cert (created by scripts/setup-signing.sh)
#   3. Ad-hoc "-" (fallback — Keychain and Accessibility will NOT persist)
#
if [ -z "${SIGNING_IDENTITY:-}" ]; then
    if security find-identity -v -p codesigning | grep -q "Cliphop Signing"; then
        SIGNING_IDENTITY="Cliphop Signing"
    else
        SIGNING_IDENTITY="-"
        echo "WARNING: No signing certificate found. Keychain access and"
        echo "         Accessibility permissions will not work reliably."
        echo "         Run: ./scripts/setup-signing.sh"
    fi
fi

echo "==> Signing bundle (identity: ${SIGNING_IDENTITY})..."
# Sign the inner binary first, then the outer bundle. Using --deep alone
# can leave Info.plist unbound, causing "invalid Info.plist" errors from
# the Security framework (Keychain access, etc.).
codesign --force --sign "$SIGNING_IDENTITY" "${BUNDLE_DIR}/Contents/MacOS/cliphop"
codesign --force --sign "$SIGNING_IDENTITY" "$BUNDLE_DIR"

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
