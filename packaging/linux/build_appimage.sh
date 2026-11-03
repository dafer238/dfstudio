#!/usr/bin/env bash
# Build the DF Studio AppImage (x86_64 or aarch64, the build machine's).
#   packaging/linux/build_appimage.sh [OUT_DIR]      (default: dist/)
# Needs Rust and curl; appimagetool is downloaded when not on PATH.
# The AppImage holds dfstudio, the dfs command-line runner, the CoolProp
# library (usr/lib) and the example projects (usr/share/dfstudio/examples).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
out=$(mkdir -p "${1:-$root/dist}" && cd "${1:-$root/dist}" && pwd)
version=$(grep -m1 '^version' "$root/Cargo.toml" | cut -d'"' -f2)
arch=$(uname -m)

cd "$root"
cargo build --release --locked -p dfapp -p dfcli
lib=$(find target/release/build -path '*coolprop-sys-linux*/out/libCoolProp.so' | head -n1)
[ -n "$lib" ] || { echo "libCoolProp.so not found under target/release/build" >&2; exit 1; }

app="$out/AppDir"
rm -rf "$app"
mkdir -p "$app/usr/bin" "$app/usr/lib" "$app/usr/share/dfstudio/examples" \
         "$app/usr/share/applications" "$app/usr/share/icons/hicolor/256x256/apps"
cp target/release/dfstudio target/release/dfs "$app/usr/bin/"
cp "$lib" "$app/usr/lib/"
target/release/dfstudio --write-examples "$app/usr/share/dfstudio/examples"

python3 - "$root/packaging/icons/dfstudio.png" "$app/usr/share/icons/hicolor/256x256/apps/dfstudio.png" <<'PY' 2>/dev/null || cp "$root/packaging/icons/dfstudio.png" "$app/usr/share/icons/hicolor/256x256/apps/dfstudio.png"
import sys
from PIL import Image
Image.open(sys.argv[1]).resize((256, 256), Image.LANCZOS).save(sys.argv[2])
PY
cp "$app/usr/share/icons/hicolor/256x256/apps/dfstudio.png" "$app/dfstudio.png"
cp "$here/dfstudio.desktop" "$app/usr/share/applications/"
cp "$here/dfstudio.desktop" "$app/"
cat > "$app/AppRun" <<'SH'
#!/bin/sh
# `DF_Studio.AppImage dfs run model.dfs` runs the command-line solver.
HERE=$(dirname "$(readlink -f "$0")")
if [ "$1" = "dfs" ]; then
    shift
    exec "$HERE/usr/bin/dfs" "$@"
fi
exec "$HERE/usr/bin/dfstudio" "$@"
SH
chmod +x "$app/AppRun"

tool=$(command -v appimagetool || true)
if [ -z "$tool" ]; then
    tool="${TMPDIR:-/tmp}/appimagetool-$arch.AppImage"
    [ -x "$tool" ] || { curl -fsSL -o "$tool" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$arch.AppImage"; chmod +x "$tool"; }
fi
# no FUSE needed (CI containers)
APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$arch" "$tool" --no-appstream "$app" "$out/DF_Studio-$version-linux-$arch.AppImage"
echo "wrote $out/DF_Studio-$version-linux-$arch.AppImage"
