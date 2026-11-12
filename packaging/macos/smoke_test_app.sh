#!/bin/bash
# Start a deployed Vortia.app on this Mac and fail with the real cause.
# Usage: smoke_test_app.sh <Vortia.app>
set -uo pipefail

app="$(cd "$1" && pwd -P)"
exe="$app/Contents/MacOS/vortia"
[[ -x "$exe" ]] || { echo "Missing $exe" >&2; exit 2; }
fail=0
bad() { echo "FAIL: $*"; fail=1; }

echo "== Host =="
sw_vers | tr '\n' ' '; echo; uname -m

echo "== Bundle =="
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist"
minimum="$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$app/Contents/Info.plist")"
echo "LSMinimumSystemVersion: $minimum"
codesign --verify --deep --strict "$app" || bad "code signature does not verify"
identity="$(codesign -dv "$app" 2>&1)"
grep -E '^(Identifier|Signature|TeamIdentifier)' <<<"$identity"
grep -q '^Identifier=dev.vortia.app$' <<<"$identity" \
  || bad "bundle identifier is not dev.vortia.app (in-app updates would be rejected)"

echo "== Linked libraries stay inside the bundle =="
outside="$(find "$app/Contents/MacOS" "$app/Contents/Frameworks" "$app/Contents/PlugIns" -type f \( -name '*.dylib' -o -perm +111 \) -print0 |
  xargs -0 -n1 otool -L 2>/dev/null | awk '/^\t/{print $1}' |
  grep -vE '^(/usr/lib/|/System/|@rpath/)' | sort -u)"
if [[ -n "$outside" ]]; then echo "$outside" | head -n 20; bad "references outside the bundle"; else echo "ok"; fi
# @rpath references only resolve inside the bundle if every search path is relative to it.
stray_rpaths="$(find "$app/Contents/MacOS" "$app/Contents/Frameworks" "$app/Contents/PlugIns" -type f \( -name '*.dylib' -o -perm +111 \) -print0 |
  xargs -0 -n1 otool -l 2>/dev/null | awk '/cmd LC_RPATH/{r=1} r && $1=="path"{print $2; r=0}' |
  grep -vE '^@(executable|loader)_path' | sort -u)"
if [[ -n "$stray_rpaths" ]]; then echo "$stray_rpaths" | head -n 20; bad "search paths outside the bundle"; fi

echo "== Headless start =="
"$exe" --verify-spanish || bad "--verify-spanish rc=$?"

echo "== Main window and OpenGL viewport =="
log="$(mktemp)"
DYLD_PRINT_LIBRARIES=1 "$exe" --verify-startup >"$log" 2>&1
rc=$?
grep -v '^dyld\[' "$log" | head -n 40 | sed 's/^/  | /'
[[ "$rc" -eq 0 ]] || bad "--verify-startup rc=$rc"
foreign="$(sed -nE 's/^dyld\[[0-9]+\]: <[^>]*> (\/.*)$/\1/p' "$log" |
  awk -v app="$app/" 'index($0, app) != 1 && $0 !~ "^(/usr/lib/|/System/|/Library/Apple/)"' | sort -u)"
if [[ -n "$foreign" ]]; then echo "$foreign" | head -n 20; bad "libraries loaded from outside the bundle"; fi
rm -f "$log"

if [[ -d "$app/Contents/Resources/gmsh" ]]; then
  echo "== Packaged mesher (Python + Gmsh) =="
  env -i HOME="$HOME" PATH=/usr/bin:/bin "$exe" --verify-mesher || bad "--verify-mesher rc=$?"
elif [[ -n "${VORTIA_EXPECT_ENGINE:-}" ]]; then
  bad "this app has no packaged mesher"
fi

if compgen -G "$app/Contents/Resources/OpenFOAM-*/*.dmg" >/dev/null; then
  echo "== Integrated OpenFOAM engine: real serial and 2-process MPI run =="
  engine_log="$(mktemp)"
  "$exe" --verify-engine >"$engine_log" 2>&1
  rc=$?
  tail -n 30 "$engine_log" | cut -c1-300 | sed 's/^/  | /'
  if [[ "$rc" -eq 0 ]] && grep -q 'verified with a real serial and parallel run' "$engine_log"; then
    echo "OK: integrated engine ran a case serially and in parallel"
  else
    bad "--verify-engine rc=$rc"
  fi
  rm -f "$engine_log"
elif [[ -n "${VORTIA_EXPECT_ENGINE:-}" ]]; then
  bad "this app has no integrated OpenFOAM engine"
fi

echo
[[ "$fail" -eq 0 ]] && echo "SMOKE TEST PASSED" || echo "SMOKE TEST FAILED"
exit "$fail"
