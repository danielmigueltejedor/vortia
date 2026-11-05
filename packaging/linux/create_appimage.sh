#!/usr/bin/env bash
# Build a Vortia AppImage from an already-compiled binary.
# Usage:
#   create_appimage.sh <vortia-executable> <output-AppImage> [version]
#
# Expects Qt/VTK shared libraries discoverable via ldd (e.g. conda env on PATH / LD_LIBRARY_PATH).
set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "AppImage packaging requires Linux" >&2
  exit 1
fi
if [[ "$#" -lt 2 || "$#" -gt 3 ]]; then
  echo "Usage: create_appimage.sh <vortia-executable> <output.AppImage> [version]" >&2
  exit 2
fi

exe="$(readlink -f "$1")"
out="$2"
version="${3:-0.1.0}"
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
icon="$repo/resources/icons/vortia.png"
desktop="$here/Vortia.desktop"

[[ -x "$exe" ]] || { echo "Missing executable: $exe" >&2; exit 1; }
[[ -f "$icon" ]] || { echo "Missing icon: $icon" >&2; exit 1; }
[[ -f "$desktop" ]] || { echo "Missing desktop file: $desktop" >&2; exit 1; }

mkdir -p "$(dirname "$out")"
out="$(cd "$(dirname "$out")" && pwd)/$(basename "$out")"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
appdir="$work/AppDir"
tools="$work/tools"
mkdir -p "$tools" "$appdir/usr/bin" "$appdir/usr/share/applications" \
  "$appdir/usr/share/icons/hicolor/256x256/apps" "$appdir/usr/share/vortia"

cp "$exe" "$appdir/usr/bin/vortia"
chmod +x "$appdir/usr/bin/vortia"

# Ship Spanish catalog next to resources when present.
for qm in \
  "$(dirname "$exe")/../vortia_es.qm" \
  "$(dirname "$exe")/vortia_es.qm" \
  "${CMAKE_BINARY_DIR:-}/vortia_es.qm"; do
  if [[ -f "$qm" ]]; then
    cp "$qm" "$appdir/usr/share/vortia/vortia_es.qm"
    break
  fi
done

cp "$desktop" "$appdir/usr/share/applications/Vortia.desktop"
cp "$icon" "$appdir/usr/share/icons/hicolor/256x256/apps/vortia.png"
cp "$icon" "$appdir/vortia.png"
cp "$desktop" "$appdir/Vortia.desktop"

# linuxdeploy (+ Qt plugin). Extract-and-run: GitHub runners often lack FUSE.
base_url="https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous"
plugin_url="https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/continuous"
curl -fsSL "$base_url/linuxdeploy-x86_64.AppImage" -o "$tools/linuxdeploy.AppImage"
curl -fsSL "$plugin_url/linuxdeploy-plugin-qt-x86_64.AppImage" -o "$tools/linuxdeploy-plugin-qt.AppImage"
chmod +x "$tools"/linuxdeploy*.AppImage

if [[ -n "${CONDA_PREFIX:-}" ]]; then
  export PATH="$CONDA_PREFIX/bin:$PATH"
  export LD_LIBRARY_PATH="$CONDA_PREFIX/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  export QMAKE="${QMAKE:-$CONDA_PREFIX/bin/qmake}"
fi
export QMAKE="${QMAKE:-$(command -v qmake6 || command -v qmake || true)}"
export ARCH=x86_64
export VERSION="$version"
export LINUXDEPLOY_OUTPUT_VERSION="$version"
export DISABLE_COPYRIGHT_FILES_DEPLOYMENT=1

(
  cd "$work"
  "$tools/linuxdeploy.AppImage" --appimage-extract-and-run \
    --appdir "$appdir" \
    --executable "$appdir/usr/bin/vortia" \
    --desktop-file "$appdir/Vortia.desktop" \
    --icon-file "$appdir/vortia.png" \
    --plugin qt \
    --output appimage
)

shopt -s nullglob
built=( "$work"/Vortia*.AppImage "$work"/*.AppImage )
[[ "${#built[@]}" -ge 1 ]] || { echo "linuxdeploy did not produce an AppImage" >&2; exit 1; }

cp "${built[0]}" "$out"
chmod +x "$out"
echo "Created $out"
