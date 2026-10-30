# Builds the Cozy Cabin's own fonts. Its other type is already in the app: Jersey 10 (the
# title, the row lengths and the player's time) and Pixel 95 (Pixelify Sans with a clear 5,
# from dv-os95-fonts.py) for the names and the words.
#
# Tiny5 400 (the small capitals: READY, REC): SRC_DIR holds the variable
# "Tiny5[BLED,JITT,ROND,wdth,wght].ttf" from github.com/google/fonts (ofl/tiny5). It is cut
# to its default instance (400, no rounding, bleed or jitter) and to the capitals and a few
# signs, since the skin writes only capital words in it; so it has no 5 to mix up with S.
# Its OFL names no Reserved Font Name, so the cut keeps the name.
#
# Cozy Figures (the timer): Jersey 10's digits are 4 to 8 pixels wide, so the timer would
# shift as it runs. Here every digit gets the widest advance with its outline moved to the
# middle (on whole pixels of the font's 75 unit grid), as a family of its own with only the
# timer's characters, so no other text picks it up. Jersey 10's 5 has a flat top and a
# square corner, so it reads as 5.
#   python tools/skin-art/px-cozy-fonts.py SRC_DIR OUT_DIR
# Needs fontTools.
import os
import sys

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

SRC, OUT = sys.argv[1], sys.argv[2]
FONTS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "crates",
                     "thrasher-app", "ui", "fonts")
os.makedirs(OUT, exist_ok=True)


def options(features):
    opts = subset.Options()
    opts.layout_features = features
    opts.name_IDs = ["*"]
    opts.name_languages = ["*"]
    opts.notdef_outline = True
    # Slint does not use TrueType hinting; dropping it keeps the files small.
    opts.hinting = False
    return opts


def rename(font, family, style, ps):
    name = font["name"]
    for rec in list(name.names):
        if rec.nameID in (16, 17, 21, 22, 25):
            name.removeNames(nameID=rec.nameID)
    for nid, val in ((1, family), (2, style), (3, ps + ";thrasher"), (4, family if style == "Regular" else family + " " + style), (6, ps)):
        name.setName(val, nid, 3, 1, 0x409)
        name.setName(val, nid, 1, 0, 0)


# Tiny5 at its default instance, capitals only.
vf = TTFont(os.path.join(SRC, "Tiny5[BLED,JITT,ROND,wdth,wght].ttf"))
tiny = instancer.instantiateVariableFont(
    vf, {"wght": 400, "wdth": 100, "ROND": 0, "BLED": 0, "JITT": 0}, updateFontNames=False)
s = subset.Subsetter(options(["*"]))
s.populate(text="ABCDEFGHIJKLMNOPQRSTUVWXYZ +-.:")
s.subset(tiny)
rename(tiny, "Tiny5", "Regular", "Tiny5-Regular")
tiny.save(os.path.join(OUT, "Tiny5-400.ttf"))

# Cozy Figures from Jersey 10.
font = TTFont(os.path.join(FONTS, "Jersey10-400.ttf"))
cmap = font.getBestCmap()
hmtx = font["hmtx"]
glyf = font["glyf"]
UNIT = 75
digits = [cmap[ord(c)] for c in "0123456789"]
widest = max(hmtx[g][0] for g in digits)
for g in digits:
    adv, lsb = hmtx[g]
    shift = (widest - adv) // 2 // UNIT * UNIT
    glyph = glyf[g]
    if glyph.numberOfContours > 0:
        glyph.coordinates.translate((shift, 0))
        glyph.recalcBounds(glyf)
    hmtx[g] = (widest, lsb + shift)
s = subset.Subsetter(options([]))
s.populate(text="0123456789:. ")
s.subset(font)
rename(font, "Cozy Figures", "Regular", "CozyFigures-Regular")
font.save(os.path.join(OUT, "CozyFigures-400.ttf"))

for n in ("Tiny5-400.ttf", "CozyFigures-400.ttf"):
    print(n, os.path.getsize(os.path.join(OUT, n)))
