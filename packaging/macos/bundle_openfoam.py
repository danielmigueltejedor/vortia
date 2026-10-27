#!/usr/bin/env python3
"""Copy a native v2606 runtime into Vortia, retaining sources and notices.

The source is the complete native disk image, copied into the app. There is
no required external installation at runtime. Run before signing the enclosing Vortia app.
"""
import argparse
from pathlib import Path
import shutil


def bundle_engine(source, app):
    source = Path(source).resolve()
    destination = Path(app) / "Contents/Resources/OpenFOAM-v2606"
    if not source.is_file() or source.name != "OpenFOAM-v2606.dmg":
        raise ValueError("Supply the native OpenFOAM-v2606.dmg disk image")
    if destination.exists():
        raise ValueError("Engine destination already exists; refusing to replace it implicitly")
    destination.mkdir(parents=True)
    shutil.copyfile(source, destination / source.name)
    helper = destination / "volume"
    shutil.copyfile(Path(__file__).with_name("openfoam-volume"), helper)
    helper.chmod(0o755)
    if (source.parent / "LICENSE").is_file():
        shutil.copyfile(source.parent / "LICENSE", destination / "LICENSE")
    (destination / "VORTIA-PACKAGING.txt").write_text(
        "OpenFOAM v2606 native Apple Silicon runtime, bundled with Vortia.\n"
        "The complete upstream disk image preserves its case-sensitive filesystem.\n"
        "OpenFOAM remains GPL-3.0-or-later; see LICENSE.md inside the image.\n"
        "OpenFOAM sources are retained in src/, applications/, modules/ and plugins/.\n"
        "Native distribution/build recipes: https://github.com/gerlero/openfoam-app\n"
        "Upstream source: https://develop.openfoam.com/Development/openfoam\n"
        "Runtime dependencies retain their notices under env/.\n"
        "Vortia mounts its embedded image read-only and hidden from Finder.\n"
        "No external OpenFOAM.app installation or manual mount is needed.\n")
    return destination


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("app", type=Path)
    args = parser.parse_args()
    print(bundle_engine(args.source, args.app))
