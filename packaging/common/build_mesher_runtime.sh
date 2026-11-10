#!/usr/bin/env bash
# Assemble the mesher runtime packaged with Vortia: a relocatable CPython and
# the Gmsh Python module with its native library.
# Usage: build_mesher_runtime.sh <output-dir>
# Output: <output-dir>/python (bin/python3 ...) and <output-dir>/gmsh (gmsh.py, libgmsh).
#
# Linux: run inside ubuntu:20.04 (as build_openfoam_payload.sh does) so the
# X11/GLU libraries copied next to libgmsh are old enough for every
# supported distribution. Needs curl, unzip, and on Linux patchelf.
set -euo pipefail

out="$(mkdir -p "$1" && cd "$1" && pwd)"
[[ -z "$(ls -A "$out")" ]] || { echo "Output directory must be empty: $out" >&2; exit 1; }

python_release="20261026"
python_version="3.12.16"
gmsh_version="4.15.2"
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)
    python_target="aarch64-apple-darwin"
    python_sha="353d5f1b4fd2715e7053f12f068435d5bbad16b91e057006b5d01e0925709a7e"
    wheel="gmsh-${gmsh_version}-py2.py3-none-macosx_12_0_arm64.whl"
    wheel_sha="f6649b3e59f49272e7ee8ab282ecb4d1a6e0d627e86cf3e3b1a83fd07417e4f8"
    sha256() { shasum -a 256 "$1" | cut -d' ' -f1; } ;;
  Linux-x86_64)
    python_target="x86_64-unknown-linux-gnu"
    python_sha="ce4f6d0a6cfdb3bed6e44c2f9affd85943c76a42e25ad3a259002f31d6fb8a20"
    wheel="gmsh-${gmsh_version}-py2.py3-none-manylinux_2_24_x86_64.whl"
    wheel_sha="4076a948ce22625330d1413d4982e22b5c69fc2f0f7951f5df64c778cf54108c"
    sha256() { sha256sum "$1" | cut -d' ' -f1; } ;;
  *) echo "Unsupported platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
fetch() { # url file sha256
  curl -fsSL --proto '=https' --tlsv1.2 "$1" -o "$2"
  [[ "$(sha256 "$2")" == "$3" ]] || { echo "Checksum mismatch for $1" >&2; exit 1; }
}

fetch "https://github.com/astral-sh/python-build-standalone/releases/download/${python_release}/cpython-${python_version}+${python_release}-${python_target}-install_only_stripped.tar.gz" \
  "$work/python.tar.gz" "$python_sha"
tar -C "$out" -xzf "$work/python.tar.gz"
[[ -x "$out/python/bin/python3" ]] || { echo "Unexpected CPython archive layout" >&2; exit 1; }
# Not needed to drive Gmsh; saves about a third of the interpreter's size.
stdlib="$(echo "$out"/python/lib/python3.*)"
rm -rf "$stdlib/test" "$stdlib/idlelib" "$stdlib/tkinter" "$stdlib/turtledemo" \
  "$stdlib/ensurepip" "$stdlib/site-packages/pip" "$out/python/share" "$out/python/include"
find "$out/python" -name '__pycache__' -type d -prune -exec rm -rf {} +

# PyPI serves wheels by content-addressed path; resolve it from the JSON API.
wheel_url="$("$out/python/bin/python3" - "$gmsh_version" "$wheel" <<'PY'
import json, sys, urllib.request
version, name = sys.argv[1:3]
with urllib.request.urlopen(f"https://pypi.org/pypi/gmsh/{version}/json") as response:
    for item in json.load(response)["urls"]:
        if item["filename"] == name:
            print(item["url"])
            break
PY
)"
[[ "$wheel_url" == https://files.pythonhosted.org/* ]] || { echo "Cannot locate $wheel on PyPI" >&2; exit 1; }
fetch "$wheel_url" "$work/gmsh.whl" "$wheel_sha"
mkdir "$work/wheel" "$out/gmsh"
unzip -q "$work/gmsh.whl" -d "$work/wheel"
module="$(find "$work/wheel" -name gmsh.py | head -n 1)"
[[ -n "$module" ]] || { echo "gmsh.py not found in the wheel" >&2; exit 1; }
cp "$module" "$out/gmsh/gmsh.py"
# gmsh.py loads the library from its own directory first.
find "$work/wheel" \( -name 'libgmsh*.so*' -o -name 'libgmsh*.dylib' \) -exec cp -a {} "$out/gmsh/" \;
compgen -G "$out/gmsh/libgmsh*" >/dev/null || { echo "libgmsh not found in the wheel" >&2; exit 1; }
find "$work/wheel" -iname 'LICENSE*' -exec cp {} "$out/gmsh/" \; 2>/dev/null || true

if [[ "$(uname -s)" == "Linux" ]]; then
  # The wheel expects libGLU, libXft, ... from the system; stock desktops do
  # not all have them. Ship them beside libgmsh (OpenGL itself stays host).
  host_only='^(ld-linux.*|libc|libm|libdl|libpthread|librt|libresolv|libutil|libnsl|libstdc\+\+|libgcc_s|libGL|libGLX|libGLdispatch|libOpenGL|libEGL|libdrm|libX11|libX11-xcb|libxcb)\.so(\..*)?$'
  added=1
  while ((added)); do
    added=0
    while IFS= read -r dep; do
      base="$(basename "$dep")"
      [[ "$base" =~ $host_only ]] && continue
      [[ -e "$out/gmsh/$base" ]] && continue
      cp -L "$dep" "$out/gmsh/$base"
      echo "Bundled for Gmsh: $base"
      added=1
    done < <(find "$out/gmsh" -type f -name '*.so*' -print0 |
               LD_LIBRARY_PATH="$out/gmsh" xargs -0 -n1 ldd 2>/dev/null |
               awk '$2 == "=>" && index($3, "/") == 1 { print $3 }' | sort -u)
  done
  find "$out/gmsh" -type f -name '*.so*' -exec patchelf --set-rpath '$ORIGIN' {} \;
fi

# Prove it works from here, with a clean environment.
env -i PATH=/usr/bin:/bin PYTHONDONTWRITEBYTECODE=1 "$out/python/bin/python3" - "$out/gmsh" <<'PY'
import sys
sys.path.insert(0, sys.argv[1])
import gmsh
gmsh.initialize()
gmsh.option.setNumber("General.Terminal", 0)
gmsh.model.occ.addRectangle(0, 0, 0, 1, 1)
gmsh.model.occ.synchronize()
gmsh.model.mesh.generate(2)
nodes = len(gmsh.model.mesh.getNodes()[0])
gmsh.finalize()
assert nodes > 4, nodes
print("Mesher runtime works:", nodes, "nodes")
PY
du -sh "$out"/*
