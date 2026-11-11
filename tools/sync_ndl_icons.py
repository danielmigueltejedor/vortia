"""Copy Vortia's icons from nodalia-design, the icon set shared by the Nodalia apps.

Usage: python3 tools/sync_ndl_icons.py <nodalia-design checkout>
       python3 tools/sync_ndl_icons.py --check

Build the icons there first (npm run build -w @nodalia/icons). Every icon listed
under /vortia/icons in resources/vortia.qrc is copied unchanged from
packages/icons/dist/svg; the checkout's commit is recorded in NDL_REV.
"""
import pathlib
import re
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
QRC = ROOT / "resources" / "vortia.qrc"
DEST = ROOT / "resources" / "icons" / "ui"


def names():
    text = QRC.read_text(encoding="utf-8")
    return sorted(set(re.findall(r'>icons/ui/([a-z0-9-]+)\.svg<', text)))


def check():
    """Every icon the app loads is present, and its NDL revision is recorded."""
    problems = [f"missing {n}.svg" for n in names() if not (DEST / f"{n}.svg").is_file()]
    if not re.fullmatch(r"[0-9a-f]{40}\n", (DEST / "NDL_REV").read_text(encoding="utf-8")
                        if (DEST / "NDL_REV").is_file() else ""):
        problems.append("NDL_REV is missing or not a commit")
    problems += [f"{p.name} is not listed in vortia.qrc" for p in DEST.glob("*.svg")
                 if p.stem not in names()]
    if problems:
        sys.exit("\n".join(problems))
    print(f"{len(names())} icons from nodalia-design")


def main():
    if sys.argv[1:] == ["--check"]:
        check()
        return
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    ndl = pathlib.Path(sys.argv[1]).resolve()
    source = ndl / "packages" / "icons" / "dist" / "svg"
    missing = [n for n in names() if not (source / f"{n}.svg").is_file()]
    if missing:
        sys.exit(f"not in {source}: {', '.join(missing)}")
    for old in DEST.glob("*.svg"):
        old.unlink()
    for n in names():
        shutil.copyfile(source / f"{n}.svg", DEST / f"{n}.svg")
    rev = subprocess.run(["git", "-C", str(ndl), "rev-parse", "HEAD"],
                         capture_output=True, text=True, check=True).stdout.strip()
    (DEST / "NDL_REV").write_text(rev + "\n", encoding="utf-8")
    print(f"{len(names())} icons from nodalia-design {rev[:7]}")


if __name__ == "__main__":
    main()
