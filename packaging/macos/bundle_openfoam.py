#!/usr/bin/env python3
"""Copy a native v2606 runtime into Vortia, retaining sources and notices.

The source is the complete native disk image, copied into the app. There is
no required external installation at runtime. Run before signing the enclosing Vortia app.
"""
import argparse
from pathlib import Path
import re
import shutil
import subprocess
import tempfile


def accept_path(path):
    """Resolve a CLI path. Relative paths must stay under cwd; absolute paths are kept after normalisation."""
    raw = Path(path).expanduser()
    resolved = raw.resolve()
    if not raw.is_absolute():
        cwd = Path.cwd().resolve()
        if not resolved.is_relative_to(cwd):
            raise ValueError(f"Refusing relative path outside the working directory: {path}")
    return resolved


def inspect_image(image):
    """Mount the image read-only and return (volume id, newest minimum macOS of its tools)."""
    with tempfile.TemporaryDirectory() as mount:
        subprocess.run(["/usr/bin/hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", mount,
                        str(image)], check=True, capture_output=True)
        try:
            volume_id = (Path(mount) / ".vol_id").read_text().strip()
            if not re.fullmatch(r"[0-9A-Fa-f-]{36}", volume_id):
                raise ValueError("The OpenFOAM image has no valid .vol_id")
            newest = (0,)
            for tool in sorted(Path(mount).glob("platforms/*/bin/*"))[:40]:
                shown = subprocess.run(["/usr/bin/vtool", "-show-build", str(tool)],
                                       capture_output=True, text=True).stdout
                match = re.search(r"minos (\d+(?:\.\d+)*)", shown)
                if match:
                    newest = max(newest, tuple(int(part) for part in match.group(1).split(".")))
            return volume_id, ".".join(str(part) for part in newest[:2])
        finally:
            subprocess.run(["/usr/bin/hdiutil", "detach", mount], check=False, capture_output=True)


def bundle_engine(source, app):
    source = accept_path(source)
    app_root = accept_path(app)
    destination = (app_root / "Contents/Resources/OpenFOAM-v2606").resolve()
    if not destination.is_relative_to(app_root):
        raise ValueError("Refusing engine destination outside the app bundle")
    if not source.is_file() or source.name != "OpenFOAM-v2606.dmg":
        raise ValueError("Supply the native OpenFOAM-v2606.dmg disk image")
    if destination.exists():
        raise ValueError("Engine destination already exists; refusing to replace it implicitly")
    destination.mkdir(parents=True)
    shutil.copyfile(source, destination / source.name)
    # The helper only accepts the exact image it was packaged with.
    volume_id, minimum_macos = inspect_image(source)
    helper = destination / "volume"
    template = Path(__file__).with_name("openfoam-volume").read_text()
    stamped, replaced = re.subn(r'^volume_id="[^"]*"$', f'volume_id="{volume_id}"', template,
                                count=1, flags=re.MULTILINE)
    if replaced != 1:
        raise ValueError("openfoam-volume has no volume_id line to stamp")
    helper.write_text(stamped)
    helper.chmod(0o755)
    (destination / "MINIMUM_MACOS").write_text(minimum_macos + "\n")
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
