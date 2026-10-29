# Builds the Desktop 95 fonts: Pixelify Sans 400, 500 and 700 as static instances with a
# new "5" (the original reads as S), renamed "Pixel 95" and subset to Latin. VT323, the
# timer font, is the Latin subset Phosphor ships. SRC_DIR holds PixelifySans.ttf (the
# variable font) from github.com/google/fonts (ofl/pixelifysans). Needs
# fontTools.
#   python tools/skin-art/dv-os95-fonts.py SRC_DIR OUT_DIR
import sys, os
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools import subset

SRC, OUT = sys.argv[1], sys.argv[2]
LATIN = ("U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,"
         "U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD")

# Measured on the original glyphs (S, 2, 5) at each weight: the outer box, the stem and
# bar thickness, the corner step and the top of the middle bar.
GRID = {
    400: dict(L=60, R=525, B=-12, T=631, tv=101, th=100, cut=92, step=90, mid=359),
    # 500 (the Quest Menu's names): a third of the way from 400 to 700, as the variable
    # font has only those two masters and no avar table.
    500: dict(L=60, R=531, B=-12, T=633, tv=110, th=112, cut=91, step=88, mid=366),
    700: dict(L=61, R=542, B=-11, T=638, tv=127, th=136, cut=89, step=85, mid=381),
}


def five(g):
    L, R, B, T = g["L"], g["R"], g["B"], g["T"]
    tv, th, cut, step, mid = g["tv"], g["th"], g["cut"], g["step"], g["mid"]
    rects = [
        (L, T - th, R, T),                     # top bar, square corners: the 5's flag
        (L, mid, L + tv, T - th),              # left stem down from the flag
        (L, mid - th, R - cut, mid),           # middle bar, its right end rounded off
        (R - tv, B + step, R, mid - th),       # the bowl's right side
        (L + cut, B, R - cut, B + th),         # bottom bar, both ends rounded off
        (L, B + step, L + tv, B + step + th),  # the foot at the bottom left
    ]
    pen = TTGlyphPen(None)
    for x0, y0, x1, y1 in rects:
        # Clockwise, as TrueType outer contours are.
        pen.moveTo((x0, y0)); pen.lineTo((x0, y1)); pen.lineTo((x1, y1)); pen.lineTo((x1, y0)); pen.closePath()
    return pen.glyph()


def rename(font, family, style, weight):
    full = f"{family} {style}" if style != "Regular" else family
    ps = f"{family.replace(' ', '')}-{style}"
    name = font["name"]
    for rec in list(name.names):
        if rec.nameID in (16, 17, 21, 22, 25):
            name.removeNames(nameID=rec.nameID)
    for nid, val in ((1, family), (2, "Regular" if weight == 400 else style), (3, f"{ps};thrasher"), (4, full), (6, ps)):
        name.setName(val, nid, 3, 1, 0x409)
        name.setName(val, nid, 1, 0, 0)
    if weight != 400:
        # A family of its own per weight would split; keep one family, picked by weight.
        name.setName(family, 1, 3, 1, 0x409)
        name.setName(style, 2, 3, 1, 0x409)
    font["OS/2"].usWeightClass = weight
    font["OS/2"].fsSelection = (font["OS/2"].fsSelection & ~0b1100001) | (0b100000 if weight == 700 else 0b1000000 if weight == 400 else 0)
    font["head"].macStyle = 1 if weight == 700 else 0


def do_subset(path_in, path_out, unicodes=LATIN):
    opts = subset.Options()
    opts.layout_features = ["*"]
    opts.name_IDs = ["*"]
    opts.name_languages = ["*"]
    opts.notdef_outline = True
    # Slint's rasteriser does not use TrueType hinting.
    opts.hinting = False
    f = TTFont(path_in)
    s = subset.Subsetter(opts)
    s.populate(unicodes=subset.parse_unicodes(unicodes))
    s.subset(f)
    f.save(path_out)


for w, style in ((400, "Regular"), (500, "Medium"), (700, "Bold")):
    f = TTFont(os.path.join(SRC, "PixelifySans.ttf"))
    st = instancer.instantiateVariableFont(f, {"wght": w}, updateFontNames=False)
    glyf = st["glyf"]
    name5 = st.getBestCmap()[ord("5")]
    glyf[name5] = five(GRID[w])
    glyf[name5].recalcBounds(glyf)
    rename(st, "Pixel 95", style, w)
    tmp = os.path.join(OUT, f"_tmp{w}.ttf")
    st.save(tmp)
    do_subset(tmp, os.path.join(OUT, f"Pixel95-{w}.ttf"))
    os.remove(tmp)

# VT323 (the timer and the scope's tags) is shared with Phosphor, which ships its Latin
# subset as ui/fonts/VT323-400.ttf; that covers the printable ASCII used here, so this
# script does not build it.
for n in os.listdir(OUT):
    print(n, os.path.getsize(os.path.join(OUT, n)))
