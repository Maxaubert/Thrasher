# The Phonograph's fonts (docs/device-themes.md, Phonograph):
#   Cinzel 600 and 700, static instances subset to Latin (OFL, no Reserved Font Name);
#   "Phono Garamond" 600 and italic 500: EB Garamond (OFL, no Reserved Font Name) with its
#   lining tabular figures mapped as the plain digits, as the mockup's
#   `font-variant-numeric: lining-nums tabular-nums` (Slint cannot turn features on, and
#   EB Garamond's default old style 5 and 3 hang below the line). Renamed, since the
#   figures differ from the original.
#   "Phono Figures" 600: only those digits, with ':' and '.' kept as empty glyphs of no
#   width, so a timer string lays out its digits one advance apart and the counter can
#   show each digit in its own wheel (the separators are drawn on their own).
#
#   python tools/skin-art/x-phono-fonts.py SRC_DIR OUT_DIR
#
# SRC_DIR holds Cinzel[wght].ttf, EBGaramond[wght].ttf and EBGaramond-Italic[wght].ttf
# from github.com/google/fonts (ofl/cinzel, ofl/ebgaramond).
import sys
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from fontTools import subset

SRC, OUT = sys.argv[1], sys.argv[2]
LATIN = "U+0020-007E,U+00A0-00FF,U+0131,U+0152-0153,U+0160-0161,U+0178,U+017D-017E,U+2013-2014,U+2018-201E,U+2022,U+2026,U+2039-203A,U+20AC"


def unicodes():
    out = []
    for part in LATIN.split(","):
        a, _, b = part[2:].partition("-")
        out += range(int(a, 16), int(b or a, 16) + 1)
    return out


def feature_map(f, tags):
    gsub = f["GSUB"].table
    m = {}
    for tag in tags:
        for fr in gsub.FeatureList.FeatureRecord:
            if fr.FeatureTag == tag:
                for li in fr.Feature.LookupListIndex:
                    for st in gsub.LookupList.Lookup[li].SubTable:
                        if st.__class__.__name__ == "SingleSubst" or hasattr(st, "mapping"):
                            for a, b in (getattr(st, "mapping", {}) or {}).items():
                                m.setdefault(a, b)
    return m


def build(src, wght, family, style, fname, figures, only=None):
    f = instancer.instantiateVariableFont(TTFont(src), {"wght": wght})
    if figures:
        # lnum then tnum on top: the plain digit's lining glyph, then its tabular form.
        lnum = feature_map(f, ["lnum"])
        tnum = feature_map(f, ["tnum"])
        for table in f["cmap"].tables:
            if table.isUnicode():
                for cp, g in list(table.cmap.items()):
                    if chr(cp) in "0123456789":
                        g2 = lnum.get(g, g)
                        g2 = tnum.get(g2, g2)
                        table.cmap[cp] = g2
    if only:
        # The separators: no outline and no advance.
        glyf, hmtx = f["glyf"], f["hmtx"]
        cmap = f.getBestCmap()
        for ch in ":.":
            g = cmap[ord(ch)]
            glyf[g] = glyf[g].__class__()
            glyf[g].numberOfContours = 0
            hmtx[g] = (0, 0)
    opts = subset.Options()
    opts.layout_features = [] if only else ["kern", "liga", "ccmp", "locl", "mark", "mkmk"]
    opts.name_IDs = ["*"]
    opts.notdef_outline = True
    opts.hinting = False
    s = subset.Subsetter(opts)
    s.populate(unicodes=[ord(c) for c in only] if only else unicodes())
    s.subset(f)
    name = f["name"]
    for rec in list(name.names):
        if rec.nameID in (16, 17, 21, 22, 25):
            name.removeNames(nameID=rec.nameID)
    ps = family.replace(" ", "") + "-" + style.replace(" ", "")
    for nid, val in ((1, family), (2, style), (3, ps), (4, family + " " + style), (6, ps)):
        name.setName(val, nid, 3, 1, 0x409)
        name.setName(val, nid, 1, 0, 0)
    f["OS/2"].usWeightClass = wght
    f.save(f"{OUT}/{fname}")
    print(fname, "ok")


build(f"{SRC}/Cinzel[wght].ttf", 600, "Cinzel", "SemiBold", "Cinzel-600.ttf", False)
build(f"{SRC}/Cinzel[wght].ttf", 700, "Cinzel", "Bold", "Cinzel-700.ttf", False)
build(f"{SRC}/EBGaramond[wght].ttf", 600, "Phono Garamond", "SemiBold", "PhonoGaramond-600.ttf", True)
build(f"{SRC}/EBGaramond-Italic[wght].ttf", 500, "Phono Garamond", "Medium Italic", "PhonoGaramond-500i.ttf", True)
build(f"{SRC}/EBGaramond[wght].ttf", 600, "Phono Figures", "SemiBold", "PhonoFigures-600.ttf", True, "0123456789:.")
