#!/bin/bash
# Build Vortia Native.app from a deployed Vortia.app (the Qt edition, after deploy_bundle.py).
#
# The two editions share everything below the interface: vortia-engine, the Qt and VTK
# libraries it links, the packaged OpenFOAM and the mesher. So the Native app starts as a copy
# of that bundle, drops the Qt interface and takes the SwiftUI one in its place.
#
# Usage: platforms/apple/build.sh <deployed Vortia.app> <output directory>
set -euo pipefail

[[ "$(uname -s)" == "Darwin" ]] || { echo "Vortia Native builds on macOS" >&2; exit 1; }
[[ "$#" -eq 2 ]] || { echo "Usage: build.sh <Vortia.app> <output directory>" >&2; exit 2; }

source_app="$1"
out="$2"
here="$(cd "$(dirname "$0")" && pwd)"
[[ -x "$source_app/Contents/MacOS/vortia-engine" ]] || {
  echo "$source_app has no vortia-engine: copy it into Contents/MacOS before deploy_bundle.py" >&2
  exit 1
}
version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$source_app/Contents/Info.plist")"

swift build -c release --package-path "$here" --product VortiaMac
binary="$(swift build -c release --package-path "$here" --show-bin-path)/VortiaMac"

app="$out/Vortia Native.app"
rm -rf "$app"
mkdir -p "$out"
ditto "$source_app" "$app"
rm -f "$app/Contents/MacOS/vortia"
install -m755 "$binary" "$app/Contents/MacOS/VortiaNative"

plist="$app/Contents/Info.plist"
cat > "$plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Vortia Native</string>
    <key>CFBundleDisplayName</key><string>Vortia Native</string>
    <key>CFBundleIdentifier</key><string>dev.vortia.app.native</string>
    <key>CFBundleExecutable</key><string>VortiaNative</string>
    <key>CFBundleIconFile</key><string>Vortia</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${version}</string>
    <key>CFBundleVersion</key><string>${version}</string>
    <key>CFBundleDevelopmentRegion</key><string>es</string>
    <key>CFBundleLocalizations</key><array><string>es</string><string>en</string></array>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST

# The executables changed, so the seal is redone over the whole bundle.
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"
echo "$app"
