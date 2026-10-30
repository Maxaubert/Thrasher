# Builds the Quest Menu's own fonts: Silkscreen 700 (the title), subset to Latin, and Quest
# Figures (the timer, below). The labels and the lengths are "Pocket Small", Silkscreen 400
# with a square 5, shared with Pocket and Brick Phone and built by px-pocket-fonts.py:
# Silkscreen's own 5 differs from its S by two pixels. The title has no digits, so
# Silkscreen 700 is used as it is. SRC_DIR holds Silkscreen-Regular.ttf and
# Silkscreen-Bold.ttf from github.com/google/fonts (ofl/silkscreen); its OFL names no
# Reserved Font Name, so the subset keeps the name. The names and the menu text are
# Pixel 95 (Pixelify Sans with a clear 5), built by dv-os95-fonts.py, which also makes the
# 500 weight the names use. Needs fontTools.
#   python tools/skin-art/px-quest-fonts.py SRC_DIR OUT_DIR
import sys, os
from fontTools.ttLib import TTFont
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools import subset

SRC, OUT = sys.argv[1], sys.argv[2]
LATIN = ("U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,"
         "U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD")

opts = subset.Options()
opts.layout_features = ["*"]
opts.name_IDs = ["*"]
opts.name_languages = ["*"]
opts.notdef_outline = True
# Slint's rasteriser does not use TrueType hinting.
opts.hinting = False
f = TTFont(os.path.join(SRC, "Silkscreen-Bold.ttf"))
s = subset.Subsetter(opts)
s.populate(unicodes=subset.parse_unicodes(LATIN))
s.subset(f)
f.save(os.path.join(OUT, "Silkscreen-700.ttf"))

# The square 5 of Pocket Small (px-pocket-fonts.py), on Silkscreen's 125 unit pixels.
U = 125
FIVE = [
    (1, 4, 4, 4),  # top bar: the 5's flag, across the whole digit
    (1, 3, 1, 3),  # the stem down the left
    (1, 2, 4, 2),  # middle bar, full width (the S has two pixels there)
    (4, 1, 4, 1),  # the bowl's right side
    (1, 0, 4, 0),  # bottom bar
]


def five():
    pen = TTGlyphPen(None)
    for c0, r0, c1, r1 in FIVE:
        x0, y0, x1, y1 = c0 * U, r0 * U, (c1 + 1) * U, (r1 + 1) * U
        # Clockwise, as TrueType outer contours are.
        pen.moveTo((x0, y0)); pen.lineTo((x0, y1)); pen.lineTo((x1, y1)); pen.lineTo((x1, y0)); pen.closePath()
    return pen.glyph()


# Quest Figures: Silkscreen 400's timer characters with tabular digits and the square 5,
# under its own family name so no other text picks it up. Silkscreen's 1 is a pixel
# narrower than the other digits, so the timer would shift each time a 1 comes or goes;
# here the 1 gets their advance and sits in the middle of it.
CHARS = "0123456789:. "
f = TTFont(os.path.join(SRC, "Silkscreen-Regular.ttf"))
cmap = f.getBestCmap()
glyf = f["glyf"]
glyf[cmap[ord("5")]] = five()
glyf[cmap[ord("5")]].recalcBounds(glyf)
width = f["hmtx"][cmap[ord("0")]][0]
one = cmap[ord("1")]
adv, lsb = f["hmtx"][one]
shift = (width - adv) // 2
g = glyf[one]
g.coordinates.translate((shift, 0))
g.recalcBounds(glyf)
f["hmtx"][one] = (width, lsb + shift)
opts = subset.Options()
opts.layout_features = []
opts.name_IDs = ["*"]
opts.notdef_outline = True
opts.hinting = False
s = subset.Subsetter(opts)
s.populate(unicodes=[ord(c) for c in CHARS])
s.subset(f)
name = f["name"]
for rec in list(name.names):
    if rec.nameID in (16, 17, 21, 22, 25):
        name.removeNames(nameID=rec.nameID)
for nid, val in ((1, "Quest Figures"), (2, "Regular"), (3, "QuestFigures-Regular"),
                 (4, "Quest Figures"), (6, "QuestFigures-Regular")):
    name.setName(val, nid, 3, 1, 0x409)
    name.setName(val, nid, 1, 0, 0)
f.save(os.path.join(OUT, "QuestFigures-400.ttf"))

for n in sorted(os.listdir(OUT)):
    print(n, os.path.getsize(os.path.join(OUT, n)))
