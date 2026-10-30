# Builds the Fighter Select fonts from Anton (OFL, no Reserved Font Name). The body type is
# Chakra Petch 600 and 700, already in the app for the Brick Phone.
#   Anton-400.ttf            Anton subset to Latin: REPLAY, LV, the toolbar words, the
#                            tile numbers.
#   FighterOblique-400.ttf   Anton's ASCII with every outline leaned 10 degrees, as the
#                            mockup's skewX(-10deg) type (the logo, CLIP SELECT, READY, P1,
#                            FIGHT, VS). Slint cannot skew text, so the lean is in the font.
#   FighterFigures-400.ttf   the timer's characters with the 1 as wide as the other digits
#                            and centred, so the round clock holds still while it runs.
# SRC_DIR holds Anton-Regular.ttf from github.com/google/fonts (ofl/anton). Needs fontTools.
#   python tools/skin-art/x-fighter-fonts.py SRC_DIR OUT_DIR
import sys, os, math
from fontTools.ttLib import TTFont
from fontTools import subset
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.recordingPen import DecomposingRecordingPen

SRC, OUT = sys.argv[1], sys.argv[2]
ANTON = os.path.join(SRC, "Anton-Regular.ttf")
LATIN = ("U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,"
         "U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD")
os.makedirs(OUT, exist_ok=True)


def cut(font, unicodes=None, text=None, features=("*",)):
    opts = subset.Options()
    opts.layout_features = list(features)
    opts.name_IDs = ["*"]
    opts.name_languages = ["*"]
    opts.notdef_outline = True
    # Slint does not use TrueType hinting; dropping it keeps the files small.
    opts.hinting = False
    s = subset.Subsetter(opts)
    if text is not None:
        s.populate(text=text)
    else:
        s.populate(unicodes=subset.parse_unicodes(unicodes))
    s.subset(font)


def rename(font, family, ps):
    name = font["name"]
    for rec in list(name.names):
        if rec.nameID in (16, 17, 21, 22, 25):
            name.removeNames(nameID=rec.nameID)
    for nid, val in ((1, family), (2, "Regular"), (3, ps + ";thrasher"), (4, family), (6, ps)):
        name.setName(val, nid, 3, 1, 0x409)
        name.setName(val, nid, 1, 0, 0)


# Anton, Latin.
font = TTFont(ANTON)
cut(font, unicodes=LATIN)
font.save(os.path.join(OUT, "Anton-400.ttf"))
print("Anton-400.ttf", os.path.getsize(os.path.join(OUT, "Anton-400.ttf")))

# Fighter Oblique: ASCII, outlines leaned by tan(10 degrees) about the baseline.
font = TTFont(ANTON)
cut(font, unicodes="U+0020-007E", features=("kern",))
glyf = font["glyf"]
hmtx = font["hmtx"]
gs = font.getGlyphSet()
lean = math.tan(math.radians(10))
new = {}
for g in font.getGlyphOrder():
    rec = DecomposingRecordingPen(gs)
    gs[g].draw(rec)
    pen = TTGlyphPen(None)
    rec.replay(TransformPen(pen, (1, 0, lean, 1, 0, 0)))
    new[g] = pen.glyph()
for g, glyph in new.items():
    glyf[g] = glyph
    glyph.recalcBounds(glyf)
    hmtx[g] = (hmtx[g][0], getattr(glyph, "xMin", 0))
font["post"].italicAngle = -10.0
font["head"].macStyle |= 2
font["OS/2"].fsSelection = (font["OS/2"].fsSelection | 1) & ~(1 << 6)
rename(font, "Fighter Oblique", "FighterOblique-Regular")
font.save(os.path.join(OUT, "FighterOblique-400.ttf"))
print("FighterOblique-400.ttf", os.path.getsize(os.path.join(OUT, "FighterOblique-400.ttf")))

# Fighter Figures: Anton's 1 is 677 units wide, the other digits 1012.
font = TTFont(ANTON)
cut(font, text="0123456789:. ", features=())
cmap = font.getBestCmap()
hmtx = font["hmtx"]
glyf = font["glyf"]
one = cmap[ord("1")]
digit = hmtx[cmap[ord("0")]][0]
adv, lsb = hmtx[one]
shift = (digit - adv) // 2
g = glyf[one]
if g.isComposite():
    raise SystemExit("composite 1")
g.coordinates.translate((shift, 0))
g.recalcBounds(glyf)
hmtx[one] = (digit, lsb + shift)
rename(font, "Fighter Figures", "FighterFigures-Regular")
font.save(os.path.join(OUT, "FighterFigures-400.ttf"))
print("FighterFigures-400.ttf", os.path.getsize(os.path.join(OUT, "FighterFigures-400.ttf")))
