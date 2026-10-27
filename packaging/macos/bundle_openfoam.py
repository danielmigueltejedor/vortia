#!/usr/bin/env python3
"""Copy a native v2606 runtime into Vortia, retaining sources and notices.

The source is an already extracted OpenFOAM root, never a required external
installation at runtime. Run before signing the enclosing Vortia app.
"""
import argparse
from pathlib import Path
import shutil


def bundle_engine(source, app):
    source = Path(source).resolve()
    destination = Path(app) / "Contents/Resources/OpenFOAM-v2606"
    for relative in ("etc/bashrc", "LICENSE.md", "src", "applications", "env/lib",
                     "platforms/darwin64ClangDPInt32Opt/bin/blockMesh",
                     "platforms/darwin64ClangDPInt32Opt/bin/rhoSimpleFoam"):
        if not (source / relative).exists():
            raise ValueError("Incomplete native OpenFOAM root: " + relative)
    if "WM_PROJECT_VERSION=v2606" not in (source / "etc/bashrc").read_text():
        raise ValueError("Expected the supported v2606 engine")
    # No symlink may leave the copied tree or keep pointing at /Volumes.
    for path in source.rglob("*"):
        if path.is_symlink():
            if path.readlink().is_absolute() or not path.resolve().is_relative_to(source):
                raise ValueError("External engine symlink: " + str(path))
    if destination.exists():
        raise ValueError("Engine destination already exists; refusing to replace it implicitly")
    shutil.copytree(source, destination, symlinks=True)
    (destination / "VORTIA-PACKAGING.txt").write_text(
        "OpenFOAM v2606 native Apple Silicon runtime, bundled with Vortia.\n"
        "OpenFOAM remains GPL-3.0-or-later; see LICENSE.md and individual notices.\n"
        "OpenFOAM sources are retained in src/, applications/, modules/ and plugins/.\n"
        "Native distribution/build recipes: https://github.com/gerlero/openfoam-app\n"
        "Upstream source: https://develop.openfoam.com/Development/openfoam\n"
        "Runtime dependencies retain their licenses under env/.\n"
        "No external OpenFOAM.app or mounted disk image is needed.\n")
    return destination


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("app", type=Path)
    args = parser.parse_args()
    print(bundle_engine(args.source, args.app))
