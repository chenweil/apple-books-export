#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
APPKIT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_DIR="$(cd "$APPKIT_DIR/.." && pwd)"

# SwiftPM 要在含有 Package.swift 的目录里运行。脚本已经算出了 APPKIT_DIR，
# 但此前两处 `swift build` 仍依赖调用者的当前工作目录，从仓库根执行会直接
# 报 "Could not find Package.swift"。这里显式切过去，让脚本可以从任意目录调用。
cd "$APPKIT_DIR"

CONFIG="${CONFIG:-release}"
# The default used to be the literal 0.1.8, left over from when the AppKit app
# carried its own hand-maintained version. Running this script with no
# arguments therefore produced a disk image named 0.1.8 around a Rust CLI whose
# --version reported 0.3.3 -- the exact split the release pipeline was changed
# to eliminate, still reachable by hand. Reading the crate manifest makes the
# local path agree with the tagged one. The pipeline passes APP_VERSION
# explicitly anyway, and this only supplies the fallback.
#
# That makes scripts/crate-version.sh a runtime dependency: it is resolved
# through $REPO_DIR, so the appkit/ tree has to sit inside the repository. It
# always has -- this script already reads $APPKIT_DIR/Resources/Info.plist and
# $REPO_DIR/target -- and it is the same kind of dependency as
# Scripts/release-channel.sh below.
APP_VERSION="${APP_VERSION:-$(bash "$REPO_DIR/scripts/crate-version.sh")}"
# CFBundleVersion is a build number, not a version, and the release pipeline
# passes the CI run number. Leaving it at a literal looks like an oversight, so:
# a commit count would satisfy "must rise on every build" but a commit count is
# not a build number, and stamping one into a public artifact would be a false
# statement about how it was made -- the same class of claim this repository
# keeps having to undo. ADR 0004 treats CFBundleVersion as diagnostic only, so
# there is nothing to keep consistent with, and the checked-in Info.plist
# template carries the same 9. The literal is a placeholder, not a claim.
#
# Note the template's *short* version is a different story: it still says 0.1.8
# while the default here is now whatever Cargo.toml says. Nothing ships that
# number -- the plist is copied and then overwritten below -- but the two are no
# longer coincidentally aligned, so a reader comparing them will see a mismatch
# that is expected rather than a bug.
BUILD_VERSION="${BUILD_VERSION:-9}"
MINIMUM_MACOS_VERSION="${MINIMUM_MACOS_VERSION:-14.0}"
ARCHITECTURE="${ARCHITECTURE:-$(uname -m)}"
RELEASE_NOTES="${RELEASE_NOTES:-}"
# One implementation of the channel rules, shared with the release workflow and
# with tests/headless_mainline.sh, so the rules only have to be changed in one
# place.
#
# This used to be a literal `channel -string "stable"`, which meant a
# prerelease release published a manifest claiming to be on the stable
# channel. That was a false statement inside a public artifact, not a live
# update-safety hole: UpdateChecker separately refuses any manifest whose
# version is a prerelease, so a prerelease was never actually offered to
# stable users. See release-channel.sh for the full reasoning.
CHANNEL="$(bash "$APPKIT_DIR/Scripts/release-channel.sh" "$APP_VERSION")"
RELEASE_URL="${RELEASE_URL:-https://github.com/chenweil/apple-books-export/releases/tag/v${APP_VERSION}}"
APP_NAME="Books Exporter.app"
DMG_NAME="Books-Exporter-${APP_VERSION}-unsigned.dmg"
DIST_DIR="${DIST_DIR:-$REPO_DIR/dist}"
UPDATE_MANIFEST_NAME="latest.json"
RUST_CLI_BIN="${RUST_CLI_BIN:-$REPO_DIR/target/release/apple-books-exporter}"

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
if [[ ! -x "$RUST_CLI_BIN" ]]; then
    echo "error: Rust CLI binary not found or not executable: $RUST_CLI_BIN" >&2
    echo "Build it with 'cargo build --release' from the Rust mainline, or set RUST_CLI_BIN to a native binary." >&2
    exit 1
fi

STAGING_DIR="$(mktemp -d "${TMPDIR:-/tmp}/books-exporter-dmg.XXXXXX")"
trap 'rm -rf "$STAGING_DIR"' EXIT

APP_DIR="$STAGING_DIR/$APP_NAME"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp "$BIN_PATH" "$APP_DIR/Contents/MacOS/BooksExporter"
# The app root, NOT Contents/. SwiftPM's generated Bundle.module accessor looks in
# exactly two places: Bundle.main.bundleURL (the .app root) and an absolute path
# inside the build tree. Contents/ is neither. A bundle there makes every Share
# Card render hit fatalError -- EXC_BREAKPOINT, not a memory fault -- on any
# machine that lacks this repo's .build directory, which is every machine a
# release reaches. v0.3.4 shipped it in Contents/ and crashed on the first card
# click. Nothing in CI caught it: inside the build tree the accessor's second
# candidate always resolves, so Bundle.module works in tests and only fails once
# the app is copied somewhere else.
cp -R "$RESOURCE_BUNDLE" "$APP_DIR/"
cp "$RUST_CLI_BIN" "$APP_DIR/Contents/Resources/apple-books-exporter"
cp "$APPKIT_DIR/Resources/Info.plist" "$APP_DIR/Contents/Info.plist"
cp "$APPKIT_DIR/Resources/AppIcon.icns" "$APP_DIR/Contents/Resources/AppIcon.icns"

plutil -replace CFBundleShortVersionString -string "$APP_VERSION" "$APP_DIR/Contents/Info.plist"
plutil -replace CFBundleVersion -string "$BUILD_VERSION" "$APP_DIR/Contents/Info.plist"
chmod +x "$APP_DIR/Contents/MacOS/BooksExporter"
chmod +x "$APP_DIR/Contents/Resources/apple-books-exporter"

# Intentionally unsigned: no Developer ID signing or notarization is performed.
ln -s /Applications "$STAGING_DIR/Applications"

# Check the staged app before spending minutes on hdiutil. This inspects the
# real directory that is about to become the DMG, not this script's own text.
bash "$APPKIT_DIR/Scripts/verify-resource-bundle-layout.sh" \
  "$APP_DIR" "$(basename "$RESOURCE_BUNDLE")"

# STAGE_ONLY stops here, with the staged app verified but no image built. CI
# uses it to exercise this script on every PR: the layout contract is the
# difference between a working app and an EXC_BREAKPOINT on first use, and no
# other check in the suite sees a packaged app at all. Skipping only hdiutil
# and the manifest keeps that check to seconds.
if [[ "${STAGE_ONLY:-0}" == "1" ]]; then
  plutil -lint "$APP_DIR/Contents/Info.plist" >/dev/null
  printf 'Staged and verified (STAGE_ONLY=1, no DMG written): %s\n' "$APP_DIR"
  exit 0
fi

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
plutil -insert channel -string "$CHANNEL" "$MANIFEST_PLIST"
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
