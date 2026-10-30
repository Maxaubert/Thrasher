# The Answering Machine fonts (docs/device-themes.md, Fonts):
#
#   python tools/skin-art/dv-answer-fonts.py SRC_DIR OUT_DIR
#
# - Barlow Condensed 600 and 700 and Courier Prime 400 and 700 (OFL, no reserved names),
#   the static TTFs from Google Fonts in SRC_DIR (BarlowCondensed-600.ttf and so on),
#   subset to Latin, with one family name per family so Slint matches the weights.
# - "Memo Segments", a seven segment display face drawn here from the mockup's own
#   segment shapes (docs/mockups/device-themes/dv-answer.html, `seg`): hexagonal
#   segments 0.13 em thick, a digit 0.56 em wide, leaning 7 degrees. Only 0 to 9, the
#   minus, the colon, the point and the space. The digits are as tall as the em, so a
#   font size of 40px gives 40px digits. Lit segments only: the unlit ones are the
#   same text as 8s in a dim colour underneath.
import math
import sys
from fontTools import subset
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

SRC, OUT = sys.argv[1], sys.argv[2]

LATIN = ("U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+2000-206F,"
         "U+2074,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD")


def unicodes(spec):
    out = []
    for part in spec.split(","):
        a = part[2:].split("-")
        lo = int(a[0], 16)
        hi = int(a[1], 16) if len(a) > 1 else lo
        out.extend(range(lo, hi + 1))
    return out


def text_font(src, out, family, style, weight):
    f = TTFont(src)
    opts = subset.Options()
    opts.layout_features = ["*"]
    opts.name_IDs = ["*"]
    opts.notdef_outline = True
    s = subset.Subsetter(opts)
    s.populate(unicodes=unicodes(LATIN))
    s.subset(f)
    name = f["name"]
    for rec in list(name.names):
        if rec.nameID in (16, 17, 21, 22, 25):
            name.removeNames(nameID=rec.nameID)
    ps = family.replace(" ", "") + "-" + style.replace(" ", "")
    for nid, val in ((1, family), (2, style), (4, family + " " + style), (6, ps), (16, family), (17, style)):
        name.setName(val, nid, 3, 1, 0x409)
    f["OS/2"].usWeightClass = weight
    f.save(out)
    print(out, "ok")


text_font(f"{SRC}/BarlowCondensed-600.ttf", f"{OUT}/BarlowCondensed-600.ttf", "Barlow Condensed", "SemiBold", 600)
text_font(f"{SRC}/BarlowCondensed-700.ttf", f"{OUT}/BarlowCondensed-700.ttf", "Barlow Condensed", "Bold", 700)
text_font(f"{SRC}/CourierPrime-400.ttf", f"{OUT}/CourierPrime-400.ttf", "Courier Prime", "Regular", 400)
text_font(f"{SRC}/CourierPrime-700.ttf", f"{OUT}/CourierPrime-700.ttf", "Courier Prime", "Bold", 700)

# ---------- Memo Segments ----------
EM = 1000
H = EM
W, T, SP = H * 0.56, H * 0.13, H * 0.16
GAP = T * 0.22
SKEW = math.tan(math.radians(7))
L, R, TOP, MID, BOT = T / 2, W - T / 2, T / 2, H / 2, H - T / 2
SEGS = {"0": "abcdef", "1": "bc", "2": "abged", "3": "abgcd", "4": "fgbc", "5": "afgcd",
        "6": "afgedc", "7": "abc", "8": "abcdefg", "9": "abcdfg", "-": "g", " ": ""}


def hseg(x0, x1, y):
    return [(x0, y), (x0 + T / 2, y - T / 2), (x1 - T / 2, y - T / 2), (x1, y), (x1 - T / 2, y + T / 2), (x0 + T / 2, y + T / 2)]


def vseg(x, y0, y1):
    return [(x, y0), (x + T / 2, y0 + T / 2), (x + T / 2, y1 - T / 2), (x, y1), (x - T / 2, y1 - T / 2), (x - T / 2, y0 + T / 2)]


POLY = {"a": hseg(L + GAP, R - GAP, TOP), "g": hseg(L + GAP, R - GAP, MID), "d": hseg(L + GAP, R - GAP, BOT),
        "f": vseg(L, TOP + GAP, MID - GAP), "b": vseg(R, TOP + GAP, MID - GAP),
        "e": vseg(L, MID + GAP, BOT - GAP), "c": vseg(R, MID + GAP, BOT - GAP)}
# The lean: the mockup's skewX(-7) about the top, so the bottom moves left; here the
# bottom stays and the top moves right by the same amount.
LEAN = SKEW * H


def to_font(pts, dx=0.0):
    # Mockup coordinates (y down, 0 at the top) to font units (y up, baseline 0).
    out = []
    for x, y in pts:
        fy = H - y
        out.append((round(x + dx + SKEW * fy), round(fy)))
    return out


def circle(cx, cy, r, n=16):
    return [(cx + r * math.cos(2 * math.pi * i / n), cy - r * math.sin(2 * math.pi * i / n)) for i in range(n)]


def draw(polys):
    pen = TTGlyphPen(None)
    for poly in polys:
        # Clockwise in font units (y up) is the outer contour for TrueType.
        area = sum(poly[i][0] * poly[(i + 1) % len(poly)][1] - poly[(i + 1) % len(poly)][0] * poly[i][1] for i in range(len(poly)))
        if area > 0:
            poly = poly[::-1]
        pen.moveTo(poly[0])
        for p in poly[1:]:
            pen.lineTo(p)
        pen.closePath()
    return pen.glyph()


glyphs = {".notdef": draw([]), "space": draw([])}
advance = {".notdef": round(W + SP), "space": round(W + SP)}
cmap = {32: "space"}
pad = round(H * 0.04)
for ch, on in SEGS.items():
    if ch == " ":
        continue
    gname = {"-": "minus"}.get(ch, "digit" + ch)
    glyphs[gname] = draw([to_font(POLY[k], pad) for k in "abcdefg" if k in on])
    advance[gname] = round(W + SP)
    cmap[ord(ch)] = gname
# The colon and the point, narrow as in the mockup.
r = T * 0.55
glyphs["colon"] = draw([to_font(circle(T * 0.7, H * 0.32, r), pad), to_font(circle(T * 0.7, H * 0.7, r), pad)])
advance["colon"] = round(T * 1.4 + SP * 0.4)
cmap[ord(":")] = "colon"
glyphs["period"] = draw([to_font(circle(T * 0.55, BOT, T * 0.6), pad)])
advance["period"] = round(T * 1.2 + SP * 0.3)
cmap[ord(".")] = "period"

order = list(glyphs.keys())
fb = FontBuilder(EM, isTTF=True)
fb.setupGlyphOrder(order)
fb.setupCharacterMap(cmap)
fb.setupGlyf(glyphs)
metrics = {}
for g in order:
    gl = glyphs[g]
    gl.recalcBounds(fb.font["glyf"])
    metrics[g] = (advance[g], getattr(gl, "xMin", 0))
fb.setupHorizontalMetrics(metrics)
fb.setupHorizontalHeader(ascent=EM, descent=0)
fb.setupNameTable({
    "familyName": "Memo Segments",
    "styleName": "Regular",
    "uniqueFontIdentifier": "MemoSegments-Regular",
    "fullName": "Memo Segments Regular",
    "psName": "MemoSegments-Regular",
    "version": "Version 1.000",
})
fb.setupOS2(sTypoAscender=EM, sTypoDescender=0, sTypoLineGap=0, usWinAscent=EM, usWinDescent=0,
            usWeightClass=400, fsType=0, achVendID="THRS")
fb.setupPost()
fb.save(f"{OUT}/MemoSegments-400.ttf")
print(f"{OUT}/MemoSegments-400.ttf ok")
