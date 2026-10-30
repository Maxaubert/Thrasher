# The Pinball fonts (docs/device-themes.md, Fonts):
#
#   python tools/skin-art/x-pinball-fonts.py SRC_DIR OUT_DIR
#
# - Barlow Condensed 400 (OFL, no reserved name), BarlowCondensed-Regular.ttf from
#   github.com/google/fonts (ofl/barlowcondensed) in SRC_DIR, subset to Latin with the
#   same names as the 600 and 700 cuts the Answering Machine brought
#   (tools/skin-art/dv-answer-fonts.py), so Slint matches the three weights.
# - The dot matrix display (the mockup draws it on a canvas, "no font"): four small fonts
#   drawn here from the mockup's own 5x7 glyph table (docs/mockups/device-themes/
#   x-pinball.html, `G`). One em is 8 dot rows (7 for a glyph and one between lines), so
#   a font size of 8 * pitch puts the dots `pitch` apart, and every advance is a whole
#   number of dots: the page places text on the grid by its measured width.
#     "Pinball Dots"      lit dots (0.4 pitch round) for every Latin capital, figure and
#                         sign the display can be asked to show (lowercase maps to the
#                         capitals), the unlit grid as one 8x8 block (U+2588, 0.3 pitch
#                         dots) and a single dot (U+2219) for the dotted rule.
#     "Pinball Halo"      the same glyphs as soft halos (0.62 pitch), drawn under the dots.
#     "Pinball Dots Big"  the timer's characters at twice the size: each glyph pixel is
#                         2x2 dots, still one pitch apart, as the mockup's scale 2.
#     "Pinball Halo Big"  their halos.
#   The 5 has a flat top and a square corner and the S a round top, as in the mockup.
import math
import sys
from fontTools import subset
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

SRC, OUT = sys.argv[1], sys.argv[2]

LATIN = ("U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+2000-206F,"
         "U+2074,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD")


def text_font(src, out, family, style, weight):
    f = TTFont(src)
    opts = subset.Options()
    opts.layout_features = ["*"]
    opts.name_IDs = ["*"]
    opts.notdef_outline = True
    s = subset.Subsetter(opts)
    s.populate(unicodes=subset.parse_unicodes(LATIN))
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


text_font(f"{SRC}/BarlowCondensed-Regular.ttf", f"{OUT}/BarlowCondensed-400.ttf", "Barlow Condensed", "Regular", 400)

# ---------- the dot matrix ----------
# The mockup's table, plus the capitals and signs it did not need (same 5x7 style).
G = {
    '0': ['01110', '10001', '10001', '10001', '10001', '10001', '01110'],
    '1': ['00100', '01100', '00100', '00100', '00100', '00100', '01110'],
    '2': ['01110', '10001', '00001', '00010', '00100', '01000', '11111'],
    '3': ['11111', '00010', '00100', '00010', '00001', '10001', '01110'],
    '4': ['00010', '00110', '01010', '10010', '11111', '00010', '00010'],
    '5': ['11111', '10000', '11110', '00001', '00001', '10001', '01110'],
    '6': ['00110', '01000', '10000', '11110', '10001', '10001', '01110'],
    '7': ['11111', '00001', '00010', '00100', '01000', '01000', '01000'],
    '8': ['01110', '10001', '10001', '01110', '10001', '10001', '01110'],
    '9': ['01110', '10001', '10001', '01111', '00001', '00010', '01100'],
    'A': ['01110', '10001', '10001', '11111', '10001', '10001', '10001'],
    'B': ['11110', '10001', '10001', '11110', '10001', '10001', '11110'],
    'C': ['01110', '10001', '10000', '10000', '10000', '10001', '01110'],
    'D': ['11110', '10001', '10001', '10001', '10001', '10001', '11110'],
    'E': ['11111', '10000', '10000', '11110', '10000', '10000', '11111'],
    'F': ['11111', '10000', '10000', '11110', '10000', '10000', '10000'],
    'G': ['01110', '10001', '10000', '10111', '10001', '10001', '01111'],
    'H': ['10001', '10001', '10001', '11111', '10001', '10001', '10001'],
    'I': ['111', '010', '010', '010', '010', '010', '111'],
    'J': ['00111', '00010', '00010', '00010', '00010', '10010', '01100'],
    'K': ['10001', '10010', '10100', '11000', '10100', '10010', '10001'],
    'L': ['10000', '10000', '10000', '10000', '10000', '10000', '11111'],
    'M': ['10001', '11011', '10101', '10101', '10001', '10001', '10001'],
    'N': ['10001', '10001', '11001', '10101', '10011', '10001', '10001'],
    'O': ['01110', '10001', '10001', '10001', '10001', '10001', '01110'],
    'P': ['11110', '10001', '10001', '11110', '10000', '10000', '10000'],
    'Q': ['01110', '10001', '10001', '10001', '10101', '10010', '01101'],
    'R': ['11110', '10001', '10001', '11110', '10100', '10010', '10001'],
    'S': ['01111', '10000', '10000', '01110', '00001', '00001', '11110'],
    'T': ['11111', '00100', '00100', '00100', '00100', '00100', '00100'],
    'U': ['10001', '10001', '10001', '10001', '10001', '10001', '01110'],
    'V': ['10001', '10001', '10001', '10001', '10001', '01010', '00100'],
    'W': ['10001', '10001', '10001', '10101', '10101', '10101', '01010'],
    'X': ['10001', '10001', '01010', '00100', '01010', '10001', '10001'],
    'Y': ['10001', '10001', '01010', '00100', '00100', '00100', '00100'],
    'Z': ['11111', '00001', '00010', '00100', '01000', '10000', '11111'],
    'Æ': ['01111', '10100', '10100', '11110', '10100', '10100', '10111'],
    'Ø': ['01110', '10011', '10101', '10101', '10101', '11001', '01110'],
    'Å': ['00100', '01010', '00100', '01110', '10001', '11111', '10001'],
    'Ä': ['01010', '00000', '01110', '10001', '11111', '10001', '10001'],
    'Á': ['00010', '00100', '01110', '10001', '11111', '10001', '10001'],
    'À': ['01000', '00100', '01110', '10001', '11111', '10001', '10001'],
    'Ö': ['01010', '00000', '01110', '10001', '10001', '10001', '01110'],
    'Ó': ['00010', '00100', '01110', '10001', '10001', '10001', '01110'],
    'Ò': ['01000', '00100', '01110', '10001', '10001', '10001', '01110'],
    'Ü': ['01010', '00000', '10001', '10001', '10001', '10001', '01110'],
    'Ú': ['00010', '00100', '10001', '10001', '10001', '10001', '01110'],
    'É': ['00010', '00100', '11111', '10000', '11110', '10000', '11111'],
    'È': ['01000', '00100', '11111', '10000', '11110', '10000', '11111'],
    'Ê': ['00100', '01010', '11111', '10000', '11110', '10000', '11111'],
    'Ë': ['01010', '00000', '11111', '10000', '11110', '10000', '11111'],
    'Í': ['010', '100', '111', '010', '010', '010', '111'],
    'Ñ': ['01101', '10010', '10001', '11001', '10101', '10011', '10001'],
    # Ç uses the row between lines for its tail.
    'Ç': ['01110', '10001', '10000', '10000', '10000', '10001', '01110', '00100'],
    ':': ['0', '1', '1', '0', '1', '1', '0'],
    '.': ['00', '00', '00', '00', '00', '11', '11'],
    ',': ['00', '00', '00', '00', '00', '11', '11', '10'],
    ';': ['00', '11', '11', '00', '00', '11', '11', '10'],
    '-': ['000', '000', '000', '111', '000', '000', '000'],
    '_': ['00000', '00000', '00000', '00000', '00000', '00000', '11111'],
    '/': ['001', '001', '010', '010', '010', '100', '100'],
    '\\': ['100', '100', '010', '010', '010', '001', '001'],
    ' ': ['00', '00', '00', '00', '00', '00', '00'],
    '*': ['00000', '01110', '11111', '11111', '11111', '01110', '00000'],
    '!': ['1', '1', '1', '1', '1', '0', '1'],
    '?': ['01110', '10001', '00001', '00010', '00100', '00000', '00100'],
    "'": ['1', '1', '0', '0', '0', '0', '0'],
    '"': ['101', '101', '000', '000', '000', '000', '000'],
    '`': ['10', '01', '00', '00', '00', '00', '00'],
    '(': ['01', '10', '10', '10', '10', '10', '01'],
    ')': ['10', '01', '01', '01', '01', '01', '10'],
    '[': ['11', '10', '10', '10', '10', '10', '11'],
    ']': ['11', '01', '01', '01', '01', '01', '11'],
    '{': ['011', '010', '010', '100', '010', '010', '011'],
    '}': ['110', '010', '010', '001', '010', '010', '110'],
    '<': ['000', '001', '010', '100', '010', '001', '000'],
    '>': ['000', '100', '010', '001', '010', '100', '000'],
    '+': ['000', '000', '010', '111', '010', '000', '000'],
    '=': ['000', '000', '111', '000', '111', '000', '000'],
    '|': ['1', '1', '1', '1', '1', '1', '1'],
    '~': ['00000', '00000', '01000', '10101', '00010', '00000', '00000'],
    '^': ['010', '101', '000', '000', '000', '000', '000'],
    '#': ['01010', '01010', '11111', '01010', '11111', '01010', '01010'],
    '%': ['11001', '11001', '00010', '00100', '01000', '10011', '10011'],
    '&': ['01100', '10010', '10100', '01000', '10101', '10010', '01101'],
    '@': ['01110', '10001', '10111', '10101', '10111', '10000', '01111'],
    '$': ['00100', '01111', '10100', '01110', '00101', '11110', '00100'],
    '€': ['00111', '01000', '11110', '01000', '11110', '01000', '00111'],
    '°': ['010', '101', '010', '000', '000', '000', '000'],
    '…': ['00000', '00000', '00000', '00000', '00000', '00000', '10101'],
    '–': ['0000', '0000', '0000', '1111', '0000', '0000', '0000'],
}
# Lowercase shows as the capitals (Slint's to-uppercase does it too).
LOWER = {c.lower(): c for c in G if c.isalpha() and c.lower() != c}

EM = 1000
P = EM // 8          # one dot pitch in font units
ASC = 7 * P          # rows 0 to 6 above the baseline, row 7 below it


def circle(cx, cy, r):
    # Clockwise in font units (y up), as TrueType outer contours are. A dot is a few
    # device pixels across, so 10 sides (12 for a halo) are round enough.
    n = 12 if r > 0.5 * P else 10
    return [(round(cx + r * math.cos(-2 * math.pi * i / n)), round(cy + r * math.sin(-2 * math.pi * i / n))) for i in range(n)]


def glyph_of(dots):
    pen = TTGlyphPen(None)
    for poly in dots:
        pen.moveTo(poly[0])
        for p in poly[1:]:
            pen.lineTo(p)
        pen.closePath()
    return pen.glyph()


def build(path, family, radius, scale, chars, extra=True):
    rows_above = 7 * scale  # rows of a glyph above the baseline
    asc = rows_above * P
    glyphs = {".notdef": glyph_of([])}
    advance = {".notdef": 2 * scale * P}
    cmap = {}
    order = [".notdef"]
    for ch in chars:
        rows = G[ch]
        w = len(rows[0])
        dots = []
        for r, line in enumerate(rows):
            for c, bit in enumerate(line):
                if bit != '1':
                    continue
                for a in range(scale):
                    for b in range(scale):
                        rr = r * scale + a
                        cc = c * scale + b
                        cx = (cc + 0.5) * P
                        cy = asc - (rr + 0.5) * P
                        dots.append(circle(cx, cy, radius))
        name = "uni%04X" % ord(ch)
        glyphs[name] = glyph_of(dots)
        advance[name] = (w + 1) * scale * P
        cmap[ord(ch)] = name
        order.append(name)
    for lo, up in LOWER.items():
        if up in chars and ord(up) in cmap:
            cmap[ord(lo)] = cmap[ord(up)]
    if extra:
        # The unlit grid: 8 columns by 8 rows (a line), dots 0.3 pitch round.
        dots = []
        for r in range(8):
            for c in range(8):
                dots.append(circle((c + 0.5) * P, asc - (r + 0.5) * P, 0.3 * P))
        glyphs["block"] = glyph_of(dots)
        advance["block"] = 8 * P
        cmap[0x2588] = "block"
        order.append("block")
        # One dot at row 4 (the dotted rule beside the status).
        glyphs["dot"] = glyph_of([circle(0.5 * P, asc - 3.5 * P, radius)])
        advance["dot"] = 2 * P
        cmap[0x2219] = "dot"
        order.append("dot")
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
    fb.setupHorizontalHeader(ascent=asc, descent=-P, lineGap=0)
    ps = family.replace(" ", "") + "-Regular"
    fb.setupNameTable({
        "familyName": family,
        "styleName": "Regular",
        "uniqueFontIdentifier": ps + ";thrasher",
        "fullName": family + " Regular",
        "psName": ps,
        "version": "Version 1.000",
    })
    fb.setupOS2(sTypoAscender=asc, sTypoDescender=-P, sTypoLineGap=0, usWinAscent=asc, usWinDescent=P,
                usWeightClass=400, fsType=0, achVendID="THRS", fsSelection=0x40 | 0x80, version=4)
    fb.setupPost()
    fb.save(path)
    print(path, "ok", len(order), "glyphs")


SMALL = list(G.keys())
BIG = list("0123456789:.-/ ")
build(f"{OUT}/PinballDots-400.ttf", "Pinball Dots", 0.4 * P, 1, SMALL)
build(f"{OUT}/PinballHalo-400.ttf", "Pinball Halo", 0.62 * P, 1, SMALL, extra=False)
build(f"{OUT}/PinballDotsBig-400.ttf", "Pinball Dots Big", 0.4 * P, 2, BIG, extra=False)
build(f"{OUT}/PinballHaloBig-400.ttf", "Pinball Halo Big", 0.62 * P, 2, BIG, extra=False)
