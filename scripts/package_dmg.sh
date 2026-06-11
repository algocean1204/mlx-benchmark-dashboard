#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_DIR="$PROJECT_ROOT/app"
DIST_DIR="$PROJECT_ROOT/dist"

VERSION="$(grep '^version:' "$APP_DIR/pubspec.yaml" | awk '{print $2}' | cut -d+ -f1)"
APP_BUNDLE_NAME="app.app"
DMG_NAME="AI_Dashboard-${VERSION}.dmg"
STAGING_DIR="$DIST_DIR/dmg_staging"
APP_BUNDLE="$APP_DIR/build/macos/Build/Products/Release/$APP_BUNDLE_NAME"

echo "==> Building Flutter macOS release (v${VERSION})"
cd "$APP_DIR"
flutter build macos --release

echo "==> Verifying app bundle contents"
DYLIB_PATH="$APP_BUNDLE/Contents/Frameworks/libaidash_frb.dylib"
PYTHON_ADAPTERS="$APP_BUNDLE/Contents/Resources/python/adapters"

if [[ ! -f "$DYLIB_PATH" ]]; then
  echo "error: missing bundled dylib at $DYLIB_PATH" >&2
  exit 1
fi
if [[ ! -d "$PYTHON_ADAPTERS" ]]; then
  echo "error: missing bundled python adapters at $PYTHON_ADAPTERS" >&2
  exit 1
fi
echo "OK: $DYLIB_PATH"
echo "OK: $PYTHON_ADAPTERS"

echo "==> Ad-hoc codesign"
codesign --force --deep -s - "$APP_BUNDLE"

echo "==> Creating DMG"
mkdir -p "$DIST_DIR"
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR"
cp -R "$APP_BUNDLE" "$STAGING_DIR/"
ln -sf /Applications "$STAGING_DIR/Applications"

DMG_PATH="$DIST_DIR/$DMG_NAME"
rm -f "$DMG_PATH"
hdiutil create \
  -volname "AI Dashboard" \
  -srcfolder "$STAGING_DIR" \
  -ov \
  -format UDZO \
  "$DMG_PATH"

rm -rf "$STAGING_DIR"
echo "==> Done: $DMG_PATH"