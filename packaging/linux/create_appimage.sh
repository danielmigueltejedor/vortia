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
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
version="${3:-$(sed -nE 's/^[[:space:]]*VERSION[[:space:]]+([0-9]+\.[0-9]+\.[0-9]+).*$/\1/p' "$repo/CMakeLists.txt" | head -n1)}"
[[ -n "$version" ]] || { echo "Cannot determine the Vortia version" >&2; exit 1; }
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

# linuxdeploy collects libraries and Qt plugins; appimagetool packs the
# final AppDir only AFTER we replace linuxdeploy's symlinked AppRun.
base_url="https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous"
plugin_url="https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/continuous"
# Refuse HTTP redirects (Sonar shell:S6506).
curl_https=(curl -fsSL --proto '=https' --tlsv1.2)
"${curl_https[@]}" "$base_url/linuxdeploy-x86_64.AppImage" -o "$tools/linuxdeploy.AppImage"
"${curl_https[@]}" "$plugin_url/linuxdeploy-plugin-qt-x86_64.AppImage" -o "$tools/linuxdeploy-plugin-qt.AppImage"
chmod +x "$tools"/linuxdeploy*.AppImage

# Use a versioned, checksum-pinned appimagetool. A rolling 'continuous'
# binary could change between CI jobs without any update to this script.
appimagetool_url="https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage"
"${curl_https[@]}" "$appimagetool_url" -o "$tools/appimagetool.AppImage"
printf '%s  %s\n' \
  'ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0' \
  "$tools/appimagetool.AppImage" | sha256sum --check --status
chmod +x "$tools/appimagetool.AppImage"

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

# Also ship the native Wayland platform plugins when this Qt provides them.
# AppRun only selects them for a Wayland session with no X display at all.
qt_plugins="$("$QMAKE" -query QT_INSTALL_PLUGINS)"
wayland_plugins=()
for plugin in libqwayland-generic.so libqwayland-egl.so; do
  [[ -f "$qt_plugins/platforms/$plugin" ]] && wayland_plugins+=("$plugin")
done
if ((${#wayland_plugins[@]})); then
  EXTRA_PLATFORM_PLUGINS="$(IFS=';'; echo "${wayland_plugins[*]}")"
  export EXTRA_PLATFORM_PLUGINS
  echo "Bundling Wayland platform plugins: $EXTRA_PLATFORM_PLUGINS"
fi

(
  cd "$work"
  "$tools/linuxdeploy.AppImage" --appimage-extract-and-run \
    --appdir "$appdir" \
    --executable "$appdir/usr/bin/vortia" \
    --desktop-file "$appdir/Vortia.desktop" \
    --icon-file "$appdir/vortia.png" \
    --plugin qt
)

# Release smoke tests previously checked only QCoreApplication; 0.1.1, 0.2.0
# and 0.2.1 therefore shipped with an AppRun symlink and no Qt plugin path.
# Fail packaging when linuxdeploy has omitted the platform plugin.
[[ -s "$appdir/usr/plugins/platforms/libqxcb.so" ]] || {
  echo "Missing bundled Qt xcb platform plugin in AppDir" >&2
  exit 1
}

# The in-app updater downloads over HTTPS.
[[ -s "$appdir/usr/plugins/tls/libqopensslbackend.so" ]] || {
  echo "Missing bundled Qt OpenSSL TLS plugin in AppDir" >&2
  exit 1
}

# linuxdeploy skips every library on the AppImage "excludelist" (harfbuzz,
# freetype, fontconfig, zlib, expat, libstdc++, ...) on the assumption that
# the build machine is the oldest supported distribution, so the host copy is
# always new enough. Our Qt and VTK come from conda-forge and need *newer*
# versions than Ubuntu 22.04/24.04 or Debian 12 provide: 0.2.1 failed there
# with "libharfbuzz.so.0: cannot open shared object file" and
# "CXXABI_1.3.15 not found". Bundle everything the build environment
# resolved, except what must come from the host: glibc, the OpenGL/driver
# stack and the core X11 protocol libraries.
env_prefix="${CONDA_PREFIX:-}"
host_only='^(ld-linux.*|libc|libm|libdl|libpthread|librt|libresolv|libutil|libnsl|libanl|libmvec|libBrokenLocale|libnss_.*|libGL|libEGL|libGLX|libOpenGL|libGLdispatch|libGLESv2|libGLU|libdrm|libgbm|libglapi|libvulkan|libX11|libX11-xcb|libxcb|libxcb-dri2|libxcb-dri3|libxcb-glx)\.so(\..*)?$'
if [[ -n "$env_prefix" && -d "$env_prefix/lib" ]]; then
  added=1
  while ((added)); do
    added=0
    while IFS= read -r dep; do
      base="$(basename "$dep")"
      [[ "$base" =~ $host_only ]] && continue
      [[ -e "$appdir/usr/lib/$base" ]] && continue
      cp -L "$dep" "$appdir/usr/lib/$base"
      chmod u+w "$appdir/usr/lib/$base"
      echo "Bundled from build environment: $base"
      added=1
    done < <(
      find "$appdir/usr" -type f \( -name 'vortia' -o -name '*.so' -o -name '*.so.*' \) -print0 |
        LD_LIBRARY_PATH="$appdir/usr/lib:$env_prefix/lib" xargs -0 -n1 ldd 2>/dev/null |
        awk -v prefix="$env_prefix/" 'index($3, prefix) == 1 { print $3 }' | sort -u
    )
  done
else
  echo "No conda build environment: relying on linuxdeploy's library set only" >&2
fi

# The C++ runtime is selected at start-up by AppRun (bundled vs host,
# whichever is newer), so it must not sit on the unconditional library path.
gcc_runtime="$appdir/usr/optional/gcc-runtime"
mkdir -p "$gcc_runtime"
for runtime_lib in libstdc++.so.6 libgcc_s.so.1; do
  [[ -e "$appdir/usr/lib/$runtime_lib" ]] && mv "$appdir/usr/lib/$runtime_lib" "$gcc_runtime/"
done
if [[ -n "$env_prefix" ]]; then
  [[ -s "$gcc_runtime/libstdc++.so.6" && -s "$gcc_runtime/libgcc_s.so.1" ]] || {
    echo "Bundled C++ runtime missing from $gcc_runtime" >&2
    exit 1
  }
fi

# Integrated CFD engine (see build_openfoam_payload.sh). Kept in its own tree
# with its own libraries: it never shares a library path with the GUI.
engine_glibc=""
if [[ -n "${VORTIA_OPENFOAM_PAYLOAD:-}" ]]; then
  payload="$(cd "$VORTIA_OPENFOAM_PAYLOAD" && pwd)"
  compgen -G "$payload/OpenFOAM-*/etc/bashrc" >/dev/null || {
    echo "No OpenFOAM-*/etc/bashrc in payload $payload" >&2
    exit 1
  }
  mkdir -p "$appdir/usr/lib/openfoam-engine" "$appdir/usr/Resources"
  cp -a "$payload/." "$appdir/usr/lib/openfoam-engine/"
  # Vortia looks for the mesher at <executable>/../Resources/{python,gmsh}.
  if [[ -d "$appdir/usr/lib/openfoam-engine/mesher" ]]; then
    mv "$appdir/usr/lib/openfoam-engine/mesher/python" "$appdir/usr/lib/openfoam-engine/mesher/gmsh" \
      "$appdir/usr/Resources/"
    rmdir "$appdir/usr/lib/openfoam-engine/mesher"
  fi
  engine_glibc="$(cat "$payload/GLIBC_REQUIRED")"
  echo "Bundled OpenFOAM engine from $payload (needs glibc $engine_glibc)"
else
  echo "No VORTIA_OPENFOAM_PAYLOAD: this AppImage will need an external OpenFOAM" >&2
fi

# libOpenGL.so.0 stays out of the unconditional path for the same reason:
# AppRun adds it only on hosts that do not have one.
glvnd="$appdir/usr/optional/glvnd"
mkdir -p "$glvnd"
if [[ -n "$env_prefix" ]]; then
  opengl_lib="$(readlink -f "$env_prefix/lib/libOpenGL.so.0" || true)"
  [[ -s "$opengl_lib" ]] || { echo "libOpenGL.so.0 missing from the build environment" >&2; exit 1; }
  cp "$opengl_lib" "$glvnd/libOpenGL.so.0"
fi

# The whole image must run on the glibc of the oldest supported distribution.
# A binary built by the runner's own compiler instead of the conda toolchain
# silently raises this (0.3.0 development builds needed glibc 2.38).
max_glibc="${VORTIA_MAX_GLIBC:-2.28}"
needed_glibc="$(
  find "$appdir/usr/bin" "$appdir/usr/lib" "$appdir/usr/plugins" "$appdir/usr/optional" \
    -path "$appdir/usr/lib/openfoam-engine" -prune -o -type f \( -name 'vortia' -o -name '*.so' -o -name '*.so.*' \) -print0 |
    { xargs -0 -n 64 objdump -T 2>/dev/null || true; } |
    grep -o 'GLIBC_[0-9][0-9.]*' | sort -u -V | tail -n 1 | cut -d_ -f2
)"
echo "Newest glibc symbol version required by the AppImage: ${needed_glibc:-unknown} (limit $max_glibc)"
if [[ -z "$needed_glibc" || "$(printf '%s\n%s\n' "$needed_glibc" "$max_glibc" | sort -V | tail -n 1)" != "$max_glibc" ]]; then
  echo "The AppImage would need glibc ${needed_glibc:-?}, newer than the supported $max_glibc. Offenders:" >&2
  find "$appdir/usr" -path "$appdir/usr/lib/openfoam-engine" -prune -o -type f \( -name 'vortia' -o -name '*.so' -o -name '*.so.*' \) -print | while read -r elf; do
    objdump -T "$elf" 2>/dev/null | grep -q "GLIBC_$needed_glibc" && echo "  $elf" >&2
  done
  exit 1
fi

# What the release notes may honestly state as the minimum distribution.
overall_glibc="$(printf '%s\n%s\n' "$needed_glibc" "${engine_glibc:-0}" | sort -V | tail -n 1)"
echo "$overall_glibc" > "$appdir/usr/share/vortia/GLIBC_REQUIRED"
echo "Minimum glibc for the complete AppImage (GUI + engine): $overall_glibc"

# Used by AppRun only on hosts without /etc/fonts/fonts.conf.
cat > "$appdir/usr/share/vortia/fonts.conf" <<'FONTS'
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <dir>/usr/share/fonts</dir>
  <dir>/usr/local/share/fonts</dir>
  <dir prefix="xdg">fonts</dir>
  <dir>~/.fonts</dir>
  <cachedir prefix="xdg">fontconfig</cachedir>
</fontconfig>
FONTS

# linuxdeploy creates AppRun -> usr/bin/vortia. That drops the Qt plugin
# environment and makes Qt search ~/Descargas/platforms instead.
rm -f "$appdir/AppRun"
install -m 0755 "$here/AppRun" "$appdir/AppRun"
[[ -x "$appdir/AppRun" && ! -L "$appdir/AppRun" ]] || {
  echo "AppRun must be an executable shell launcher, not a symlink" >&2
  exit 1
}

# Produce the final AppImage from our fully prepared AppDir, without any
# subsequent step that could overwrite the wrapper with a bare symlink.
ARCH=x86_64 VERSION="$version" \
  "$tools/appimagetool.AppImage" --appimage-extract-and-run "$appdir" "$out"
[[ -s "$out" ]] || { echo "appimagetool did not create $out" >&2; exit 1; }
chmod +x "$out"
echo "Created $out"
