#!/bin/bash
# Build a branded Vortia installer DMG from an existing Vortia.app.
# Usage: create_dmg.sh <Vortia.app> <output.dmg> [version]
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "DMG packaging requires macOS" >&2
  exit 1
fi
if [[ "$#" -lt 2 || "$#" -gt 3 ]]; then
  echo "Usage: create_dmg.sh <Vortia.app> <output.dmg> [version]" >&2
  exit 2
fi

app="$1"
out="$2"
here="$(cd "$(dirname "$0")" && pwd)"
version="${3:-$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$1/Contents/Info.plist")}"
bg="$here/dmg-background.png"

[[ -d "$app" && -f "$app/Contents/MacOS/vortia" ]] || {
  echo "Missing Vortia.app" >&2
  exit 1
}
[[ -f "$bg" ]] || {
  echo "Missing branded background: $bg" >&2
  exit 1
}

stage="$(mktemp -d)/stage"
rw="$(mktemp -d)/vortia-rw.dmg"
volname="Vortia ${version}"
mkdir -p "$stage/.background"
trap 'rm -rf "$(dirname "$stage")" "$(dirname "$rw")" >/dev/null 2>&1 || true' EXIT

ditto "$app" "$stage/Vortia.app"
ln -s /Applications "$stage/Applications"
cp "$bg" "$stage/.background/background.png"

# Room for the app plus file-system overhead, whatever the bundle weighs.
stage_mb="$(du -sm "$stage" | cut -f1)"
hdiutil create -srcfolder "$stage" -volname "$volname" -fs HFS+ \
  -fsargs "-c c=64,a=16,e=16" -format UDRW -size "$((stage_mb + stage_mb / 5 + 40))m" "$rw" >/dev/null

hdiutil attach -readwrite -noverify -noautoopen "$rw" >/dev/null
mount="/Volumes/${volname}"
for _ in $(seq 1 50); do [[ -d "$mount" ]] && break; sleep 0.1; done
[[ -d "$mount" ]] || { echo "Failed to mount $rw" >&2; exit 1; }

# The window layout is cosmetic. Finder scripting can be unavailable (CI
# runners, missing Automation permission): the DMG is still valid without it.
layout_window() {
osascript <<EOF
with timeout of 60 seconds
tell application "Finder"
  tell disk "$volname"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {200, 120, 860, 520}
    set theViewOptions to the icon view options of container window
    set arrangement of theViewOptions to not arranged
    set icon size of theViewOptions to 128
    set background picture of theViewOptions to file ".background:background.png"
    set position of item "Vortia.app" of container window to {160, 200}
    set position of item "Applications" of container window to {500, 200}
    update without registering applications
    delay 2
    close
  end tell
end tell
end timeout
EOF
}
layout_window || echo "warning: could not arrange the DMG window; continuing without the layout" >&2

sync
# Finder or Spotlight can keep the volume busy for a moment after the layout
# step; a failed eject must not fail the release.
detached=0
for attempt in 1 2 3 4 5 6 7 8; do
  if hdiutil detach "$mount" >/dev/null 2>&1; then detached=1; break; fi
  sleep 3
done
if ((!detached)); then
  hdiutil detach -force "$mount" >/dev/null
fi
rm -f "$out"
hdiutil convert "$rw" -format UDZO -imagekey zlib-level=9 -o "$out" >/dev/null
echo "Created $out"
