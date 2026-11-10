#!/usr/bin/env bash
# Assemble a relocatable OpenFOAM engine for the Vortia AppImage.
# Usage (as root, inside an ubuntu:20.04 container):
#   build_openfoam_payload.sh <output-dir> [openfoam-version, default 2412]
#
# Ubuntu 20.04 is the oldest distribution OpenCFD publishes packages for, so
# the engine runs on any distribution with glibc >= 2.31 (checked below).
# The output directory receives:
#   OpenFOAM-v<version>/     the ESI runtime with a self-locating etc/bashrc,
#                            plus etc/vortia-runtime.sh
#   sysroot/usr/...          Open MPI (launcher, plugins, help files) and every
#                            third-party library the engine links to
#   licenses/                copyright files of everything copied
#   mesher/                  relocatable CPython and Gmsh (build_mesher_runtime.sh)
set -euo pipefail

out="$(mkdir -p "$1" && cd "$1" && pwd)"
version="${2:-2412}"
[[ -z "$(ls -A "$out")" ]] || { echo "Output directory must be empty: $out" >&2; exit 1; }
. /etc/os-release
[[ "${VERSION_CODENAME:-}" == "focal" ]] || { echo "Run this in ubuntu:20.04 (focal)" >&2; exit 1; }

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends ca-certificates curl gnupg binutils file python3
curl -fsSL --proto '=https' --tlsv1.2 https://dl.openfoam.com/pubkey.gpg |
  gpg --dearmor > /usr/share/keyrings/openfoam.gpg
echo "deb [arch=amd64 signed-by=/usr/share/keyrings/openfoam.gpg] https://dl.openfoam.com/repos/deb focal main" \
  > /etc/apt/sources.list.d/openfoam.list
apt-get update
apt-get install -y --no-install-recommends "openfoam${version}" openmpi-bin

root="/usr/lib/openfoam/openfoam${version}"
[[ -f "$root/etc/bashrc" ]] || { echo "Unexpected package layout: $root" >&2; exit 1; }
engine="$out/OpenFOAM-v${version}"
cp -a "$root" "$engine"

# The Debian package's post-install step pins the installation path in
# etc/bashrc ("Replaced WM_PROJECT_DIR by ...") and removes the upstream
# self-locating logic. Put that logic back so the engine works wherever the
# AppImage is mounted.
grep -n 'WM_PROJECT_DIR=' "$engine/etc/bashrc" || true
python3 - "$engine/etc/bashrc" "$root" <<'PY'
import sys
path, root = sys.argv[1:3]
text = open(path).read()
pinned = f'export WM_PROJECT_DIR="{root}"\n'
relocatable = (
    '# Vortia: locate this installation from the path of this file (upstream\n'
    '# behaviour) instead of the fixed path written by the Debian package.\n'
    '_vortiaProjectDir="${BASH_SOURCE:-${ZSH_NAME:+$0}}"\n'
    '_vortiaProjectDir="$(\\cd "$(dirname "$_vortiaProjectDir")/.." && \\pwd -L)"\n'
    'export WM_PROJECT_DIR="$_vortiaProjectDir"\n'
    'unset _vortiaProjectDir\n'
)
if text.count(pinned) != 1:
    sys.exit(f"etc/bashrc does not pin WM_PROJECT_DIR the way this script expects ({text.count(pinned)} matches)")
open(path, "w").write(text.replace(pinned, relocatable))
PY

sysroot="$out/sysroot"
triplet="x86_64-linux-gnu"
libdir="$sysroot/usr/lib/$triplet"
mkdir -p "$libdir" "$sysroot/usr/bin" "$sysroot/usr/share" "$out/licenses"

# Open MPI as Ubuntu lays it out below /usr, so OPAL_PREFIX relocates it.
ompi_info --path prefix --path libdir --path pkglibdir --path pkgdatadir || true
for tool in orterun orted; do
  cp -L "$(command -v "$tool")" "$sysroot/usr/bin/$tool"
done
ln -s orterun "$sysroot/usr/bin/mpirun"
ln -s orterun "$sysroot/usr/bin/mpiexec"
mkdir -p "$libdir/openmpi/lib"
cp -a "/usr/lib/$triplet/openmpi/lib/openmpi3" "$libdir/openmpi/lib/"
cp -a /usr/share/openmpi "$sysroot/usr/share/openmpi"

# Everything from the host C library and compiler runtime stays on the host:
# any distribution new enough for this engine has newer ones.
host_only='^(ld-linux.*|libc|libm|libdl|libpthread|librt|libresolv|libutil|libnsl|libanl|libmvec|libnss_.*|libstdc\+\+|libgcc_s)\.so(\..*)?$'
added=1
while ((added)); do
  added=0
  while IFS= read -r dep; do
    base="$(basename "$dep")"
    [[ "$base" =~ $host_only ]] && continue
    [[ -e "$libdir/$base" ]] && continue
    case "$dep" in "$out"/*) continue ;; esac
    cp -L "$dep" "$libdir/$base"
    package="$(dpkg -S "$(readlink -f "$dep")" 2>/dev/null | head -n1 | cut -d: -f1 || true)"
    if [[ -n "$package" && -f "/usr/share/doc/$package/copyright" ]]; then
      cp -L "/usr/share/doc/$package/copyright" "$out/licenses/$package.copyright"
    fi
    echo "Bundled $base${package:+ ($package)}"
    added=1
  done < <(
    find "$engine/platforms" "$sysroot" -type f -print0 |
      xargs -0 file | awk -F: '/ELF/{print $1}' |
      LD_LIBRARY_PATH="$libdir:$libdir/openmpi/lib" xargs -n1 ldd 2>/dev/null |
      awk '$2 == "=>" && index($3, "/") == 1 { print $3 }' | sort -u
  )
done
for package in "openfoam${version}" openmpi-bin libopenmpi3; do
  if [[ -f "/usr/share/doc/$package/copyright" ]]; then
    cp -L "/usr/share/doc/$package/copyright" "$out/licenses/$package.copyright"
  fi
done

cat > "$engine/etc/vortia-runtime.sh" <<'RUNTIME'
# Sourced by Vortia after etc/bashrc (see Detector::captureEnvironment).
# Points the engine at the Open MPI and third-party libraries packaged next
# to it, wherever the AppImage happens to be mounted.
_vortia_sysroot="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../sysroot" && pwd -P)"
export OPAL_PREFIX="$_vortia_sysroot/usr"
export PATH="$OPAL_PREFIX/bin:$PATH"
export LD_LIBRARY_PATH="$OPAL_PREFIX/lib/x86_64-linux-gnu:$OPAL_PREFIX/lib/x86_64-linux-gnu/openmpi/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
# Single-machine runs only: never look for ssh/rsh on the user's computer.
export OMPI_MCA_plm=isolated
export OMPI_MCA_rmaps_base_oversubscribe=1
unset _vortia_sysroot
RUNTIME

# objdump exits non-zero for the few ELF files without a dynamic symbol table.
# Mesher runtime (relocatable CPython + Gmsh), built here for the same
# old-distribution compatibility.
apt-get install -y --no-install-recommends unzip patchelf libglu1-mesa libxft2 libxcursor1 \
  libxinerama1 libxrender1 libxfixes3 libfontconfig1 libgomp1
bash "$(dirname "$0")/../common/build_mesher_runtime.sh" "$out/mesher"

# Self-check from a different location than the package's, with a clean
# environment and without the system Open MPI on the path.
relocated="$(mktemp -d)/engine with space"
mkdir -p "$relocated"
cp -a "$engine" "$sysroot" "$relocated/"
env -i HOME=/root PATH=/usr/bin:/bin bash --noprofile --norc -c '
  set -e
  source "$1/etc/bashrc" >/dev/null 2>&1
  source "$1/etc/vortia-runtime.sh"
  [ "$WM_PROJECT_DIR" = "$1" ] || { echo "WM_PROJECT_DIR=$WM_PROJECT_DIR" >&2; exit 1; }
  case "$(command -v blockMesh)" in "$1"/*) ;; *) echo "blockMesh not from the payload" >&2; exit 1 ;; esac
  case "$(command -v mpirun)" in "$2"/*) ;; *) echo "mpirun not from the payload" >&2; exit 1 ;; esac
  blockMesh -help >/dev/null
  echo "Relocated engine works from: $WM_PROJECT_DIR"
' bash "$relocated/OpenFOAM-v${version}" "$relocated/sysroot"
rm -rf "$(dirname "$relocated")"

needed_glibc="$(find "$out" -type f -print0 | xargs -0 file | awk -F: '/ELF/{print $1}' |
  { xargs -n 64 objdump -T 2>/dev/null || true; } |
  grep -o 'GLIBC_[0-9][0-9.]*' | sort -u -V | tail -n 1 | cut -d_ -f2)"
[[ -n "$needed_glibc" ]] || { echo "Could not determine the engine's glibc requirement" >&2; exit 1; }
echo "Newest glibc symbol version required by the engine: $needed_glibc"
echo "$needed_glibc" > "$out/GLIBC_REQUIRED"
cat > "$out/README.txt" <<EOF
OpenFOAM v${version} (OpenCFD Ltd, ESI) runtime packaged with Vortia.
Unmodified binaries from https://dl.openfoam.com/repos/deb (Ubuntu 20.04 build),
with Open MPI and third-party libraries from Ubuntu 20.04. Changes to the
package: etc/bashrc locates itself instead of using a fixed installation path,
and etc/vortia-runtime.sh is added.
OpenFOAM is GPL-3.0-or-later; source: https://develop.openfoam.com/Development/openfoam
(tag OpenFOAM-v${version}). Licences of the bundled libraries are in licenses/.
OPENFOAM is a registered trademark of OpenCFD Ltd; Vortia is not endorsed by OpenCFD.
Requires glibc >= ${needed_glibc}.
EOF
du -sh "$out"/* | sort -h
