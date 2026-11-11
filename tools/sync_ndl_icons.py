"""Copy Vortia's icons from nodalia-design, the icon set shared by the Nodalia apps.

Usage: python3 tools/sync_ndl_icons.py <nodalia-design checkout>
       python3 tools/sync_ndl_icons.py --check

Vortia shows NDL's bold style (2.25 stroke, the icon's `fill` parts solid), which
the icons package builds from its sources: every icon listed under /vortia/icons
in resources/vortia.qrc is written from packages/icons/src with node, unchanged.
The checkout's commit is recorded in NDL_REV.
"""
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
QRC = ROOT / "resources" / "vortia.qrc"
DEST = ROOT / "resources" / "icons" / "ui"


EMIT = """
import { pathToFileURL } from 'node:url';
import fs from 'node:fs';
const [src, out, ...names] = process.argv.slice(1);
const { boldBody, BOLD_STROKE } = await import(pathToFileURL(`${src}/bold.mjs`));
const { ICONS } = await import(pathToFileURL(`${src}/icons.mjs`));
const missing = names.filter((n) => !ICONS[n]);
if (missing.length) { console.error(`not in NDL: ${missing.join(', ')}`); process.exit(1); }
for (const n of names) {
  fs.writeFileSync(`${out}/${n}.svg`, '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" '
    + `viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${BOLD_STROKE}" `
    + `stroke-linecap="round" stroke-linejoin="round">${boldBody(n).join('')}</svg>\\n`);
}
"""


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
    source = ndl / "packages" / "icons" / "src"
    if not (source / "bold.mjs").is_file():
        sys.exit(f"no NDL icon sources in {source}")
    for old in DEST.glob("*.svg"):
        old.unlink()
    # Fails, writing nothing, if NDL lacks an icon; the old files are then gone, so
    # a failed sync shows in git status rather than leaving a half-updated set.
    subprocess.run(["node", "--input-type=module", "-e", EMIT, str(source), str(DEST), *names()],
                   check=True)
    rev = subprocess.run(["git", "-C", str(ndl), "rev-parse", "HEAD"],
                         capture_output=True, text=True, check=True).stdout.strip()
    (DEST / "NDL_REV").write_text(rev + "\n", encoding="utf-8")
    print(f"{len(names())} icons from nodalia-design {rev[:7]}")


if __name__ == "__main__":
    main()
