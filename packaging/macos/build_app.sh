#!/usr/bin/env bash
# Build "DeltaF Studio.app" and a DMG for macOS.
#   packaging/macos/build_app.sh [OUT_DIR]      (default: dist/)
# Builds a universal binary (Apple silicon + Intel) when both Rust targets
# are installed (rustup target add aarch64-apple-darwin x86_64-apple-darwin),
# otherwise for the build machine only. The bundle holds dfstudio and the
# dfs command-line runner (Contents/MacOS), the CoolProp library
# (Contents/Frameworks) and the example projects (Contents/Resources/examples).
# The app is signed ad hoc (no Apple Developer ID): on first launch macOS asks
# to confirm it (right-click > Open, or System Settings > Privacy & Security).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
out=$(mkdir -p "${1:-$root/dist}" && cd "${1:-$root/dist}" && pwd)
version=$(grep -m1 '^version' "$root/Cargo.toml" | cut -d'"' -f2)
cd "$root"

# Drop the link-time reference to libCoolProp (it is loaded at run time from
# the bundle) and let the loader look in Contents/Frameworks if it is kept.
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,-dead_strip_dylibs -C link-arg=-Wl,-rpath,@executable_path/../Frameworks"
export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-11.0}

targets=()
for t in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup target list --installed 2>/dev/null | grep -qx "$t" && targets+=("$t")
done
[ ${#targets[@]} -gt 0 ] || targets=("$(rustc -vV | sed -n 's/^host: //p')")

bins=() libs=()
for t in "${targets[@]}"; do
    cargo build --release --locked -p dfapp -p dfcli --target "$t"
    bins+=("target/$t/release")
    lib=$(find "target/$t/release/build" -path '*coolprop-sys-macos*/out/libCoolProp.dylib' | head -n1)
    [ -n "$lib" ] || { echo "libCoolProp.dylib not found for $t" >&2; exit 1; }
    libs+=("$lib")
done
suffix=$([ ${#targets[@]} -gt 1 ] && echo universal || echo "${targets[0]%%-*}")

app="$out/DeltaF Studio.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Frameworks" "$app/Contents/Resources/examples"
for b in dfstudio dfs; do
    lipo -create $(for d in "${bins[@]}"; do printf '%s/%s ' "$d" "$b"; done) -output "$app/Contents/MacOS/$b"
done
lipo -create "${libs[@]}" -output "$app/Contents/Frameworks/libCoolProp.dylib"
cp "$root/packaging/icons/dfstudio.icns" "$app/Contents/Resources/"
"$app/Contents/MacOS/dfstudio" --write-examples "$app/Contents/Resources/examples"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>DeltaF Studio</string>
  <key>CFBundleDisplayName</key><string>DeltaF Studio</string>
  <key>CFBundleIdentifier</key><string>io.github.dafer238.dfstudio</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleExecutable</key><string>dfstudio</string>
  <key>CFBundleIconFile</key><string>dfstudio</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>$MACOSX_DEPLOYMENT_TARGET</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>DeltaF Studio project</string>
      <key>CFBundleTypeExtensions</key><array><string>df</string></array>
      <key>CFBundleTypeRole</key><string>Editor</string>
      <key>CFBundleTypeIconFile</key><string>dfstudio</string>
    </dict>
    <dict>
      <key>CFBundleTypeName</key><string>DeltaF Studio script</string>
      <key>CFBundleTypeExtensions</key><array><string>dfs</string></array>
      <key>CFBundleTypeRole</key><string>Editor</string>
    </dict>
  </array>
</dict>
</plist>
PLIST

echo "linked libraries of dfstudio:"; otool -L "$app/Contents/MacOS/dfstudio"
codesign --force --deep --sign - "$app"

dmg="$out/DeltaF_Studio-$version-macos-$suffix.dmg"
stage="$out/dmg"
rm -rf "$stage" "$dmg"
mkdir -p "$stage"
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
hdiutil create -volname "DeltaF Studio $version" -srcfolder "$stage" -ov -format UDZO "$dmg"
rm -rf "$stage"
echo "wrote $dmg"
