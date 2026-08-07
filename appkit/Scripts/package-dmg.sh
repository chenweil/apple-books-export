#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
APPKIT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_DIR="$(cd "$APPKIT_DIR/.." && pwd)"

CONFIG="${CONFIG:-release}"
APP_VERSION="${APP_VERSION:-0.1.8}"
BUILD_VERSION="${BUILD_VERSION:-9}"
MINIMUM_MACOS_VERSION="${MINIMUM_MACOS_VERSION:-14.0}"
ARCHITECTURE="${ARCHITECTURE:-$(uname -m)}"
RELEASE_NOTES="${RELEASE_NOTES:-}"
RELEASE_URL="${RELEASE_URL:-https://github.com/chenweil/apple-books-export/releases/tag/v${APP_VERSION}}"
APP_NAME="Books Exporter.app"
DMG_NAME="Books-Exporter-${APP_VERSION}-unsigned.dmg"
DIST_DIR="${DIST_DIR:-$REPO_DIR/dist}"
UPDATE_MANIFEST_NAME="latest.json"

BIN_DIR="$(swift build -c "$CONFIG" --show-bin-path)"
BIN_PATH="$BIN_DIR/BooksExporter"
RESOURCE_BUNDLE="$BIN_DIR/BooksExporter_BooksExporterCore.bundle"
swift build -c "$CONFIG"
if [[ ! -x "$BIN_PATH" ]]; then
    echo "error: executable not found: $BIN_PATH" >&2
    exit 1
fi
if [[ ! -d "$RESOURCE_BUNDLE" ]]; then
    echo "error: Share Card resource bundle not found: $RESOURCE_BUNDLE" >&2
    exit 1
fi

STAGING_DIR="$(mktemp -d "${TMPDIR:-/tmp}/books-exporter-dmg.XXXXXX")"
trap 'rm -rf "$STAGING_DIR"' EXIT

APP_DIR="$STAGING_DIR/$APP_NAME"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp "$BIN_PATH" "$APP_DIR/Contents/MacOS/BooksExporter"
cp -R "$RESOURCE_BUNDLE" "$APP_DIR/Contents/"
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

MANIFEST_PLIST="$STAGING_DIR/update-manifest.plist"
MANIFEST_JSON="$STAGING_DIR/$UPDATE_MANIFEST_NAME"
UPDATE_MANIFEST_PATH="$DIST_DIR/$UPDATE_MANIFEST_NAME"

plutil -create xml1 "$MANIFEST_PLIST"
plutil -insert schema_version -integer 1 "$MANIFEST_PLIST"
plutil -insert channel -string "stable" "$MANIFEST_PLIST"
plutil -insert version -string "$APP_VERSION" "$MANIFEST_PLIST"
plutil -insert minimum_macos -string "$MINIMUM_MACOS_VERSION" "$MANIFEST_PLIST"
plutil -insert architectures -array "$MANIFEST_PLIST"
plutil -insert architectures.0 -string "$ARCHITECTURE" "$MANIFEST_PLIST"
plutil -insert release_url -string "$RELEASE_URL" "$MANIFEST_PLIST"
plutil -insert notes -string "$RELEASE_NOTES" "$MANIFEST_PLIST"
plutil -lint "$MANIFEST_PLIST" >/dev/null
plutil -convert json -o "$MANIFEST_JSON" "$MANIFEST_PLIST"
cp "$MANIFEST_JSON" "$UPDATE_MANIFEST_PATH"
echo "Created update manifest: $UPDATE_MANIFEST_PATH"
