#!/usr/bin/env python3
"""Make Vortia.app self-contained and ad-hoc signed.

Usage: deploy_bundle.py <Vortia.app> <qt-plugins-dir> [--min-macos X.Y]

Copies every non-system library the executable and the needed Qt plugins
link to (Qt, VTK, HDF5, ...; plain dylibs or frameworks, Homebrew or
conda-forge) into Contents/Frameworks, rewrites all references to @rpath,
installs the Qt plugins in Contents/PlugIns, records the real minimum macOS
version of the bundled binaries in Info.plist and signs the result ad-hoc.

Ad-hoc signing is not Apple notarization: Gatekeeper still warns on first
open of a downloaded copy.
"""
from __future__ import annotations

import argparse
import os
import plistlib
import re
import shutil
import subprocess
import sys
from pathlib import Path

SYSTEM_PREFIXES = ("/usr/lib/", "/System/")
# Plugin directories Vortia uses; everything else (QML, virtual keyboard,
# PDF, multimedia) is deliberately left out.
PLUGINS = {
    "platforms": ["libqcocoa.dylib"],
    "styles": ["libqmacstyle.dylib"],
    "imageformats": ["libqsvg.dylib", "libqjpeg.dylib", "libqgif.dylib", "libqico.dylib", "libqicns.dylib"],
    "iconengines": ["libqsvgicon.dylib"],
    "tls": None,  # all back ends available in this Qt
    "networkinformation": None,
}


def run(*args: str) -> str:
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout


def is_macho(path: Path) -> bool:
    if path.is_symlink() or not path.is_file():
        return False
    with path.open("rb") as handle:
        return handle.read(4) in (b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf")


def dependencies(path: Path) -> list[str]:
    lines = run("otool", "-L", str(path)).splitlines()[1:]
    return [line.strip().split(" (compatibility")[0] for line in lines if line.strip()]


def rpaths(path: Path) -> list[str]:
    out = run("otool", "-l", str(path))
    return re.findall(r"cmd LC_RPATH\n\s+cmdsize \d+\n\s+path (.+?) \(offset", out)


def min_macos(path: Path) -> tuple[int, ...] | None:
    match = re.search(r"minos (\d+(?:\.\d+)*)", run("vtool", "-show-build", str(path)))
    return tuple(int(part) for part in match.group(1).split(".")) if match else None


def framework_root(path: Path) -> Path | None:
    for parent in path.parents:
        if parent.suffix == ".framework":
            return parent
    return None


class Bundle:
    def __init__(self, app: Path, search: list[Path]):
        self.app = app
        self.frameworks = app / "Contents" / "Frameworks"
        self.plugins = app / "Contents" / "PlugIns"
        self.executable = app / "Contents" / "MacOS" / "vortia"
        self.search = search
        # install name inside the bundle -> bundled binary
        self.bundled: dict[str, Path] = {}
        self.todo: list[tuple[Path, Path]] = []  # (bundled copy, original location)

    def resolve(self, reference: str, origin: Path, origin_rpaths: list[str]) -> Path | None:
        def expand(text: str) -> str:
            return (text.replace("@loader_path", str(origin.parent))
                        .replace("@executable_path", str(origin.parent)))

        if reference.startswith("@rpath/"):
            tail = reference[len("@rpath/"):]
            for base in [expand(r) for r in origin_rpaths] + [str(s) for s in self.search]:
                candidate = Path(base) / tail
                if candidate.exists():
                    return candidate
            return None
        candidate = Path(expand(reference))
        return candidate if candidate.exists() else None

    def adopt(self, source: Path) -> tuple[str, Path]:
        """Copy a library (or its whole framework) into the bundle once."""
        real = source.resolve()
        root = framework_root(real)
        if root is not None:
            inner = real.relative_to(root)
            name = f"@rpath/{root.name}/{inner}"
            target = self.frameworks / root.name / inner
            if name not in self.bundled:
                shutil.copytree(root, self.frameworks / root.name, symlinks=True,
                                ignore=shutil.ignore_patterns("Headers", "*.prl", "*.dSYM"),
                                dirs_exist_ok=True)
        else:
            # Keep the name the dependants ask for (libfoo.1.dylib), not the
            # fully versioned file a symlink may point to.
            name = f"@rpath/{source.name}"
            target = self.frameworks / source.name
            if name not in self.bundled:
                shutil.copy2(real, target)
        if name not in self.bundled:
            os.chmod(target, 0o755)
            self.bundled[name] = target
            self.todo.append((target, real))
        return name, target

    def relink(self, binary: Path, original: Path) -> None:
        original_rpaths = rpaths(original)
        changes: list[str] = []
        for reference in dependencies(original):
            if reference.startswith(SYSTEM_PREFIXES):
                continue
            source = self.resolve(reference, original, original_rpaths)
            if source is None:
                raise SystemExit(f"error: cannot resolve {reference} needed by {original}")
            if source.resolve() == original.resolve():
                continue  # the library's own id
            name, _ = self.adopt(source)
            if name != reference:
                changes += ["-change", reference, name]
        for rpath in dict.fromkeys(rpaths(binary)):
            changes += ["-delete_rpath", rpath]
        relative = os.path.relpath(self.frameworks, binary.parent)
        prefix = "@executable_path" if binary == self.executable else "@loader_path"
        changes += ["-add_rpath", f"{prefix}/{relative}" if relative != "." else prefix]
        if binary != self.executable and self.plugins not in binary.parents:
            own = next(name for name, path in self.bundled.items() if path == binary)
            changes += ["-id", own]
        subprocess.run(["install_name_tool", *changes, str(binary)], check=True,
                       capture_output=True)

    def deploy(self, qt_plugins: Path) -> None:
        self.frameworks.mkdir(parents=True, exist_ok=True)
        roots: list[tuple[Path, Path]] = [(self.executable, self.executable)]
        for category, names in PLUGINS.items():
            source_dir = qt_plugins / category
            if not source_dir.is_dir():
                if names:
                    raise SystemExit(f"error: missing Qt plugin directory {source_dir}")
                continue
            for source in sorted(source_dir.glob("*.dylib")):
                if names is not None and source.name not in names:
                    continue
                if source.name.endswith("_debug.dylib"):
                    continue
                target = self.plugins / category / source.name
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source.resolve(), target)
                os.chmod(target, 0o755)
                roots.append((target, source.resolve()))
            if names:
                for name in names:
                    if not (self.plugins / category / name).exists():
                        raise SystemExit(f"error: missing Qt plugin {category}/{name}")
        # The original executable keeps its build-tree rpaths until relinked.
        for binary, original in roots:
            self.relink(binary, original)
        while self.todo:
            binary, original = self.todo.pop()
            self.relink(binary, original)
        resources = self.app / "Contents" / "Resources"
        resources.mkdir(exist_ok=True)
        (resources / "qt.conf").write_text("[Paths]\nPlugins = PlugIns\n")

    def binaries(self) -> list[Path]:
        found = [p for p in (self.app / "Contents").rglob("*") if is_macho(p)]
        return sorted(found, key=lambda p: -len(p.parts))

    def check_closed(self) -> None:
        """No bundled binary may still point outside the bundle or the system."""
        for binary in self.binaries():
            for reference in dependencies(binary):
                if not reference.startswith(SYSTEM_PREFIXES + ("@rpath/",)):
                    raise SystemExit(f"error: {binary} still references {reference}")
                if reference.startswith("@rpath/") and reference not in self.bundled:
                    raise SystemExit(f"error: {binary} references unbundled {reference}")

    def sign(self) -> None:
        for binary in self.binaries():
            if binary == self.executable:
                continue
            subprocess.run(["codesign", "--force", "--sign", "-", "--timestamp=none", str(binary)],
                           check=True, capture_output=True)
        for framework in sorted(self.frameworks.glob("*.framework")):
            subprocess.run(["codesign", "--force", "--sign", "-", "--timestamp=none", str(framework)],
                           check=True, capture_output=True)
        subprocess.run(["codesign", "--force", "--sign", "-", "--timestamp=none", str(self.app)],
                       check=True, capture_output=True)
        subprocess.run(["codesign", "--verify", "--deep", "--strict", str(self.app)], check=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("app", type=Path)
    parser.add_argument("qt_plugins", type=Path)
    parser.add_argument("--search", type=Path, action="append", default=[],
                        help="extra directory to resolve @rpath references in")
    parser.add_argument("--max-min-macos", default=None,
                        help="fail if any bundled binary needs a newer macOS than this")
    args = parser.parse_args()

    app = args.app.resolve()
    bundle = Bundle(app, [p.resolve() for p in args.search])
    if not is_macho(bundle.executable):
        raise SystemExit(f"error: {bundle.executable} is not a Mach-O executable")
    bundle.deploy(args.qt_plugins.resolve())
    bundle.check_closed()

    newest: tuple[int, ...] = (0,)
    culprit = bundle.executable
    for binary in bundle.binaries():
        needed = min_macos(binary)
        if needed and needed > newest:
            newest, culprit = needed, binary
    minimum = ".".join(str(part) for part in newest[:2])
    print(f"Minimum macOS of the bundle: {minimum} (set by {culprit.relative_to(app)})")
    if args.max_min_macos:
        limit = tuple(int(part) for part in args.max_min_macos.split("."))
        if newest[:len(limit)] > limit:
            raise SystemExit(f"error: bundle needs macOS {minimum}, above the allowed {args.max_min_macos}")

    plist_path = app / "Contents" / "Info.plist"
    with plist_path.open("rb") as handle:
        plist = plistlib.load(handle)
    plist["LSMinimumSystemVersion"] = minimum
    with plist_path.open("wb") as handle:
        plistlib.dump(plist, handle)

    bundle.sign()
    print(f"Deployed and ad-hoc signed {app} ({len(bundle.bundled)} bundled libraries)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
