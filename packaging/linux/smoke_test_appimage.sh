#!/usr/bin/env bash
# Start a Vortia AppImage the way an end user would, on the current machine,
# and fail loudly with the real cause when it does not come up.
# Usage: smoke_test_appimage.sh <Vortia.AppImage> [--fuse]
#
# Default: the image is unpacked with --appimage-extract (containers have no
# FUSE); everything after the mount (AppRun, bundled libraries, Qt platform
# plugin, OpenGL context, main window) is exercised exactly as shipped.
# --fuse: additionally run the image file itself, mounted by its runtime, as
# a double click does. Needs a machine with FUSE (not a container).
set -uo pipefail

image="$(readlink -f "$1")"
mode="${2:-}"
[[ -s "$image" ]] || { echo "Missing AppImage: $1" >&2; exit 2; }
chmod +x "$image"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
# A directory with a space and a non-ASCII character, like ~/Descargas/Mis apps.
run_dir="$work/Mis aplicaciónes"
mkdir -p "$run_dir"
cd "$run_dir" || exit 2

fail=0
note() { printf '\n== %s ==\n' "$*"; }
bad() { echo "FAIL: $*"; fail=1; }

# No conda/Qt/OpenFOAM overrides from the calling environment, a private
# HOME, and stderr straight to us instead of the desktop-launch log.
clean_env=(env -u LD_LIBRARY_PATH -u QT_PLUGIN_PATH -u QT_QPA_PLATFORM_PLUGIN_PATH
           -u QT_QPA_PLATFORM -u FONTCONFIG_FILE -u FONTCONFIG_PATH
           "HOME=$work/home" VORTIA_NO_STARTUP_DIALOG=1)
mkdir -p "$work/home"
xvfb=(xvfb-run -a -s "-screen 0 1280x800x24")

note "Host"
( . /etc/os-release && echo "$PRETTY_NAME" )
ldd --version 2>&1 | head -n1
host_stdcxx="$(/sbin/ldconfig -p 2>/dev/null | awk '/libstdc\+\+\.so\.6 /{print $NF; exit}')"
if [[ -n "$host_stdcxx" ]]; then
  echo "host libstdc++: $host_stdcxx ($(grep -a -o 'GLIBCXX_3\.4\.[0-9]*' "$host_stdcxx" | sort -V | tail -n1))"
else
  echo "host libstdc++: none"
fi

note "AppImage runtime"
file -b "$image" | cut -c1-200

note "Extract"
"$image" --appimage-extract >/dev/null || { echo "FAIL: --appimage-extract"; exit 1; }
app="$run_dir/squashfs-root"
ls -l "$app/AppRun"
[[ -x "$app/AppRun" && ! -L "$app/AppRun" ]] || bad "AppRun is not an executable launcher script"

# Ask the launcher itself which library path it would use on this host.
launcher_path="$("${clean_env[@]}" sh -c '
  appdir="$1"; stub="$2"
  mkdir -p "$stub/usr/bin"
  for entry in "$appdir"/* ; do
    [ "$(basename "$entry")" = usr ] || ln -s "$entry" "$stub/"
  done
  for entry in "$appdir"/usr/*; do
    [ "$(basename "$entry")" = bin ] || ln -s "$entry" "$stub/usr/"
  done
  rm -f "$stub/AppRun"; cp "$appdir/AppRun" "$stub/AppRun"
  printf "#!/bin/sh\nprintf %%s \"\$LD_LIBRARY_PATH\"\n" > "$stub/usr/bin/vortia"
  chmod +x "$stub/usr/bin/vortia"
  "$stub/AppRun"' sh "$app" "$work/stub" 2>/dev/null | sed "s|$work/stub|$app|g")"
[[ -n "$launcher_path" ]] || launcher_path="$app/usr/lib"
echo "launcher LD_LIBRARY_PATH: ${launcher_path//$app/\$APPDIR}"
case "$launcher_path" in
  *optional/gcc-runtime*) echo "C++ runtime: bundled" ;;
  *) echo "C++ runtime: host" ;;
esac

note "Library resolution (as the launcher resolves it)"
report="$work/ldd.txt"
# The engine has its own library tree and is exercised by --verify-engine.
find "$app/usr/bin/vortia" "$app/usr/plugins" "$app/usr/lib" \
  -path "$app/usr/lib/openfoam-engine" -prune -o -type f \
  \( -name 'vortia' -o -name '*.so' -o -name '*.so.*' \) -print0 |
  while IFS= read -r -d '' elf; do
    LD_LIBRARY_PATH="$launcher_path" ldd -r "$elf" 2>&1 | sed "s|^|$(basename "$elf"): |"
  done > "$report"
problems="$(grep -E 'not found|undefined symbol' "$report" | sed "s|$app|\$APPDIR|g" | sort -u)"
if [[ -n "$problems" ]]; then
  echo "$problems" | cut -c1-240 | head -n 40
  # Only the executable, Qt itself and the xcb platform plugin are hard
  # requirements; optional plugins are reported, not fatal.
  if grep -qE '^(vortia|libqxcb\.so|libQt6[A-Za-z]+\.so\.6|libvtk[^:]*): ' <<<"$problems"; then
    bad "executable, Qt, VTK or the xcb platform plugin has unresolved libraries or symbols"
  fi
else
  echo "nothing unresolved"
fi
# A library we ship must never be taken from the host instead.
leaks="$(sed -nE 's/^[^:]+: \t?([^ ]+) => (\/.*) \(0x[0-9a-f]+\)$/\1\t\2/p' "$report" | sort -u |
  while IFS=$'\t' read -r name path; do
    [[ "$path" == "$app/"* ]] && continue
    if [[ -e "$app/usr/lib/$name" ]]; then echo "$name resolved to host copy $path"; fi
  done)"
if [[ -n "$leaks" ]]; then
  echo "$leaks" | head -n 20
  bad "bundled libraries are shadowed by host copies"
fi

note "Headless start (--verify-spanish)"
"${clean_env[@]}" "$app/AppRun" --verify-spanish || bad "rc=$?"

note "Full application start on a clean X display"
log="$work/start.log"
# 0.3.0 introduced the launcher environment hand-over and --verify-startup.
modern=0
grep -q 'VORTIA_LAUNCHER_ENV' "$app/AppRun" 2>/dev/null && modern=1
if ((modern)); then
  "${clean_env[@]}" timeout --signal=TERM --kill-after=10 60 \
    "${xvfb[@]}" "$app/AppRun" --verify-startup >"$log" 2>&1
  rc=$?
  sed 's/^/  | /' "$log" | head -n 60
  [[ "$rc" -eq 0 ]] && echo "OK: main window and OpenGL viewport came up" || bad "--verify-startup rc=$rc"
else
  # Releases before 0.3.0: the GUI has no reason to exit by itself, so still
  # running at the timeout (124) means the main window came up.
  "${clean_env[@]}" timeout --signal=TERM --kill-after=10 25 \
    "${xvfb[@]}" "$app/AppRun" >"$log" 2>&1
  rc=$?
  sed 's/^/  | /' "$log" | head -n 60
  [[ "$rc" -eq 124 ]] && echo "OK: application was still running after 25 s" || bad "application exited by itself, rc=$rc"
fi
if grep -qiE 'could not (load|find) the qt platform plugin|GLIBCXX_[0-9.]+.? not found|cannot open shared object|Fontconfig error|xkbcommon: ERROR' "$log"; then
  bad "startup log reports a missing plugin, library, font configuration or keyboard data"
fi

if [[ -x "$app/AppRun" && ! -L "$app/AppRun" ]]; then
  note "Hostile environment: foreign LD_LIBRARY_PATH and Qt plugin path"
  mkdir -p "$work/foreign"
  "${clean_env[@]}" LD_LIBRARY_PATH="$work/foreign:/usr/lib/x86_64-linux-gnu:/usr/lib64" \
    QT_PLUGIN_PATH="$work/foreign" "$app/AppRun" --verify-spanish || bad "rc=$?"

  note "Desktop launch without a terminal: failures leave a log"
  rc=0
  "${clean_env[@]}" QT_QPA_PLATFORM=vortia-missing-platform \
    env -u VORTIA_NO_STARTUP_DIALOG timeout 30 "$app/AppRun" </dev/null >/dev/null 2>/dev/null || rc=$?
  startup_log="$work/home/.local/state/vortia/startup.log"
  if [[ "$rc" -ge 126 && -s "$startup_log" ]]; then
    echo "OK: rc=$rc, log kept at ~/.local/state/vortia/startup.log:"
    head -n 3 "$startup_log" | sed 's/^/  | /'
  else
    bad "expected a failing start with a startup log (rc=$rc)"
  fi
fi

if ((modern)); then
  note "No usable OpenGL: a clear message instead of a crash"
  # Mesa is told to offer OpenGL 2.1 only, for the driver and for the
  # software-rendering retry alike.
  rc=0
  "${clean_env[@]}" MESA_GL_VERSION_OVERRIDE=2.1 timeout --kill-after=10 60 \
    "${xvfb[@]}" "$app/AppRun" --verify-startup >"$log" 2>&1 || rc=$?
  sed 's/^/  | /' "$log" | head -n 12
  if [[ "$rc" -eq 5 ]] && grep -q 'OpenGL 3.2' "$log" && grep -q 'software rendering' "$log"; then
    echo "OK: reported and exited with rc=5 after retrying with software rendering"
  elif [[ "$rc" -eq 0 ]]; then
    # Some Mesa builds ignore the override for core contexts: nothing was
    # withheld, so nothing could be tested here. Any crash still fails.
    echo "NOT EXERCISED: this Mesa still offered OpenGL 3.2 despite the override"
  else
    bad "expected rc=5 with an OpenGL explanation, got rc=$rc"
  fi
fi

if [[ -n "${VORTIA_SCREENSHOT_DIR:-}" ]] && ((modern)); then
  note "Screenshots of the main window (integrated top bar)"
  mkdir -p "$VORTIA_SCREENSHOT_DIR"
  for theme in light dark; do
    shot="$VORTIA_SCREENSHOT_DIR/vortia-$(. /etc/os-release && echo "$ID-$VERSION_ID")-$theme.png"
    "${clean_env[@]}" VORTIA_THEME="$theme" timeout --kill-after=10 60 \
      "${xvfb[@]}" "$app/AppRun" --screenshot "$shot" >/dev/null 2>&1 || bad "--screenshot ($theme) rc=$?"
    [[ -s "$shot" ]] && echo "saved $(basename "$shot") ($(stat -c %s "$shot") bytes)" || bad "no screenshot for $theme"
  done
fi

if [[ -d "$app/usr/Resources/gmsh" ]]; then
  note "Packaged mesher (Python + Gmsh)"
  command -v python3 >/dev/null 2>&1 && echo "note: host already provides python3"
  "${clean_env[@]}" "$app/AppRun" --verify-mesher || bad "--verify-mesher rc=$?"
elif [[ -n "${VORTIA_EXPECT_ENGINE:-}" ]]; then
  bad "this AppImage has no packaged mesher"
fi

if compgen -G "$app/usr/lib/openfoam-engine/OpenFOAM-*/etc/bashrc" >/dev/null; then
  note "Integrated OpenFOAM engine: real serial and 2-process MPI run"
  echo "AppImage states minimum glibc: $(cat "$app/usr/share/vortia/GLIBC_REQUIRED" 2>/dev/null || echo unknown)"
  # No OpenFOAM, MPI, ssh or compiler exists on this machine. Open MPI
  # refuses to run as root (containers) unless told otherwise.
  engine_env=()
  if [[ "$(id -u)" -eq 0 ]]; then
    engine_env=(OMPI_ALLOW_RUN_AS_ROOT=1 OMPI_ALLOW_RUN_AS_ROOT_CONFIRM=1)
  fi
  for absent in blockMesh simpleFoam mpirun; do
    command -v "$absent" >/dev/null 2>&1 && echo "note: host already provides $absent"
  done
  rc=0
  "${clean_env[@]}" "${engine_env[@]}" timeout --kill-after=10 600 \
    "$app/AppRun" --verify-engine >"$log" 2>&1 || rc=$?
  sed 's/^/  | /' "$log" | tail -n 40 | cut -c1-300
  if [[ "$rc" -eq 0 ]] && grep -q 'verified with a real serial and parallel run' "$log"; then
    echo "OK: integrated engine ran a case serially and in parallel"
  else
    bad "--verify-engine rc=$rc"
  fi
elif [[ -n "${VORTIA_EXPECT_ENGINE:-}" ]]; then
  bad "this AppImage has no integrated OpenFOAM engine"
fi

if [[ "$mode" == "--fuse" ]]; then
  note "Direct run of the image file (FUSE mount, as a double click)"
  "${clean_env[@]}" "$image" --verify-spanish || bad "direct --verify-spanish rc=$?"
  if ((modern)); then
    "${clean_env[@]}" timeout --kill-after=10 60 "${xvfb[@]}" "$image" --verify-startup \
      || bad "direct --verify-startup rc=$?"
  fi
fi

echo
[[ "$fail" -eq 0 ]] && echo "SMOKE TEST PASSED" || echo "SMOKE TEST FAILED"
exit "$fail"
