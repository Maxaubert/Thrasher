# Builds "Synth Figures", the Pocket Synth timer: Doto (a dot matrix face, OFL, no
# Reserved Font Name) as a static Bold instance (wght 700, ROND 0, as the mockup asks),
# with only the timer's characters, under its own family name so no other text can pick
# it up. Doto's digits are all 0.6 em wide, so the timer does not jump. The Pocket Synth's
# other text is Inter Tight and JetBrains Mono, already in the app.
# SRC is Doto[ROND,wght].ttf from github.com/google/fonts (ofl/doto). Needs fontTools.
#   python tools/skin-art/dv-synth-fonts.py SRC OUT_DIR
import sys, os
from fontTools.ttLib import TTFont
from fontTools import subset
from fontTools.varLib import instancer

SRC, OUT = sys.argv[1], sys.argv[2]
os.makedirs(OUT, exist_ok=True)
font = TTFont(SRC)
font = instancer.instantiateVariableFont(font, {"wght": 700, "ROND": 0})
opts = subset.Options()
opts.layout_features = []
opts.name_IDs = ["*"]
opts.notdef_outline = True
# Slint does not use TrueType hinting; dropping it keeps the file small.
opts.hinting = False
s = subset.Subsetter(opts)
s.populate(text="0123456789:. ")
s.subset(font)
name = font["name"]
for rec in list(name.names):
    if rec.nameID in (16, 17, 21, 22, 25):
        name.removeNames(nameID=rec.nameID)
for nid, val in ((1, "Synth Figures"), (2, "Bold"), (3, "SynthFigures-Bold;thrasher"),
                 (4, "Synth Figures Bold"), (6, "SynthFigures-Bold")):
    name.setName(val, nid, 3, 1, 0x409)
    name.setName(val, nid, 1, 0, 0)
font["OS/2"].usWeightClass = 700
path = os.path.join(OUT, "SynthFigures-700.ttf")
font.save(path)
print("SynthFigures-700.ttf", os.path.getsize(path))
