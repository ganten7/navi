#!/usr/bin/env bash
# Build Navi.app for macOS and package it as Navi-<version>-<arch>.zip.
#
# Usage:
#   scripts/build-macos.sh                # release build, host arch
#   ARCH=universal scripts/build-macos.sh # arm64 + x86_64 universal binary
#
# Output: dist/Navi.app  and  dist/Navi-<version>-<arch>.zip
#
# Requirements: Rust toolchain (rustup), Xcode CLT for codesign / ditto.
# For ARCH=universal: rustup target add x86_64-apple-darwin aarch64-apple-darwin
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

ARCH="${ARCH:-host}"
VERSION="$(grep '^version' navi/Cargo.toml | head -1 | sed -E 's/.*"([^"]+)".*/\1/')"
APP="dist/Navi.app"
ZIP="dist/Navi-${VERSION}-${ARCH}.zip"

echo "==> Navi $VERSION  arch=$ARCH"

# ── Build ────────────────────────────────────────────────────────────────────
case "$ARCH" in
  host)
    cargo build --release -p navi
    BIN="target/release/navi"
    ;;
  universal)
    rustup target add x86_64-apple-darwin aarch64-apple-darwin >/dev/null 2>&1 || true
    cargo build --release -p navi --target x86_64-apple-darwin
    cargo build --release -p navi --target aarch64-apple-darwin
    mkdir -p target/universal/release
    BIN="target/universal/release/navi"
    lipo -create -output "$BIN" \
      target/x86_64-apple-darwin/release/navi \
      target/aarch64-apple-darwin/release/navi
    ;;
  *)
    echo "unknown ARCH=$ARCH (expected host|universal)" >&2
    exit 2
    ;;
esac

# ── Bundle ───────────────────────────────────────────────────────────────────
echo "==> Assembling $APP"
rm -rf "$APP" "$ZIP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$BIN" "$APP/Contents/MacOS/navi"
chmod +x "$APP/Contents/MacOS/navi"
strip -x "$APP/Contents/MacOS/navi" 2>/dev/null || true

if [ -f assets/icon.icns ]; then
  cp assets/icon.icns "$APP/Contents/Resources/AppIcon.icns"
fi

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>             <string>Navi</string>
    <key>CFBundleDisplayName</key>      <string>Navi</string>
    <key>CFBundleExecutable</key>       <string>navi</string>
    <key>CFBundleIconFile</key>         <string>AppIcon</string>
    <key>CFBundleIdentifier</key>       <string>com.navi.graph</string>
    <key>CFBundleInfoDictionaryVersion</key> <string>6.0</string>
    <key>CFBundlePackageType</key>      <string>APPL</string>
    <key>CFBundleShortVersionString</key> <string>${VERSION}</string>
    <key>CFBundleVersion</key>          <string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key>   <string>11.0</string>
    <key>NSHighResolutionCapable</key>  <true/>
    <key>LSApplicationCategoryType</key> <string>public.app-category.utilities</string>
    <key>NSPrincipalClass</key>         <string>NSApplication</string>
</dict>
</plist>
PLIST

# Ad-hoc codesign so macOS lets the user open it (still flagged by Gatekeeper
# on first launch — instruct users to right-click → Open or `xattr -cr Navi.app`).
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || \
  echo "warn: codesign failed (continuing, app will trip Gatekeeper)"

# ── Package ──────────────────────────────────────────────────────────────────
echo "==> Packaging $ZIP"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$ZIP"

du -h "$BIN" "$APP/Contents/MacOS/navi" "$ZIP" | sed 's/^/    /'
echo "==> Done. Open with:  open $APP"
echo "==> Upload to release:  gh release upload v${VERSION} $ZIP --clobber"
