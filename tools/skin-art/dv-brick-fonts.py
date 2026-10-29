# Builds the Brick Phone fonts: Chakra Petch 600 and 700 (the brand and the key labels)
# and Brick Caps (the LCD's small caps), subset to Latin. Brick Caps is Silkscreen 400
# with the 5 redrawn on its own pixel grid as a square 5 (full top, middle and bottom
# bars), since Silkscreen's 5 differs from its S by two pixels at 8px; renamed, as OFL
# asks of a modified font (Silkscreen has no Reserved Font Name). The timer uses Pixel 95
# 700 (Pixelify Sans with a clear 5, from dv-os95-fonts.py) and the LCD words VT323,
# both already in the app. SRC_DIR holds ChakraPetch-SemiBold.ttf, ChakraPetch-Bold.ttf
# and Silkscreen-Regular.ttf from github.com/google/fonts (ofl/chakrapetch,
# ofl/silkscreen). Needs fontTools.
#   python tools/skin-art/dv-brick-fonts.py SRC_DIR OUT_DIR
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


def rename(font, family, style):
    ps = family.replace(" ", "") + "-" + style
    name = font["name"]
    for rec in list(name.names):
        if rec.nameID in (16, 17, 21, 22, 25):
            name.removeNames(nameID=rec.nameID)
    for nid, val in ((1, family), (2, style), (3, f"{ps};thrasher"), (4, family + " " + style), (6, ps)):
        name.setName(val, nid, 3, 1, 0x409)
        name.setName(val, nid, 1, 0, 0)


os.makedirs(OUT, exist_ok=True)
for src, dst in (("ChakraPetch-SemiBold.ttf", "ChakraPetch-600.ttf"),
                 ("ChakraPetch-Bold.ttf", "ChakraPetch-700.ttf"),
                 ("Silkscreen-Regular.ttf", "BrickCaps-400.ttf")):
    font = TTFont(os.path.join(SRC, src))
    if dst.startswith("BrickCaps"):
        glyf = font["glyf"]
        n5 = font.getBestCmap()[ord("5")]
        glyf[n5] = five()
        glyf[n5].recalcBounds(glyf)
        rename(font, "Brick Caps", "Regular")
    opts = subset.Options()
    opts.layout_features = ["*"]
    opts.name_IDs = ["*"]
    opts.name_languages = ["*"]
    opts.notdef_outline = True
    # Slint does not use TrueType hinting; dropping it keeps the files small.
    opts.hinting = False
    s = subset.Subsetter(opts)
    s.populate(unicodes=subset.parse_unicodes(LATIN))
    s.subset(font)
    path = os.path.join(OUT, dst)
    font.save(path)
    print(dst, os.path.getsize(path))

# The timer: Pixel 95 700's digits are all one width but the 1, so the timer would jump
# when a 1 comes or goes. "Brick Figures" keeps only the timer's characters, with the 1
# widened to the other digits and its outline moved to the middle, as a family of its
# own so no other text can pick it up. PIXEL95 is the app's Pixel95-700.ttf.
PIXEL95 = sys.argv[3] if len(sys.argv) > 3 else os.path.join(
    os.path.dirname(__file__), "..", "..", "crates", "thrasher-app", "ui", "fonts", "Pixel95-700.ttf")
font = TTFont(PIXEL95)
cmap = font.getBestCmap()
hmtx = font["hmtx"]
glyf = font["glyf"]
one = cmap[ord("1")]
digit = hmtx[cmap[ord("0")]][0]
adv, lsb = hmtx[one]
shift = (digit - adv) // 2
g = glyf[one]
g.coordinates.translate((shift, 0))
g.recalcBounds(glyf)
hmtx[one] = (digit, lsb + shift)
opts = subset.Options()
opts.layout_features = []
opts.name_IDs = ["*"]
opts.notdef_outline = True
opts.hinting = False
s = subset.Subsetter(opts)
s.populate(text="0123456789:. ")
s.subset(font)
name = font["name"]
for rec in list(name.names):
    if rec.nameID in (16, 17, 21, 22, 25):
        name.removeNames(nameID=rec.nameID)
for nid, val in ((1, "Brick Figures"), (2, "Bold"), (3, "BrickFigures-Bold;thrasher"),
                 (4, "Brick Figures Bold"), (6, "BrickFigures-Bold")):
    name.setName(val, nid, 3, 1, 0x409)
    name.setName(val, nid, 1, 0, 0)
path = os.path.join(OUT, "BrickFigures-700.ttf")
font.save(path)
print("BrickFigures-700.ttf", os.path.getsize(path))
