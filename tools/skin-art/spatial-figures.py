# Spatial Figures: Inter Tight's tabular figures (the tnum glyphs mapped as the plain
# ones) at 300 and 400, only the characters of a timer or a length, under their own
# family name so no other text of the app can pick them up.
import sys
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from fontTools import subset

SRC, OUT = sys.argv[1], sys.argv[2]
CHARS = "0123456789:.-/ "
for wght, style in ((300, "Light"), (400, "Regular")):
    f = TTFont(SRC)
    f = instancer.instantiateVariableFont(f, {"wght": wght})
    gsub = f["GSUB"].table
    tnum = {}
    for fr in gsub.FeatureList.FeatureRecord:
        if fr.FeatureTag == "tnum":
            for li in fr.Feature.LookupListIndex:
                for st in gsub.LookupList.Lookup[li].SubTable:
                    tnum.update(getattr(st, "mapping", {}) or {})
    for table in f["cmap"].tables:
        if table.isUnicode():
            for cp, g in list(table.cmap.items()):
                if chr(cp) in CHARS and g in tnum:
                    table.cmap[cp] = tnum[g]
    opts = subset.Options()
    opts.layout_features = []
    opts.name_IDs = ["*"]
    opts.notdef_outline = True
    s = subset.Subsetter(opts)
    s.populate(unicodes=[ord(c) for c in CHARS])
    s.subset(f)
    name = f["name"]
    for rec in list(name.names):
        if rec.nameID in (16, 17, 21, 22, 25):
            name.removeNames(nameID=rec.nameID)
    full = "Spatial Figures " + style
    for nid, val in ((1, "Spatial Figures"), (2, style),
                     (3, "SpatialFigures-" + style), (4, full), (6, "SpatialFigures-" + style),
                     (16, "Spatial Figures"), (17, style)):
        name.setName(val, nid, 3, 1, 0x409)
        name.setName(val, nid, 1, 0, 0)
    f["OS/2"].usWeightClass = wght
    f["OS/2"].fsSelection = (f["OS/2"].fsSelection & ~0b1100001) | (0b1000000 if style == "Regular" else 0)
    f.save(f"{OUT}/SpatialFigures-{wght}.ttf")
    print(wght, "ok")
