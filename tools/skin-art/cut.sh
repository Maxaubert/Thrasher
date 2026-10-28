#!/bin/sh
# Cuts a device skin's art out of its element page with headless Edge, at 1x and 2x,
# on a transparent background (docs/device-themes.md, "Recipe from the pilot").
#
#   sh tools/skin-art/cut.sh tape OUT_DIR
#
# reads tools/skin-art/<skin>.html and the piece list below, and writes OUT_DIR/<name>.png
# and OUT_DIR/<name>@2x.png. Copy the ones the skin uses to crates/thrasher-app/ui/skins/<skin>/.
# Edge runs headless with its own profile in OUT_DIR, so nothing shows and the user's
# browser is not touched. Needs Python with Pillow for the crop.
set -e
SKIN=$1
OUT=$2
[ -n "$SKIN" ] && [ -n "$OUT" ] || { echo "usage: cut.sh SKIN OUT_DIR"; exit 1; }
HERE=$(cd "$(dirname "$0")" && pwd -W 2>/dev/null || pwd)
EDGE="/c/Program Files (x86)/Microsoft/Edge/Application/msedge.exe"
mkdir -p "$OUT"

# name, element, css width, css height
case $SKIN in
tape) PIECES="cassette:cassette:354:126 reels:reels:352:44 vu-face:vu-face:184:106 vu-over:vu-over:184:106
  walnut:walnut-grain:153:4 alu:alu-lines:4:2 spine:spine-lines:4:5 grille:grille:7:4 dial:dial:52:52
  knob:knob:36:36 knob-sel:knob-sel:36:36 screw:screw:7:7 led:led:21:21" ;;
*) echo "no piece list for $SKIN"; exit 1 ;;
esac

for p in $PIECES; do
  IFS=: read -r name el w h <<EOF
$p
EOF
  for k in 1 2; do
    suf=""; [ $k = 2 ] && suf="@2x"
    raw="$OUT/raw-$name$suf.png"
    rm -f "$raw"
    "$EDGE" --headless=new --disable-gpu --hide-scrollbars --no-first-run \
      --user-data-dir="$OUT/edge-profile" --default-background-color=00000000 \
      --force-device-scale-factor=$k --screenshot="$raw" --window-size=$((w + 60)),$((h + 60)) \
      "file:///$HERE/$SKIN.html?el=$el&w=$w&h=$h" > /dev/null 2>&1 || true
    # Edge hands the shot over after it returns; wait for the file.
    n=0; while [ ! -s "$raw" ] && [ $n -lt 60 ]; do sleep 0.25; n=$((n + 1)); done; sleep 0.3
    python -c "import sys; from PIL import Image; Image.open(sys.argv[1]).convert('RGBA').crop((0, 0, int(sys.argv[3]), int(sys.argv[4]))).save(sys.argv[2], optimize=True)" \
      "$raw" "$OUT/$name$suf.png" $((w * k)) $((h * k))
    rm -f "$raw"
  done
done
ls -la "$OUT"
