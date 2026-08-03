#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
APPKIT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_DIR="$(cd "$APPKIT_DIR/.." && pwd)"

CONFIG="${CONFIG:-release}"
APP_VERSION="${APP_VERSION:-0.1.0}"
BUILD_VERSION="${BUILD_VERSION:-1}"
APP_NAME="Books Exporter.app"
DMG_NAME="Books-Exporter-${APP_VERSION}-unsigned.dmg"
DIST_DIR="${DIST_DIR:-$REPO_DIR/dist}"

BIN_DIR="$(swift build -c "$CONFIG" --show-bin-path)"
BIN_PATH="$BIN_DIR/BooksExporter"
if [[ ! -x "$BIN_PATH" ]]; then
    swift build -c "$CONFIG"
fi
if [[ ! -x "$BIN_PATH" ]]; then
    echo "error: executable not found: $BIN_PATH" >&2
    exit 1
fi

STAGING_DIR="$(mktemp -d "${TMPDIR:-/tmp}/books-exporter-dmg.XXXXXX")"
trap 'rm -rf "$STAGING_DIR"' EXIT

APP_DIR="$STAGING_DIR/$APP_NAME"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp "$BIN_PATH" "$APP_DIR/Contents/MacOS/BooksExporter"
cp "$APPKIT_DIR/Resources/Info.plist" "$APP_DIR/Contents/Info.plist"
cp "$APPKIT_DIR/Resources/AppIcon.icns" "$APP_DIR/Contents/Resources/AppIcon.icns"

plutil -replace CFBundleShortVersionString -string "$APP_VERSION" "$APP_DIR/Contents/Info.plist"
plutil -replace CFBundleVersion -string "$BUILD_VERSION" "$APP_DIR/Contents/Info.plist"
chmod +x "$APP_DIR/Contents/MacOS/BooksExporter"

# Intentionally unsigned: no Developer ID signing or notarization is performed.
ln -s /Applications "$STAGING_DIR/Applications"

mkdir -p "$DIST_DIR"
DMG_PATH="$DIST_DIR/$DMG_NAME"
hdiutil create \
    -volname "Books Exporter" \
    -srcfolder "$STAGING_DIR" \
    -ov \
    -format UDZO \
    "$DMG_PATH"

plutil -lint "$APP_DIR/Contents/Info.plist" >/dev/null
echo "Created unsigned DMG: $DMG_PATH"
