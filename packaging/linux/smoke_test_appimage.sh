#!/usr/bin/env bash
# Start a Vortia AppImage the way an end user would, on the current machine,
# and fail loudly with the real cause when it does not come up.
# Usage: smoke_test_appimage.sh <Vortia.AppImage>
#
# Containers have no FUSE, so the image is unpacked with --appimage-extract;
# everything after the mount (AppRun, bundled libraries, Qt platform plugin,
# OpenGL context, main window) is exercised exactly as shipped.
set -uo pipefail

image="$(readlink -f "$1")"
[[ -s "$image" ]] || { echo "Missing AppImage: $1" >&2; exit 2; }
chmod +x "$image"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
# A directory with a space and a non-ASCII character, like ~/Descargas/Mis apps.
run_dir="$work/Mis aplicaciónes"
mkdir -p "$run_dir"
cd "$run_dir"

fail=0
note() { printf '\n== %s ==\n' "$*"; }

note "Host"
( . /etc/os-release && echo "$PRETTY_NAME" )
ldd --version 2>&1 | head -n1
host_stdcxx="$(/sbin/ldconfig -p 2>/dev/null | awk '/libstdc\+\+\.so\.6 /{print $NF; exit}')"
if [[ -n "$host_stdcxx" ]]; then
  echo "host libstdc++: $host_stdcxx ($(strings "$host_stdcxx" | grep -E '^GLIBCXX_[0-9.]+$' | sort -V | tail -n1))"
else
  echo "host libstdc++: none"
fi

note "AppImage runtime"
file -b "$image" | cut -c1-200

note "Extract"
"$image" --appimage-extract >/dev/null || { echo "FAIL: --appimage-extract"; exit 1; }
app="$run_dir/squashfs-root"
ls -l "$app/AppRun"

note "Unresolved libraries (as the launcher would resolve them)"
missing="$(
  export LD_LIBRARY_PATH="$app/usr/lib:$app/usr/lib/x86_64-linux-gnu"
  find "$app/usr/bin/vortia" "$app/usr/plugins" "$app/usr/lib" -type f \
    \( -name 'vortia' -o -name '*.so' -o -name '*.so.*' \) -print0 |
    xargs -0 -n1 sh -c 'ldd "$0" 2>&1 | grep -E "not found" | sed "s|^|$(basename "$0"):|"' |
    sort -u
)"
if [[ -n "$missing" ]]; then
  echo "$missing"
  # Only the executable and the xcb platform plugin are hard requirements;
  # optional plugins with absent host libraries are reported, not fatal.
  if grep -qE '^(vortia|libqxcb\.so):' <<<"$missing"; then
    echo "FAIL: executable or xcb platform plugin has unresolved libraries"
    fail=1
  fi
else
  echo "none"
fi

clean_env=(env -u LD_LIBRARY_PATH -u QT_PLUGIN_PATH -u QT_QPA_PLATFORM_PLUGIN_PATH -u QT_QPA_PLATFORM)

note "Headless start (--verify-spanish)"
"${clean_env[@]}" "$app/AppRun" --verify-spanish || { echo "FAIL: rc=$?"; fail=1; }

note "Full application start on a clean X display"
log="$work/start.log"
"${clean_env[@]}" timeout --signal=TERM --kill-after=10 25 \
  xvfb-run -a -s "-screen 0 1280x800x24" "$app/AppRun" >"$log" 2>&1
rc=$?
sed 's/^/  | /' "$log" | head -n 60
# The GUI has no reason to exit by itself: still running at the timeout
# (124) means the main window came up and the event loop was alive.
if [[ "$rc" -eq 124 ]]; then
  echo "OK: application was still running after 25 s"
else
  echo "FAIL: application exited by itself, rc=$rc"
  fail=1
fi
if grep -qiE 'could not (load|find) the qt platform plugin|GLIBCXX_[0-9.]+.? not found|cannot open shared object' "$log"; then
  echo "FAIL: startup log reports a missing platform plugin or library"
  fail=1
fi

exit "$fail"
