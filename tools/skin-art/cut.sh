#!/bin/sh
# Cuts a device skin's art out of its element page with headless Edge, at 1x and 2x,
# on a transparent background (docs/device-themes.md, "Recipe from the pilot").
#
#   sh tools/skin-art/cut.sh tape OUT_DIR    (or x-comic)
#   sh tools/skin-art/cut.sh dash OUT_DIR    (the Car Dashboard, into ui/skins/dash/)
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
tape) PIECES="cassette:cassette:354:126 cassette-clip:cassette-clip:354:126 reels:reels:352:44 vu-face:vu-face:184:106 vu-over:vu-over:184:106
  walnut:walnut-grain:153:4 alu:alu-lines:4:2 spine:spine-lines:4:5 grille:grille:7:4 dial:dial:52:52
  knob:knob:36:36 knob-sel:knob-sel:36:36 screw:screw:7:7 led:led:21:21" ;;
phosphor) PIECES="vig-tl:vig-tl:200:200 vig-tr:vig-tr:200:200 vig-bl:vig-bl:200:200 vig-br:vig-br:200:200 halo:halo:30:30 dash:dash:6:1 vdash:vdash:1:6 dither:dither:4:4" ;;
# Desktop 95: two tiles (1x and 2x used) and pixel icons (only the 1x cut is used).
dv-os95) PIECES="desk:desk:4:4 checker:checker:2:2 floppy:floppy:16:15 balloon:balloon:16:13 widget:widget:16:14
  wave:wave:13:13 note:note:10:11 folder:folder:16:11 mic:mic:8:11 dot:dot:12:12 stop:stop:12:12 recpause:recpause:12:12
  led:led:4:4 gear:gear:10:10 min:min:6:8 close:close:8:7 play:play:4:7 pause:pause:5:7 taskplay:taskplay:5:9
  taskpause:taskpause:5:9 dots:dots:2:10 arrow:arrow:7:4 mag:mag:10:10 spk:spk:9:8 thumb:thumb:11:18 logo:logo:13:12
  check:check:10:10" ;;
x-comic) PIECES="logo:logo:214:52 picked:picked:60:26 rec-p:rec-p:128:128 rec-w:rec-w:200:200 stop-p:stop-p:256:128 stop-w:stop-w:400:200
  meter-off-p:meter-off-p:118:38 meter-on-p:meter-on-p:118:38 meter-off-w:meter-off-w:178:56 meter-on-w:meter-on-w:178:56
  dots:dots:7:7 dots-yellow:dots-yellow:6:6 dots-grey:dots-grey:5:5" ;;
# Spatial Glass: the rooms and the shadows are used at 1x only (they are
# blurred, so scaling them up costs nothing); the shadows are nine-slice images.
x-spatial) PIECES="room-p:room-p:200:310 room-w:room-w:500:310 shadow-pane:shadow-pane:196:204 shadow-near:shadow-near:220:226
  shadow-pill:shadow-pill:180:140 shadow-tab:shadow-tab:108:80 orb-s:orb-s:170:170 orb-l:orb-l:242:242
  rb-28:rb-28:48:48 rb-30:rb-30:50:50 rb-34:rb-34:54:54 logo:logo:40:40 thumb:thumb:27:27 ring-s:ring-s:110:110 ring-l:ring-l:220:220" ;;
dash) PIECES="gauge-p:gauge-p:188:188 gauge-w:gauge-w:270:270 wheel-p:wheel-p:200:192 wheel-w:wheel-w:400:244
  rec:rec:128:128 rec-on:rec-on:128:128 sbtn:sbtn:42:42 chip:chip:20:20 cover:cover:38:38 logo:logo:16:16
  lamp-ok:lamp-ok:22:22 lamp-rec:lamp-rec:22:22 lamp-pause:lamp-pause:22:22" ;;
# Pocket: pixel art and tiles, one css pixel per art pixel; only the 1x cut is used.
px-pocket) PIECES="mic:mic:9:11 mics:mics:5:7 gear:gear:9:9 min:min:7:7 close:close:7:7 imp:imp:9:9 ask:ask:9:7 wid:wid:9:8
  play:play:6:7 pause:pause:6:7 stop:stop:7:7 dots:dots:2:8 down:down:7:4 find:find:8:8 cur:cur:4:7 check:check:8:6
  sprite:sprite:16:24 key-12:key-12:12:12 key-11:key-11:11:11 key-16:key-16:16:16 dpad:dpad:19:19
  dith:dith:4:4 sep:sep:4:4 rule-row:rule-row:4:2 rule-col:rule-col:2:4 rule-sur:rule-sur:4:1 grid:grid:2:2" ;;
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
