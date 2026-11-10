#!/usr/bin/env bash
# Builds the macOS (Apple silicon) release files on a Mac. The release
# workflow runs it on a GitHub macOS runner with Rust 1.99.1.
#
#   scripts/macos.sh    # writes dist/
#
# The macOS files are not reproducible: they have a provenance attestation
# only. The program has an ad-hoc signature, which Apple silicon requires.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
export SOURCE_DATE_EPOCH=$(git log -1 --format=%ct)
export RUSTFLAGS="--remap-path-prefix=$ROOT=. --remap-path-prefix=$HOME=~"
T=aarch64-apple-darwin
N=builddeed-checksum
V=$(sed -n 's/^version = "\(.*\)"/\1/p' app/Cargo.toml | head -1)

cargo build --locked --release --target "$T" --manifest-path app/Cargo.toml

W=$(mktemp -d)
APP="$W/BuildDeed Checksum.app"
mkdir -p "$APP/Contents/MacOS" dist
cp "app/target/$T/release/$N" "$APP/Contents/MacOS/$N"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>BuildDeed Checksum</string>
  <key>CFBundleDisplayName</key><string>BuildDeed Checksum</string>
  <key>CFBundleIdentifier</key><string>com.hardenedminds.builddeed-checksum</string>
  <key>CFBundleVersion</key><string>$V</string>
  <key>CFBundleShortVersionString</key><string>$V</string>
  <key>CFBundleExecutable</key><string>$N</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
codesign --force --deep --sign - "$APP"

# The bare program, with the same bytes as the one in the app bundle, so
# that the app can find its own record in the contract.
cp "$APP/Contents/MacOS/$N" "dist/$N-$V-macos-aarch64"
ditto -c -k --keepParent "$APP" "dist/$N-$V-macos-aarch64.zip"
hdiutil create -volname "BuildDeed Checksum" -srcfolder "$APP" -ov -format UDZO "dist/$N-$V-macos-aarch64.dmg"
rm -rf "$W"
(cd dist && shasum -a 256 "$N-$V-macos-aarch64"*)
