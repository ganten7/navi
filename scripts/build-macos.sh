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

# Icon — build a fully canonical .icns from the highest-resolution variant
# we can find, scaling with `sips` to every size Finder/Dock expects. We
# write the icns binary ourselves rather than going through iconutil because
# `iconutil` on recent macOS has been reported to reject perfectly valid
# iconsets with the "Invalid Iconset" error when files have any extended
# attributes the tool didn't write itself.
if [ -f assets/icon.icns ] || [ -f assets/icon.png ]; then
  TMP_DIR="$(mktemp -d)"
  PNG_DIR="$TMP_DIR/png"
  mkdir -p "$PNG_DIR"

  if [ -f assets/icon.png ]; then
    SOURCE="assets/icon.png"
  else
    iconutil -c iconset -o "$TMP_DIR/extracted.iconset" assets/icon.icns >/dev/null 2>&1
    if [ -f "$TMP_DIR/extracted.iconset/icon_512x512@2x.png" ]; then
      SOURCE="$TMP_DIR/extracted.iconset/icon_512x512@2x.png"
    elif [ -f "$TMP_DIR/extracted.iconset/icon_512x512.png" ]; then
      SOURCE="$TMP_DIR/extracted.iconset/icon_512x512.png"
    else
      SOURCE=""
    fi
  fi

  if [ -n "$SOURCE" ]; then
    # Render every variant size as a clean PNG via sips.
    for spec in 16:16.png 32:32.png 64:64.png 128:128.png 256:256.png \
                512:512.png 1024:1024.png; do
      px="${spec%%:*}"; name="${spec##*:}"
      sips -z "$px" "$px" -s format png --out "$PNG_DIR/$name" "$SOURCE" >/dev/null
    done

    # Pack into a canonical icns. The chunk-type → pixel-size mapping is
    # the standard one Apple's docs document (icp4 = 16, icp5 = 32, etc.).
    /usr/bin/python3 - "$PNG_DIR" "$APP/Contents/Resources/AppIcon.icns" <<'PY'
import os, struct, sys
src, out = sys.argv[1], sys.argv[2]
chunks = [
    ('icp4', '16.png'),    # 16x16
    ('icp5', '32.png'),    # 32x32
    ('icp6', '64.png'),    # 64x64
    ('ic07', '128.png'),   # 128x128
    ('ic08', '256.png'),   # 256x256
    ('ic09', '512.png'),   # 512x512
    ('ic10', '1024.png'),  # 512x512@2x
    ('ic11', '32.png'),    # 16x16@2x
    ('ic12', '64.png'),    # 32x32@2x
    ('ic13', '256.png'),   # 128x128@2x
    ('ic14', '512.png'),   # 256x256@2x
]
body = b''
for code, fname in chunks:
    data = open(os.path.join(src, fname), 'rb').read()
    body += code.encode('ascii') + struct.pack('>I', len(data) + 8) + data
total = len(body) + 8
header = b'icns' + struct.pack('>I', total)
open(out, 'wb').write(header + body)
print(f"  icns: {len(chunks)} chunks, {total} bytes -> {out}")
PY
  else
    cp assets/icon.icns "$APP/Contents/Resources/AppIcon.icns"
  fi
  rm -rf "$TMP_DIR"
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
    <key>CFBundleIconName</key>         <string>AppIcon</string>
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

# Bump bundle mtime so Icon Services notices a fresh app and reloads the icon.
touch "$APP" "$APP/Contents/Info.plist"

# ── Package ──────────────────────────────────────────────────────────────────
echo "==> Packaging $ZIP"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$ZIP"

du -h "$BIN" "$APP/Contents/MacOS/navi" "$ZIP" | sed 's/^/    /'
echo "==> Done. Open with:  open $APP"
echo "==> Upload to release:  gh release upload v${VERSION} $ZIP --clobber"
