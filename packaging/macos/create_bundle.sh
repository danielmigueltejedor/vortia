#!/bin/bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Vortia.app packaging requires macOS" >&2
  exit 1
fi
if [[ "$#" -ne 4 ]]; then
  echo "Usage: create_bundle.sh <vortia-executable> <icon.png> <Vortia.app> <Info.plist>" >&2
  exit 2
fi

exe="$1"
png="$2"
bundle="$3"
plist="$4"
[[ -f "$exe" && -f "$png" && -f "$plist" ]] || {
  echo "Missing Vortia executable, icon or Info.plist" >&2
  exit 1
}

mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$exe" "$bundle/Contents/MacOS/vortia"
chmod +x "$bundle/Contents/MacOS/vortia"
cp "$plist" "$bundle/Contents/Info.plist"
# Install generated translations for the Finder-launched bundle as well.
generated_qm="$(dirname "$(dirname "$exe")")/vortia_es.qm"
if [[ -f "$generated_qm" ]]; then
    cp "$generated_qm" "$bundle/Contents/Resources/vortia_es.qm"
fi

temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT
iconset="$temporary/Vortia.iconset"
mkdir -p "$iconset"
native_png="$temporary/Vortia-macos.png"
/usr/bin/swift "$(dirname "$0")/prepare_icon.swift" "$png" "$native_png"
# iconutil expects pairs of normal/Retina PNG images.
for size in 16 32 128 256 512; do
    retina=$((size * 2))
    sips -s format png -z "$size" "$size" "$native_png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -s format png -z "$retina" "$retina" "$native_png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$bundle/Contents/Resources/Vortia.icns"
if [[ -n "${VORTIA_BUNDLE_OPENFOAM_IMAGE:-}" ]]; then
    python3 "$(dirname "$0")/bundle_openfoam.py" "$VORTIA_BUNDLE_OPENFOAM_IMAGE" "$bundle"
fi
echo "Created $bundle (local build; distribution requires dependency packaging/signing)"
