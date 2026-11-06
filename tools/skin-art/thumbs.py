"""Cuts the theme gallery thumbnails (docs/device-themes.md, section 4).

    python tools/skin-art/thumbs.py            (all 12 device skins)
    python tools/skin-art/thumbs.py tape x-dash

Each device skin's mockup screenshot, docs/mockups/device-themes/<skin>.png, holds the
wide window of that skin. The window is found on the page background, cut to
1000 x 620, scaled down to the tile (104 x 64, and 208 x 128 for 2x) and quantised to
64 colours. Writes crates/thrasher-app/ui/skins/thumbs/<skin>.png and <skin>@2x.png.
Needs Python with Pillow. The classic themes have no file: their tiles are drawn live
from the palette.
"""

import glob
import os
import sys

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SRC = os.path.join(ROOT, "docs", "mockups", "device-themes")
OUT = os.path.join(ROOT, "crates", "thrasher-app", "ui", "skins", "thumbs")
SIZES = {"": (104, 64), "@2x": (208, 128)}
# Skins Max removed (2026-11-06); their mockups stay in docs, but they get no tile.
REMOVED = {"phosphor", "x-spatial", "x-fighter"}


def diff(a, b):
    return sum(abs(x - y) for x, y in zip(a[:3], b[:3]))


def window(im):
    """The wide window on the page: its top edge, then its left edge."""
    w, h = im.size
    bg = im.getpixel((5, h - 5))
    top = next(
        y
        for y in range(85, h - 620)
        if sum(1 for x in range(480, 1440, 8) if diff(im.getpixel((x, y)), bg) > 30) > 100
    )
    left = next(
        x
        for x in range(430, 520)
        if sum(1 for y in range(top + 100, top + 600, 8) if diff(im.getpixel((x, y)), bg) > 30) > 50
    )
    return im.crop((left, top, left + 1000, top + 620))


def main(names):
    os.makedirs(OUT, exist_ok=True)
    files = sorted(glob.glob(os.path.join(SRC, "*.png")))
    files = [f for f in files if os.path.basename(f)[:-4] not in REMOVED]
    if names:
        files = [f for f in files if os.path.basename(f)[:-4] in names]
    total = 0
    for f in files:
        skin = os.path.basename(f)[:-4]
        win = window(Image.open(f).convert("RGB"))
        for suffix, size in SIZES.items():
            t = win.resize(size, Image.LANCZOS)
            q = t.quantize(colors=64, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
            p = os.path.join(OUT, f"{skin}{suffix}.png")
            q.save(p, optimize=True)
            total += os.path.getsize(p)
        print(skin)
    print(f"{len(files)} skins, {total // 1024} KB")


if __name__ == "__main__":
    main(sys.argv[1:])
