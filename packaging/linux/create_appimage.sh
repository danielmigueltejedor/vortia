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

base_url="https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous"
plugin_url="https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/continuous"
# Refuse HTTP redirects (Sonar shell:S6506).
curl_https=(curl -fsSL --proto '=https' --tlsv1.2)
"${curl_https[@]}" "$base_url/linuxdeploy-x86_64.AppImage" -o "$tools/linuxdeploy.AppImage"
"${curl_https[@]}" "$plugin_url/linuxdeploy-plugin-qt-x86_64.AppImage" -o "$tools/linuxdeploy-plugin-qt.AppImage"
chmod +x "$tools"/linuxdeploy*.AppImage

if [[ -n "${CONDA_PREFIX:-}" ]]; then
  export PATH="$CONDA_PREFIX/bin:$CONDA_PREFIX/lib/qt6/bin:$PATH"
  export LD_LIBRARY_PATH="$CONDA_PREFIX/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi

resolve_qmake() {
  local candidates=(
    "${QMAKE:-}"
    "${CONDA_PREFIX:-}/lib/qt6/bin/qmake"
    "${CONDA_PREFIX:-}/bin/qmake"
    "${CONDA_PREFIX:-}/bin/qmake6"
  )
  local found
  found="$(command -v qmake6 2>/dev/null || true)"
  candidates+=("$found")
  found="$(command -v qmake 2>/dev/null || true)"
  candidates+=("$found")
  local c
  for c in "${candidates[@]}"; do
    if [[ -n "$c" && -x "$c" ]]; then
      printf '%s\n' "$c"
      return 0
    fi
  done
  return 1
}

if ! QMAKE="$(resolve_qmake)"; then
  echo "qmake not found (CONDA_PREFIX=${CONDA_PREFIX:-<unset>}, PATH=$PATH)" >&2
  ls -la "${CONDA_PREFIX:-/nonexistent}/bin"/qmake* 2>/dev/null || true
  ls -la "${CONDA_PREFIX:-/nonexistent}/lib/qt6/bin"/qmake* 2>/dev/null || true
  exit 1
fi
export QMAKE
echo "Using qmake: $QMAKE"

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
