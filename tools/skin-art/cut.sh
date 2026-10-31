#!/bin/sh
# Cuts a device skin's art out of its element page with headless Edge, at 1x and 2x,
# on a transparent background (docs/device-themes.md, "Recipe from the pilot").
#
#   sh tools/skin-art/cut.sh tape OUT_DIR    (or x-comic)
#   sh tools/skin-art/cut.sh dash OUT_DIR    (the Car Dashboard, into ui/skins/dash/)
#   sh tools/skin-art/cut.sh px-quest OUT_DIR (the Quest Menu, into ui/skins/px-quest/)
#   sh tools/skin-art/cut.sh dv-brick OUT_DIR (the Brick Phone, into ui/skins/dv-brick/)
#   sh tools/skin-art/cut.sh dv-synth OUT_DIR (the Pocket Synth, into ui/skins/dv-synth/)
#   sh tools/skin-art/cut.sh x-fighter OUT_DIR (Fighter Select, into ui/skins/x-fighter/)
#   sh tools/skin-art/cut.sh px-cozy OUT_DIR  (the Cozy Cabin, into ui/skins/px-cozy/)
#   sh tools/skin-art/cut.sh x-phono OUT_DIR  (the Phonograph, into ui/skins/x-phono/)
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
  check:check:10:10
  tick:tick:9:7 notepad:notepad:12:12 fopen:fopen:16:11 playl:playl:4:7 bigplay:bigplay:7:7 rew:rew:8:7 ff:ff:8:7
  up:up:7:4 x7:x7:7:5 backg:backg:11:9 pen:pen:11:10 bin:bin:11:11 copy:copy:11:11 globe:globe:11:10 spark:spark:11:9
  grip:grip:5:8 expand:expand:9:9 hour:hour:9:9 dl:dl:9:10 box:box:12:9 plus:plus:8:8 warn:warn:16:14 binbig:binbig:16:14
  book:book:13:13 ydown:ydown:8:8 yright:yright:8:7 dropin:dropin:20:16 ban:ban:24:160" ;;
x-comic) PIECES="logo:logo:214:52 picked:picked:60:26 rec-p:rec-p:128:128 rec-w:rec-w:200:200 stop-p:stop-p:256:128 stop-w:stop-w:400:200
  meter-off-p:meter-off-p:118:38 meter-on-p:meter-on-p:118:38 meter-off-w:meter-off-w:178:56 meter-on-w:meter-on-w:178:56
  dots:dots:7:7 dots-yellow:dots-yellow:6:6 dots-grey:dots-grey:5:5
  dots-red:dots-red:5:5 wrec:wrec:70:70 wstop:wstop:140:70 plus-l:plus-l:150:150 plus-s:plus-s:88:88
  logo-s:logo-s:112:34 title-settings:title-settings:113:36 title-models:title-models:96:36
  title-ask:title-ask:53:36 title-merge:title-merge:149:36 inuse:inuse:64:30" ;;
# Spatial Glass: the rooms and the shadows are used at 1x only (they are
# blurred, so scaling them up costs nothing); the shadows are nine-slice images.
x-spatial) PIECES="room-p:room-p:200:310 room-w:room-w:500:310 shadow-pane:shadow-pane:196:204 shadow-near:shadow-near:220:226
  shadow-pill:shadow-pill:180:140 shadow-tab:shadow-tab:108:80 orb-s:orb-s:170:170 orb-l:orb-l:242:242
  rb-28:rb-28:48:48 rb-30:rb-30:50:50 rb-34:rb-34:54:54 logo:logo:40:40 thumb:thumb:27:27 ring-s:ring-s:110:110 ring-l:ring-l:220:220
  widget-room:widget-room:240:124 orb-w:orb-w:142:142 ring-w:ring-w:96:96" ;;
dash) PIECES="gauge-p:gauge-p:188:188 gauge-w:gauge-w:270:270 wheel-p:wheel-p:200:192 wheel-w:wheel-w:400:244
  rec:rec:128:128 rec-on:rec-on:128:128 sbtn:sbtn:42:42 chip:chip:20:20 cover:cover:38:38 logo:logo:16:16
  lamp-ok:lamp-ok:22:22 lamp-rec:lamp-rec:22:22 lamp-pause:lamp-pause:22:22 gauge-c:gauge-c:188:188 ctile:ctile:20:20" ;;
# Pocket: pixel art and tiles, one css pixel per art pixel; only the 1x cut is used.
px-pocket) PIECES="mic:mic:9:11 mics:mics:5:7 gear:gear:9:9 min:min:7:7 close:close:7:7 imp:imp:9:9 ask:ask:9:7 wid:wid:9:8
  play:play:6:7 pause:pause:6:7 stop:stop:7:7 dots:dots:2:8 down:down:7:4 find:find:8:8 cur:cur:4:7 check:check:8:6
  sprite:sprite:16:24 key-12:key-12:12:12 key-11:key-11:11:11 key-16:key-16:16:16 dpad:dpad:19:19
  dith:dith:4:4 sep:sep:4:4 rule-row:rule-row:4:2 rule-col:rule-col:2:4 rule-sur:rule-sur:4:1 grid:grid:2:2
  back:back:4:7 pencil:pencil:8:8 trash:trash:8:9 lang:lang:7:7 spark:spark:7:7 copy:copy:7:7 up:up:7:7
  grip:grip:5:8 folder:folder:7:6 plus:plus:5:5 x:x:5:5 expand:expand:7:7 chev:chev:4:7 loop:loop:7:6 tick:tick:7:6
  drop:drop:16:17 key-8:key-8:8:8 dimL:dimL:4:4 dimD:dimD:4:4" ;;
# Quest Menu: pixel art (only the 1x cut is used, scaled up pixelated in Slint) and the
# wide command menu's upright title (1x and 2x); from back on, the other pages' pieces
# (ONLY="back chev ..." cuts just those).
px-quest) PIECES="gear:gear:7:7 min:min:7:7 close:close:7:7 play:play:5:5 pause:pause:5:5 dots:dots:1:5
  imp:imp:7:7 ask:ask:7:7 widget:widget:7:7 search:search:7:7 down:down:5:3 note:note:4:5 cpause:cpause:7:7
  cplay:cplay:7:7 check:check:5:5 hand:hand:8:6 mic:mic:10:14 rec-s:rec-s:19:19 rec-l:rec-l:33:33
  stop-s:stop-s:19:19 stop-l:stop-l:33:33 cmd:cmd:14:32
  back:back:4:7 chev:chev:4:7 pencil:pencil:7:7 trash:trash:7:7 rew:rew:7:7 ff:ff:7:7 lang:lang:7:7
  spark:spark:7:7 copy:copy:7:7 tick:tick:7:6 up:up:7:7 x:x:5:5 grip:grip:3:5 folder:folder:7:6 plus:plus:5:5
  box:box:7:7 expand:expand:7:7 tri:tri:3:5 upt:upt:5:3 stop:stop:5:5 tape:tape:14:11 owl:owl:12:13
  chest:chest:12:9 wrec:wrec:17:17 wstop:wstop:17:17" ;;
# Brick Phone: the stretching keys in three slices, the fixed keys whole, the rocker and
# its OK key, the antenna and the earpieces at 1x and 2x; the white icons for colorize;
# the LCD's pixel glyphs and tiles (x-, t-) are used at 1x only. From kp-off on: the
# other pages' pieces (the keys while off, the widget's keys, more icons and glyphs).
dv-brick) PIECES="kp-menu-l:kp-menu-l:30:42 kp-menu-m:kp-menu-m:1:42 kp-menu-r:kp-menu-r:30:42
  kp-find-l:kp-find-l:30:42 kp-find-m:kp-find-m:1:42 kp-find-r:kp-find-r:30:42
  kp-rec-l:kp-rec-l:30:42 kp-rec-m:kp-rec-m:1:42 kp-rec-r:kp-rec-r:30:42
  kp-stop-l:kp-stop-l:30:42 kp-stop-m:kp-stop-m:1:42 kp-stop-r:kp-stop-r:30:42
  kp-pad-l:kp-pad-l:30:33 kp-pad-m:kp-pad-m:1:33 kp-pad-r:kp-pad-r:30:33
  kw-pad:kw-pad:55:60 kw-rec:kw-rec:148:64 kw-stop:kw-stop:148:52 kw-soft:kw-soft:122:40 nub:nub:42:28
  nav-p:nav-p:136:86 nav-w:nav-w:124:122 ok-p:ok-p:42:42 ok-w:ok-w:50:50 antenna:antenna:34:40 ear-p:ear-p:62:7 ear-w:ear-w:72:10
  i-gear14:i-gear14:14:14 i-search14:i-search14:14:14 i-mic14:i-mic14:14:14 i-mic18:i-mic18:18:18 i-stop14:i-stop14:14:14
  i-up12:i-up12:12:12 i-down12:i-down12:12:12 i-play14:i-play14:14:14 i-play16:i-play16:16:16
  i-import13:i-import13:13:13 i-ask13:i-ask13:13:13 i-widget13:i-widget13:13:13
  i-import12:i-import12:12:12 i-ask12:i-ask12:12:12 i-widget12:i-widget12:12:12 i-min12:i-min12:12:12 i-close12:i-close12:12:12
  x-play:x-play:10:10 x-pause:x-pause:10:10 x-dots:x-dots:4:10 x-fold:x-fold:9:8 x-chev:x-chev:8:8 x-mag:x-mag:9:9
  i-pause14:i-pause14:14:14 i-pause16:i-pause16:16:16 i-pause18:i-pause18:18:18 i-play18:i-play18:18:18
  x-mic:x-mic:12:16 x-micsm:x-micsm:7:8 x-tri:x-tri:8:8 x-trid:x-trid:8:8 t-dot:t-dot:2:1 t-dash:t-dash:6:1 t-dashv:t-dashv:1:6
  t-rule:t-rule:1:3 t-grid:t-grid:3:3 t-micdots:t-micdots:4:4
  kp-off-l:kp-off-l:30:42 kp-off-m:kp-off-m:1:42 kp-off-r:kp-off-r:30:42 kw-off:kw-off:148:64
  kg-rec:kg-rec:78:46 kg-stop:kg-stop:78:40 kg-stopoff:kg-stopoff:78:40
  i-send14:i-send14:14:14 i-send18:i-send18:18:18 i-back14:i-back14:14:14 i-check14:i-check14:14:14 i-check18:i-check18:18:18
  i-merge14:i-merge14:14:14 i-merge18:i-merge18:18:18 i-ask14:i-ask14:14:14 i-ask18:i-ask18:18:18
  i-expand12:i-expand12:12:12 i-left12:i-left12:12:12 i-right12:i-right12:12:12
  x-up:x-up:8:4 x-dn:x-dn:8:4 x-lf:x-lf:4:7 x-rt:x-rt:4:7 x-check:x-check:9:6 x-x:x-x:7:7 x-grip:x-grip:5:8
  x-lang:x-lang:9:8 x-spark:x-spark:9:7 x-copy:x-copy:8:8 x-pencil:x-pencil:7:7 x-trash:x-trash:7:7 x-folder:x-folder:9:6
  x-plus:x-plus:7:7 x-box:x-box:7:7 x-loop:x-loop:7:6 x-speak:x-speak:8:8 x-bigimp:x-bigimp:16:16 x-hour:x-hour:7:7
  t-dither:t-dither:2:2" ;;
# Answering Machine: parts of variable width in three slices (-l, -m, -r); the grille
# of the wide window is cut short and built up from its rows (see Grille in dv-answer.slint).
dv-answer) PIECES="disp-p-l:disp-p-l:14:100 disp-p-m:disp-p-m:2:100 disp-p-r:disp-p-r:14:100
  disp-w-l:disp-w-l:14:132 disp-w-m:disp-w-m:2:132 disp-w-r:disp-w-r:14:132
  mw-p-l:mw-p-l:10:46 mw-p-m:mw-p-m:2:46 mw-p-r:mw-p-r:10:46 mw-w-l:mw-w-l:10:68 mw-w-m:mw-w-m:2:68 mw-w-r:mw-w-r:10:68
  rec-w-l:rec-w-l:24:90 rec-w-m:rec-w-m:2:90 rec-w-r:rec-w-r:24:90 key-w-l:key-w-l:24:90 key-w-m:key-w-m:2:90 key-w-r:key-w-r:24:90
  pick-p-l:pick-p-l:10:36 pick-p-m:pick-p-m:2:36 pick-p-r:pick-p-r:10:36 pick-w-l:pick-w-l:10:40 pick-w-m:pick-w-m:2:40 pick-w-r:pick-w-r:10:40
  srch-p-l:srch-p-l:17:34 srch-p-m:srch-p-m:2:34 srch-p-r:srch-p-r:17:34 srch-w-l:srch-w-l:19:38 srch-w-m:srch-w-m:2:38 srch-w-r:srch-w-r:19:38
  strip-l:strip-l:8:28 strip-m:strip-m:2:28 strip-r:strip-r:8:28 track-l:track-l:4:8 track-m:track-m:2:8 track-r:track-r:4:8
  rec-p:rec-p:120:104 tk:tk:62:56 play-p:play-p:84:70 play-w:play-w:98:70 nub:nub:36:34
  pb-s:pb-s:34:34 pb-s-red:pb-s-red:34:34 pb-l:pb-l:42:42 pb-l-red:pb-l-red:42:42 thumb:thumb:15:20
  grille-p:grille-p:94:80 grille-w:grille-w:256:39 keys:keys:366:98 led-on:led-on:16:16 led-off:led-off:16:16
  dot-ready:dot-ready:15:15 dot-rec:dot-rec:15:15 dot-pause:dot-pause:15:15
  mc-p:mc-p:148:84 mc-w:mc-w:214:110 reel-p:reel-p:128:16 reel-w:reel-w:176:22" ;;
# Pocket Synth: the screens in three slices (-l, -m, -r), the fixed keys whole with their
# shadows, the pads' shadows for a nine-slice, the knobs, LEDs and round buttons; the
# grille is one band of dot rows and the brushed tile one period (both repeated in Slint).
dv-synth) PIECES="oled-p-l:oled-p-l:26:116 oled-p-m:oled-p-m:2:116 oled-p-r:oled-p-r:26:116
  oled-w-l:oled-w-l:26:196 oled-w-m:oled-w-m:2:196 oled-w-r:oled-w-r:26:196
  lcd30-l:lcd30-l:16:36 lcd30-m:lcd30-m:2:36 lcd30-r:lcd30-r:16:36 lcd24-l:lcd24-l:16:30 lcd24-m:lcd24-m:2:30 lcd24-r:lcd24-r:16:30
  fk-p:fk-p:76:68 fk-p-or:fk-p-or:100:68 fk-p-lit:fk-p-lit:88:80 fk-w:fk-w:96:82 fk-w-or:fk-w-or:120:82 fk-w-lit:fk-w-lit:108:94
  rk:rk:52:51 rk-lit:rk-lit:64:63 dk-p:dk-p:62:60 dk-w:dk-w:70:64 pad-shade:pad-shade:64:64 pad-glow:pad-glow:80:80
  led-off:led-off:18:18 led-on:led-on:18:18 led-w:led-w:18:18 step-off:led-off:17:17 step-on:led-on:17:17
  tbtn:tbtn:26:26 dots:dots:24:24 screw:screw:9:9 knob-44:knob-44:52:52 knob-62:knob-62:70:70 knob-30:knob-30:38:38
  knob-32:knob-32:40:40 grille:grille:277:10 brush:brush:3:1" ;;
# Fighter Select: the record diamond (FIGHT, and STOP while recording) at both sizes,
# the floor at 1x and 2x; the soft glows (stage, selbg) are used at 1x only, stretched.
x-fighter) PIECES="rec-p:rec-p:96:96 stop-p:stop-p:96:96 rec-w:rec-w:170:170 stop-w:stop-w:170:170
  floor:floor:480:120 stage:stage:200:280 selbg:selbg:240:200" ;;
# Cozy Cabin: pixel art one css pixel per art pixel, the wood and parchment tiles at three
# times, the window scene and the lit lantern as frame strips; only the 1x cut is used.
px-cozy) PIECES="gear:gear:7:7 min:min:7:6 close:close:7:5 play:play:5:9 pause:pause:5:9 down:down:5:3
  search:search:7:7 imp:imp:7:7 ask:ask:8:7 widget:widget:8:6 tpause:tpause:5:7 tplay:tplay:5:7 dots:dots:1:5
  mic:mic:5:7 micsm:micsm:3:4 scroll:scroll:5:5 check:check:5:5 stop:stop:5:5 flame:flame:5:6 plant:plant:7:7
  mug:mug:7:7 candle:candle:5:7 books:books:9:6 jar:jar:6:7 tape:tape:8:6 lantern:lantern:18:23
  lantern-hot:lantern-hot:18:23 lantern-lit:lantern-lit:72:23 glow-p:glow-p:38:38 glow-w:glow-w:72:72
  wood:wood:192:144 wood-d:wood-d:192:144 parch:parch:96:96 scene-p:scene-p:1296:36 scene-w:scene-w:1872:66" ;;
# Pinball: the backglass and playfield pictures (stretched to the window in Slint; the
# playfields and the display's nine-slice glow are used at 1x only, they are soft), the Monoton words, the display case,
# the shooter lane, the bumpers, lamps and inserts, the flipper button, the ball and the
# wide flippers.
x-pinball) PIECES="brand:brand:139:40 title-p:title-p:202:52 title-w:title-w:334:78 bg-p:bg-p:520:146 bg-w:bg-w:400:212
  dmd-glow:dmd-glow:92:92 spk:spk:26:78 pf-p:pf-p:520:100 pf-w:pf-w:395:316
  arr-p:arr-p:24:30 arr-w1:arr-w1:26:34 arr-w2:arr-w2:22:28 arr-w3:arr-w3:20:25 rod-p:rod-p:18:22 rod-w:rod-w:18:120
  knob-p:knob-p:108:108 knob-w:knob-w:118:118
  bk-red-p:bk-red-p:80:80 bk-yel-p:bk-yel-p:80:80 bk-blu-p:bk-blu-p:80:80 bk-red-w:bk-red-w:94:94 bk-yel-w:bk-yel-w:94:94 bk-blu-w:bk-blu-w:94:94
  lamp-off-p:lamp-off-p:27:27 lamp-o-p:lamp-o-p:27:27 lamp-y-p:lamp-y-p:27:27 lamp-r-p:lamp-r-p:27:27
  lamp-off-w:lamp-off-w:28:28 lamp-o-w:lamp-o-w:28:28 lamp-y-w:lamp-y-w:28:28 lamp-r-w:lamp-r-w:28:28
  ins-0-p:ins-0-p:42:34 ins-1-p:ins-1-p:42:34 ins-2-p:ins-2-p:42:34 ins-3-p:ins-3-p:42:34 ins-s-p:ins-s-p:42:34
  ins-0-w:ins-0-w:50:40 ins-1-w:ins-1-w:50:40 ins-2-w:ins-2-w:50:40 ins-3-w:ins-3-w:50:40 ins-s-w:ins-s-w:50:40
  flip-p:flip-p:72:72 flip-w:flip-w:78:78 ball:ball:19:19 flippers:flippers:290:132" ;;
# Phonograph: parts of variable width in three slices (-l, -m, -r); the textures and the
# grooves are tiles (only the 1x cut is used); hub and gov are frame strips.
x-phono) PIECES="wood:wood:100:4 woodd:woodd:31:4 groove:groove:3:4 groovec:groovec:2:4
  odo-p-l:odo-p-l:10:29 odo-p-m:odo-p-m:2:29 odo-p-r:odo-p-r:10:29 odo-w-l:odo-w-l:10:37 odo-w-m:odo-w-m:2:37 odo-w-r:odo-w-r:10:37
  key-p-l:key-p-l:16:48 key-p-m:key-p-m:2:48 key-p-r:key-p-r:16:48 key-w-l:key-w-l:16:58 key-w-m:key-w-m:2:58 key-w-r:key-w-r:16:58
  hold-p-l:hold-p-l:11:34 hold-p-m:hold-p-m:2:34 hold-p-r:hold-p-r:11:34 hold-w-l:hold-w-l:11:52 hold-w-m:hold-w-m:2:52 hold-w-r:hold-w-r:11:52
  srch-p-l:srch-p-l:7:30 srch-p-m:srch-p-m:2:30 srch-p-r:srch-p-r:7:30 srch-w-l:srch-w-l:7:48 srch-w-m:srch-w-m:2:48 srch-w-r:srch-w-r:7:48
  tw-p-l:tw-p-l:9:27 tw-p-m:tw-p-m:2:27 tw-p-r:tw-p-r:9:27 tw-w-l:tw-w-l:9:29 tw-w-m:tw-w-m:2:29 tw-w-r:tw-w-r:9:29
  dev-p-l:dev-p-l:7:26 dev-p-m:dev-p-m:2:26 dev-p-r:dev-p-r:7:26 dev-w-l:dev-w-l:7:30 dev-w-m:dev-w-m:2:30 dev-w-r:dev-w-r:7:30
  rail-l:rail-l:8:20 rail-m:rail-m:2:20 rail-r:rail-r:8:20
  cyl-p-l:cyl-p-l:4:36 cyl-p-r:cyl-p-r:4:36 cyl-w-l:cyl-w-l:4:58 cyl-w-r:cyl-w-r:4:58 tin-p-r:tin-p-r:4:28 tin-w-r:tin-w-r:4:50
  plate:plate:214:30 plate-rec:plate-rec:96:22 plate-stop:plate-stop:96:22 gear:gear:26:26 screw-min:screw-min:26:26 screw-close:screw-close:26:26
  i-crank-17:i-crank-17:17:17 i-trumpet-17:i-trumpet-17:17:17 i-watch-17:i-watch-17:17:17
  i-crank-20:i-crank-20:20:20 i-trumpet-20:i-trumpet-20:20:20 i-watch-20:i-watch-20:20:20
  jewel-g:jewel-g:21:21 jewel-r:jewel-r:21:21 jewel-a:jewel-a:21:21 cell-p:cell-p:10:17 cell-w:cell-w:15:25
  cap-p:cap-p:12:29 cap-w:cap-w:12:45 hub-p:hub-p:180:30 hub-w:hub-w:264:44 repro-p:repro-p:23:31 repro-w:repro-w:34:42
  gov-p:gov-p:624:80 gov-w:gov-w:1008:130 bell-p:bell-p:148:148 bell-w:bell-w:218:218 seal-p:seal-p:55:55 seal-w:seal-w:80:80
  lid-p:lid-p:32:32 lid-p-sel:lid-p-sel:32:32 lid-w:lid-w:46:46 lid-w-sel:lid-w-sel:46:46 dots:dots:4:15
  stamp-p:stamp-p:70:14 stamp-w:stamp-w:84:16 knob-p:knob-p:42:42 knob-w:knob-w:46:46 car:car:18:22" ;;
*) echo "no piece list for $SKIN"; exit 1 ;;
esac

# ONLY="a b" cuts just those pieces.
for p in $PIECES; do
  IFS=: read -r name el w h <<EOF
$p
EOF
  if [ -n "$ONLY" ]; then case " $ONLY " in *" $name "*) ;; *) continue ;; esac; fi
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
