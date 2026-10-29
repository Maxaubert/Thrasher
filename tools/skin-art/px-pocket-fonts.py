# Builds the Pocket fonts: Jersey 10 (the big LCD type: the timer, names, lengths) and
# Silkscreen (the small caps labels), both subset to Latin. Silkscreen's 5 differs from
# its S by two pixels at 8px, so the 5 is redrawn on Silkscreen's own pixel grid as a
# square 5 (full top, middle and bottom bars) and the family renamed "Pocket Small" (OFL,
# no Reserved Font Name). Jersey 10's 5 has a flat top, a square corner and no hook, and
# is narrower than its S, so it is kept. SRC_DIR holds Jersey10-Regular.ttf and
# Silkscreen-Regular.ttf from github.com/google/fonts (ofl/jersey10, ofl/silkscreen).
# Needs fontTools.
#   python tools/skin-art/px-pocket-fonts.py SRC_DIR OUT_DIR
import sys, os
from fontTools.ttLib import TTFont
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools import subset

SRC, OUT = sys.argv[1], sys.argv[2]
LATIN = ("U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,"
         "U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD")

# Silkscreen draws on 125 unit pixels; its digits sit in columns 1 to 4, rows 0 to 4.
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


def rename(font, family):
    ps = family.replace(" ", "") + "-Regular"
    name = font["name"]
    for rec in list(name.names):
        if rec.nameID in (16, 17, 21, 22, 25):
            name.removeNames(nameID=rec.nameID)
    for nid, val in ((1, family), (2, "Regular"), (3, f"{ps};thrasher"), (4, family), (6, ps)):
        name.setName(val, nid, 3, 1, 0x409)
        name.setName(val, nid, 1, 0, 0)


def do_subset(font, path_out):
    opts = subset.Options()
    opts.layout_features = ["*"]
    opts.name_IDs = ["*"]
    opts.name_languages = ["*"]
    opts.notdef_outline = True
    # Slint's rasteriser does not use TrueType hinting.
    opts.hinting = False
    s = subset.Subsetter(opts)
    s.populate(unicodes=subset.parse_unicodes(LATIN))
    s.subset(font)
    font.save(path_out)


j = TTFont(os.path.join(SRC, "Jersey10-Regular.ttf"))
do_subset(j, os.path.join(OUT, "Jersey10-400.ttf"))

s = TTFont(os.path.join(SRC, "Silkscreen-Regular.ttf"))
glyf = s["glyf"]
n5 = s.getBestCmap()[ord("5")]
glyf[n5] = five()
glyf[n5].recalcBounds(glyf)
rename(s, "Pocket Small")
do_subset(s, os.path.join(OUT, "PocketSmall-400.ttf"))

for n in ("Jersey10-400.ttf", "PocketSmall-400.ttf"):
    print(n, os.path.getsize(os.path.join(OUT, n)))
