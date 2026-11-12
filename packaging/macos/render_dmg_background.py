#!/usr/bin/env python3
"""Render the Vortia installer background, dmg-background.tiff, from dmg-background.svg.

    python3 packaging/macos/render_dmg_background.py packaging/macos

dmg-background.svg is drawn by tools/dmg-background/draw_background.py in nodalia-design, the
layout every Nodalia app's DMG uses: two islands on the window colour, a wing section with its
streamlines on the left, the icons on the white one (Finder draws their names in black). Its
text is outlined, so resvg needs no fonts. To change the art, change that script and regenerate
the SVG with `draw_background.py vortia dmg-background.svg`.

The Finder window shows 660x400 points with the icons at (326,205) and (574,205). The picture
is padded to 2000x1300 points with the window colour on purpose: Finder never scales it, and a
window opened wider would otherwise show a white band. The TIFF holds a 1x (72 dpi) and a 2x
(144 dpi) page. Needs resvg on PATH and Pillow 12 or later.
"""
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

WINDOW = (0xE4, 0xE6, 0xEB)  # Nodalia color.surface.window, light
AREA = (660, 400)
CANVAS = (2000, 1300)
PALETTE = 32  # flat inks plus antialiasing; a palette keeps the file small


def render(svg: Path, scale: int, tmp: Path) -> Image.Image:
    area = tmp / f"area{scale}.png"
    subprocess.run(["resvg", "--skip-system-fonts", "-w", str(AREA[0] * scale), str(svg), str(area)],
                   check=True)
    canvas = Image.new("RGB", (CANVAS[0] * scale, CANVAS[1] * scale), WINDOW)
    with Image.open(area) as im:
        canvas.paste(im.convert("RGB"), (0, 0))
    return canvas.quantize(PALETTE, method=Image.Quantize.FASTOCTREE, dither=Image.Dither.NONE)


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    out = Path(sys.argv[1])
    with tempfile.TemporaryDirectory() as tmp:
        one, two = (render(out / "dmg-background.svg", s, Path(tmp)) for s in (1, 2))
    two.encoderinfo = {"dpi": (144, 144)}
    tiff = out / "dmg-background.tiff"
    one.save(tiff, "TIFF", save_all=True, append_images=[two], compression="tiff_deflate", dpi=(72, 72))
    with Image.open(tiff) as im:
        im.seek(1)
        if im.info.get("dpi") != (144, 144):
            sys.exit("error: the 2x page isn't 144 dpi (needs Pillow 12 or later)")


main()
