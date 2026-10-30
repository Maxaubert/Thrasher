# Device themes: spec and implementation plan

Status: approved by Max 2026-10-28 (full device look on every page, Tape Deck pilot). Owner: Max Aubert.

## 1. What and why

Today a theme changes colours plus one effect. A device theme goes further: the whole app
becomes an object or a game world (a cassette deck, a handheld, a pinball cabinet). The
layout, the icons and the controls change, but the features stay the same. No option is
added or removed.

Max locked in 15 of them on 2026-10-28, picked from 34 browser mockups. The mockups are in
`docs/mockups/device-themes/` (one HTML page and one screenshot each, portrait and wide
side by side). They are the design source.

| # | Theme | File | Kind |
|---|---|---|---|
| 1 | Tape Deck | `tape` | Retro hardware |
| 2 | Phosphor | `phosphor` | Terminal |
| 3 | Pocket | `px-pocket` | Pixel handheld |
| 4 | Quest Menu | `px-quest` | Pixel RPG menu |
| 5 | Cozy Cabin | `px-cozy` | Pixel world |
| 6 | Answering Machine | `dv-answer` | Retro hardware |
| 7 | Desktop 95 | `dv-os95` | Retro computer |
| 8 | Pocket Synth | `dv-synth` | Modern hardware |
| 9 | Brick Phone | `dv-brick` | Retro hardware |
| 10 | Phonograph | `x-phono` | Antique |
| 11 | Pinball | `x-pinball` | Game |
| 12 | Fighter Select | `x-fighter` | Game |
| 13 | Comic Book | `x-comic` | Print |
| 14 | Car Dashboard | `x-dash` | Modern |
| 15 | Spatial Glass | `x-spatial` | Modern |

The 8 current themes (Ember to Cyberpunk) stay as they are.

## 2. Scope

- **Main page:** built as in the mockup, in portrait and wide: title bar, recorder, toolbar,
  clip rows, footer player.
- **Other pages** (clip page, settings, merge, dialogs, menus, widget): the full device
  look too (Max, 2026-10-28). Each theme gets mockups of these pages first, in the browser,
  for Max to pick from, before they are built.
- **Behaviour is shared.** Clicks, keys, tooltips, accessibility names, scrolling, drag,
  and the recording and playback state all come from one place. A theme only decides how
  things look and where they sit.
- **Same controls everywhere.** A device control that stands in for a button (for example
  the A key on Pocket = record) has the same tooltip and accessible name as the normal
  button.

## 3. Architecture

### Skins

- `Theme` gets a `skin` next to the palette: `classic` for the 8 current themes, or one of
  the 15 device skins.
- The main page today is one `AppWindow` block in `app.slint` (5,500 lines). It is split
  into five regions: `TitleBar`, `Recorder`, `Toolbar`, `ClipRow` and `Player`. Each takes
  the same inputs and callbacks, collected in shared structs (`RecState`, `ClipRowData`,
  `PlayerState`).
- Each skin lives in its own file `ui/skins/<name>.slint` with its own version of the
  regions, plus a `layout` that says where the regions go. Examples: toolbar at the bottom
  (Phosphor), icons as a side rail, recorder at the bottom, or toolbar buttons moved into
  the recorder (Tape Deck keys).
- `AppWindow` picks the skin's version with `if Theme.skin == ...`, so only the active skin
  is built.
- First step: move the classic look into `ui/skins/classic.slint` with **no visible
  change**. The headless screenshots before and after must match.

**Built in step 1 (2026-10-28):**
- `ui/common.slint` holds every shared struct, global and widget (Slint forbids circular
  imports, so a skin file cannot import from `app.slint`). `app.slint` keeps `Tray` and
  `AppWindow` and re-exports what Rust uses.
- The `Main` global holds the main page's state and actions. AppWindow's properties link
  to it (`<=>`), so Rust sets them as before. The main page's callbacks are declared on
  `Main` and Rust handles them with `ui.global::<Main>().on_...`: Slint 1.18 still allows
  aliasing a global's callback from a component, but warns that it is deprecated.
- `ui/skins/classic.slint` exports `MainPage` (imported as `ClassicMain`) with the
  regions `Title`, `Recorder`, `Toolbar`, `Row` and `Player`. MainPage takes the size
  classes (`narrow`, `wide`, `tall`) and four window actions (`go`, `ask-rename`,
  `ask-delete`, `refocus`), since a global cannot reach AppWindow's functions or focus.
- `Theme.skin` comes from `Theme.skins`, one per palette (0 classic).

**Built in step 2, the Tape Deck pilot (2026-10-28):**
- `ui/skins/tape.slint` (skin 1, theme "Tape Deck", palette 9). Shared pieces moved to
  `common.slint` so a skin reuses the behaviour and draws its own look: `ProjectMenu`
  (the project popup), `ClipMenu` (a row's menu, `show-at(x, y)`), `WaveSeek` (the
  waveform's seek and pick logic), style properties on `TextField` (`radius`, `fill`,
  `rest-edge`, `ink`, `hint`, `font-size`, `family`, `side`), `Main.row-h` (each MainPage
  sets its row height on `init`; keyboard scrolling and `clamp-list` use it) and
  `ClipRow.when` (the date alone, for skins that show "transcribed" their own way).
- Rust: `EFFECT_FPS` gets one entry per theme; `RECORDING_FPS` runs the Fx clock only
  while recording, not paused (Tape Deck 12), so a still deck costs nothing.

**Built in step 3, Tape Deck on every other page (2026-10-29):** mockups in
`docs/mockups/device-themes/tape-pages/` (Max approved them as they are).
- `ui/skins/tape-parts.slint`: the deck's parts, shared by the main page and the other
  pages (walnut, aluminium, screws, wells, piano keys, the cassette, the counter drums,
  the play knob, the title bar with an optional Eject key).
- `ui/skins/tape-pages.slint`: `ClipPage`, `SettingsPage`, `ModelsPage`, `MergePage`,
  `AskPage`, `Widget`, `Dialog`, `Speaking` and `Drop`. `app.slint` picks them with
  `Theme.skin == 1`, the classic blocks with `Theme.skin == 0`.
- The other pages' state is in a new `Pages` global (as `Main` is for the main page);
  AppWindow links its properties to it, so Rust is unchanged. The actions stay
  AppWindow's callbacks and functions; a skin page gets them as callbacks. The question
  words (`dialog-title`, `dialog-note`, `dialog-ok` and so on) are worked out once in
  AppWindow for both looks. The trim handles of the clip waveform are `TrimTouch`, used
  by both looks.
- The shared widgets draw the deck look themselves when `Theme.tape` is set: `Btn` and
  `IconBtn` (piano keys, `red` for Delete and Merge), `Toggle` (slide switch with a lamp),
  `Select` (black Dymo strip), `ShortcutBox` (key caps in a well), `MenuRow`, `ExportItem`
  and `MenuCard` (the cream J-card of every menu), `ProjectMenu` (an aluminium drawer of
  Dymo tapes), `ExportButton`, `OrderList` (tape spines with grip, place and knob),
  `TranscriptCard` (the ruled insert card; its rules follow the text's line height and
  scroll with it) and `Tip`. So the menus, the transcript and the dropdowns keep one
  behaviour for every look.
- The search results on the main page are index strips (`Hit` in `tape.slint`).
- Rust: `ModelRow.group` (the catalog group, for the spine colours); the motion clock
  also runs while a clip plays on its page (the cassette's reels turn); the editor's
  pause tags are a darker amber on the cream card.
- Art: one new piece, `cassette-clip` (the cassette with a writing line for the clip's
  name), 22 KB at 1x and 2x. `cut.sh` reproduces all 28 PNGs.

**Built in step 5, Desktop 95 main page (2026-10-29):** `ui/skins/dv-os95.slint` (skin 3,
theme "Desktop 95", index 10 in `Theme.names`), art in `ui/skins/dv-os95/`, fonts Pixel 95
and VT323 (the same `VT323-400.ttf` as Phosphor: its Latin subset covers the ASCII this
skin needs, with the same outlines and metrics).
- `Theme.pages` says how the other pages are built: 1 for Tape Deck, else 0, the classic
  pages in the theme's palette. A skin whose other pages are not built yet keeps them
  classic this way; `app.slint` picks the main page by `Theme.skin` and the rest by
  `Theme.pages`. Desktop 95's palette is grey and navy for those pages.
- The scope and the LED level follow `Main.live` like the classic meter, so the skin needs
  no clock (`EFFECT_FPS` and `RECORDING_FPS` 0). Nothing else moves.
- `Main.transcribed-count` (set in `refresh_clips`) gives the wide status bar its
  "N transcribed". The Type column shows from a 560px file window, so the mockup's 1000px
  wide window has Type and Wave.

**Built in step 5, Comic Book main page (2026-10-29):**
- `ui/skins/x-comic.slint` (skin 4, theme "Comic Book") and `ui/skins/x-comic-parts.slint`
  (the `Comic` inks, `Box`, `Glyph`, `InkBtn`, `Caption`, `Tag`, `Bubble`, `Burst`,
  `Meter`, `InkBars`, `PlayDot`), ready for the other pages. Fonts Bangers 400 and Comic
  Neue 700 (OFL, Latin subset, 92 KB); Bangers' 5 has a flat top and a sharp corner, so it
  does not read as S. Art: 23 PNGs, 459 KB (`sh tools/skin-art/cut.sh x-comic OUT`).
- The theme is index 11 in `Theme.names`, `Theme.skins` 4. Its other pages are the classic
  ones in its light palette (`Theme.pages` 0, like every skin without its own pages).
- While recording, Pause is a small caption under the burst (the mockup's unused
  `burstlbl` style); Import stays in the toolbar, disabled, as in the classic look. The
  STOP burst boils between two inked frames at `RECORDING_FPS` 6.
- Lessons:
  - **Tiles repeat at one image pixel per logical pixel** (measured: the 14 px 2x dot
    tile repeated every 14.2 logical px at 2.25x). A tile whose period matters (halftone
    dots) uses the 1x image; the pilot's "2x tile when `1phx < 0.7px`" doubles the period.
  - **`Path` stroke widths are pixels**, not viewbox units: icons copied from an SVG
    mockup set `stroke-width: width * units / 24` (`Glyph`), or they turn into blobs.
  - **Rotated or outlined text is art.** The masthead, the burst words and the meter are
    PNGs. The meter is two words (dark and lit); the lit one is clipped with
    `source-clip-width` at the gap after the last lit letter, the gaps worked out from
    Bangers' advance widths (letter i is `base + i * step` px, 1 px apart).
  - **Ellipses are `Path` arcs in a pixel viewbox** (`viewbox-width: width / 1px`), so the
    outline keeps its width at every size. The timer bubble's width comes from the number
    of characters, not their shapes, so it holds still while the tenths run.
  - **Drawing over a later sibling** (the search bubble's thought circles over the clips
    panel): the later sibling draws it, with the position handed over (`search-x`).
  - **Padding inside the list:** take it out of `Main.list-height`, or keyboard scrolling
    leaves the last row half hidden.
  - **A toolbar shares its room** (review, 2026-10-29): a long project name plus All
    projects pushed Widget out of the window. The project caption takes at most what
    the rest leaves (`Toolbar.avail`, worked out by the page, since the toolbar's own
    width loops back through the layout) and elides; the wide toolbar drops the button
    words when they do not fit. Check skins with a long project name and a query.
  - **Same row facts as the classic page:** a clip with the computer's sound says
    "+ system"; the burst and Pause give the keyboard back (`refocus`) after a click.
  - **Checks:** a posted `WM_CHAR` does not type in winit (post `WM_KEYDOWN` with the
    virtual key); a posted wheel needs a mouse move first; tooltips did not show in
    PrintWindow shots for any theme. Never press Import in a headless check: it opens
    the system file dialog on screen.

**Built in step 5, Car Dashboard main page (2026-10-29):** `ui/skins/x-dash.slint` (skin 6,
theme "Car Dashboard", index 13 in `Theme.names`), art in `ui/skins/dash/` (26 PNGs, 0.36 MB)
from `tools/skin-art/dash.html` (`sh tools/skin-art/cut.sh dash OUT`).
- The recorder is the driver display: an arc meter (-48 to 0 dB) with the timer in the middle,
  and a steering wheel whose hub is Record. The spoke buttons are Import (Pause while
  recording), Ask, the widget and Settings (the gear left the title bar, as in the mockup).
  The project picker is the screen's source chip; the clips are a playlist on a glass screen.
- Its other pages, the widget and the overlays are the classic ones in its palette
  (`Theme.pages` 0, from Desktop 95); the main page is picked by `Theme.skin`.
- Nothing moves but the meter, so its `RECORDING_FPS` is 0.
- Fonts: Manrope 500, 700 and 800 (a 600 request takes 700) and JetBrains Mono 300 for the
  timer. No existing text asks for less than 400, so the new light weight changes nothing
  (Ember and Tape Deck shots match pixel for pixel).

**Lessons from the Car Dashboard:**
- **Arcs:** a `Path` with an SVG `A` command and a `@linear-gradient` stroke renders fine in
  the software renderer. Work out the end point with `cos`/`sin` on the 200 unit box and set
  the large-arc flag when the sweep passes 180 degrees. `stroke-width` is in px, not
  viewbox units, so scale it by hand.
- **No `animate` on a meter that spans a big area:** the level already comes 30 times a
  second, and the animation redrew the whole gauge every frame in between. Taking it out
  cut recording CPU from 24.5% to 9.4% at 520x700 (36.9% to 11.4% wide).
- **A rounded gradient highlight:** gradients ignore `border-radius`, so the selected row is
  a solid left cap (left corners rounded), a square gradient in the middle and a solid right
  cap, with a rounded border on top.
- **Tests:** a posted `WM_CHAR` does not type into a Slint field; post `WM_KEYDOWN` with the
  virtual key (`win.ps1 -Action vk`). Run the test copy under its own process name (here
  `thrasher-dash.exe`) with its own scratch `THRASHER_DIR` and `THRASHER_SETTINGS`, since the
  shared scripts kill every `thrasher-test` and parallel agents would stop each other's.

**Budgets measured on the Car Dashboard** (test copy, 2.25x display, 10 s samples):
- CPU idle 0.0 to 0.3% (Ember 0.2 to 0.3%); recording 9.4% at 520x700 (Ember 20.8%) and
  11.4% at 1100x700 (Ember 26.4%).
- Working set idle 46.1 vs Ember 41.3 MB (+4.8); wide 54.2 vs 49.1 MB (+5.1).
- Exe +2.11 MB (44.83 to 46.95 MB): fonts 0.16 MB, art 0.36 MB, the rest generated code.

**Built in step 5, Pocket main page (2026-10-29):** `ui/skins/px-pocket.slint` (skin 7, theme
"Pocket", index 14 in `Theme.names`), art in `ui/skins/px-pocket/` (27 PNGs at 1x, 3 KB) from
`tools/skin-art/px-pocket.html` (`sh tools/skin-art/cut.sh px-pocket OUT`, only the 1x cut is
used). Its other pages are the classic ones in a green LCD palette (`Theme.pages` 0).
- Settings gallery: its tile is the Pocket line in `Gallery.devices` (`ui/gallery.slint`), band #8bac0f.
- The LCD holds the recorder, the project and search row, the list and the player; the bezel
  holds A (Record, Stop while recording), B (Import, Pause or Resume while recording), the Ask
  and Widget pills and the D pad. Settings, minimise and close are keys on the top edge.
- The D pad does what the arrow keys do: its upper left half steps to the clip above, its
  lower right half to the one below, and the list scrolls to keep it in view. For screen
  readers it is two buttons, "Previous clip" and "Next clip".
- Only the status dot moves on the clock (`RECORDING_FPS` 4, blinking while recording).
- Fonts: Jersey 10 (the LCD type and the timer) subset to Latin, 20 KB; its 5 has a flat top,
  a square corner and no hook, and is narrower than its S, so it is kept. Silkscreen (the
  small caps) differs from its S by two pixels at 8px, so `tools/skin-art/px-pocket-fonts.py`
  redraws its 5 on its own 125 unit pixel grid as a square 5 and renames it "Pocket Small"
  (OFL, no Reserved Font Name), 14 KB.

**Lessons from Pocket:**
- **Tiles need a size:** an `Image` with tiling but no `width` and `height` draws one copy at
  its natural size in the middle (a dot under each row), and `image-fit: fill` with tiling
  does the same. Give a tile `width: 100%; height: 100%` (`Tile`) and no `image-fit`. Tiles
  then repeat at one image pixel per logical pixel, as the Comic Book measured: a 4x4 checker
  gives the mockup's 2px dither.
- **Whole device pixels for pixel art:** `Px` sizes an art pixel as
  `max(1, round(s * 1px / 1phx)) * 1phx`, so a 2x icon is 4 or 5 device pixels per art pixel
  at 2.25x, never a blurred 4.5. The mockup's 1.5x title icons come out at 3 device pixels.
- **Stepped corners and pixel outlines are rectangles:** a 4px or 8px step is two or three
  overlapping rectangles, the mockup's inset bevels are bands clipped to the same steps, and
  the `pxo` outline (a box shadow on four sides) is four 2px strips without the corners.
- **CSS line heights:** a Text with `vertical-alignment: center` and the mockup's line height
  as its height centres the font's line box as CSS does, so names and the timer land where
  the mockup has them. The tenths sit on the timer's baseline at `base - size/2 * ascent`
  (Jersey's ascent 0.8036 em, line 1.0714 em).
- **A centred timer with proportional digits:** the wide timer is placed by the width of
  "00:00" (or "0:00:00") in an invisible Text, so it does not wander while it runs.
- **The LCD ghost** is one copy of the timer one shade lighter, 2px (4px wide) down and
  right; recording stays at 7.7% CPU with it.
- **Silent text in a Text component:** `accessible-role: none` on an instance of a component
  that inherits Text (`Small`, `Big`) still shows up in UI Automation as a Custom element
  named by its text, so shadow copies were read twice. The component sets
  `accessible-label: self.accessible-role == AccessibleRole.none ? "" : self.text` (review,
  2026-10-29). A plain `Text { accessible-role: none; }` is silent already.
- **Checks:** the shared scripts' shots are half size; set `FULL=1` in this skin's copy of
  `win.ps1` for device pixel crops. A project folder made by hand did not show in the picker;
  make test projects through New project (typed as posted key-downs) and delete them after.

**Budgets measured on Pocket** (test copy, 2.25x display, 10 s samples, 2026-10-29):
- CPU idle 0.3% at 520x700 and 1100x700 (Ember 0.0%); recording 7.7% and 7.8% (Ember 21.1%
  and 24.1%).
- Working set idle 42.4 vs Ember 41.3 MB (+1.1); wide 49.8 vs 48.5 MB (+1.3); recording 45.1
  vs 44.3 MB and 53.1 vs 52.0 MB.
- Exe +1.89 MB (55.54 to 57.43 MB): fonts 34 KB, art 3 KB, the rest generated code. Ember,
  Tape Deck and Phosphor main pages at 520x700 and 1100x700 match before and after.

**Built in step 5, Answering Machine main page (2026-10-29):** `ui/skins/dv-answer.slint`
(skin 10, theme "Answering Machine", index 17 in `Theme.names`, after Pocket, Quest Menu and
Brick Phone), art in `ui/skins/dv-answer/`
(58 pieces at 1x and 2x, 0.24 MB) from `tools/skin-art/dv-answer.html`
(`sh tools/skin-art/cut.sh dv-answer OUT`).
- Settings gallery: its tile is the Answering Machine line in `Gallery.devices` (`ui/gallery.slint`), band #d9452b.
- The recorder is the machine: the smoked display (the clip count with the NEW lamp, the
  state, the timer, the microphone), the red REC key, the micro cassette (its reels turn
  while recording), the LED meter (-40 to 0 dB) and the speaker. Wide windows move LOAD, ASK
  and MINI onto the key bed beside REC. Settings is the SET nub in the title bar. While
  recording, LOAD is PAUSE and RESUME (latched amber), since Import is not offered then.
- The clips are a numbered message log on paper. The highlighted one is lit red with its
  number on a small display; the head says "2 OF 8" ("MESSAGE 2 OF 8" wide), or
  "8 MESSAGES" when none is highlighted. Rows show the TRANSCRIBED stamp and "+ SYSTEM".
- The NEW lamp is lit while there are clips and blinks while one is recorded; with the
  reels that is the only motion (`RECORDING_FPS` 12, `EFFECT_FPS` 0).
- A wider portrait window gets more meter bars (10 at the mockup's 400px, up to 20); the
  well fits them.
- Its other pages are the classic ones in its beige palette (`Theme.pages` 0).
- Fonts (0.19 MB): Barlow Condensed 600 and 700 and Courier Prime 400 and 700 (OFL, no
  reserved names, Latin subset; their 5s are clear), and Memo Segments, a seven segment
  face drawn from the mockup's own segment shapes (2 KB: digits, minus, colon, point).
  `python tools/skin-art/dv-answer-fonts.py SRC OUT` makes all five.

**Lessons from the Answering Machine:**
- **Parts of any width in three slices.** A part with a gradient, an inset shadow, rings and
  round corners cannot be drawn by the software renderer. Cut it as its left end, a 2px
  column from its middle and its right end, each with the shadow margin (`Slice3`); the
  column stretched with `image-fit: fill` is exact, since the part does not change along its
  width. So an angled gradient becomes a vertical one (the display's sheen is 180 degrees,
  not 165).
- **Seven segment figures are a font.** Slint cannot split a string per character, so a
  frame strip per digit does not work for a timer. A small TrueType font built with
  fontTools (`FontBuilder`, the segments as polygons, the 7 degree lean, digits as tall as
  the em so the font size is the digit height) draws the mockup's shapes exactly. The unlit
  segments are the same layout as 8s in a dim colour underneath, picked by
  `character-count` and right-aligned with the lit text. No glow: copies of a text that
  changes cost CPU (see Phosphor).
- **A dot grille at any height without a tall image.** A 7px tile is blocky at 2.25x (tiles
  repeat one image pixel per logical pixel). The wide grille is cut 37 rows tall and built
  from `source-clip` pieces: the top 14 rows, one 7-row band repeated by a `for`, the bottom
  9 rows. Each piece starts on a multiple of 7, so the dots stay in step. The first version
  showed part of a 429 row cut (1.8 MB decoded at 2x).
- **Fewer `if`s, smaller exe.** One picture or `Slice3` whose sources are picked by a
  condition (and `visible` for overlays) instead of an `if` per variant (six key faces, a
  portrait and a wide version of each part) took the skin from +3.17 MB to +2.68 MB.
- **Toolbar room:** the words under the keys (RESUME is wider than its key) and the
  ALL PROJECTS chip count in the room the project name may take. Checked with a 40
  character project name and a query at 420x560, 520x700, 700x700 and 1100x700.
- **Checks:** the test copy's microphone is silent, so the meter stayed dark in every shot;
  Ask is not offered there (no local AI), so the ASK key was not seen; tooltips do not show
  headless (as for every theme).

**Budgets measured on the Answering Machine** (test copy, 2.25x display, 10 s samples):
- CPU idle 0.0% at 520x700 and 0.2% at 1100x700 (Ember 0.0% and 0.2%); recording 9.2% and
  11.1% (Ember 22.7% and 23.9%).
- Working set idle 45.4 vs Ember 39.6 MB (+5.8), wide 53.9 vs 48.0 MB (+5.9); recording
  48.2 vs 43.0 MB, wide 56.9 vs 51.4 MB.
- Exe +2.68 MB (55.54 to 58.21 MB): fonts 0.19 MB, art 0.24 MB, the rest generated code.
  Ember, Tape Deck and Phosphor main pages at 520x700 and 1100x700 match before and after.

**Built in step 5, Pocket Synth main page (2026-10-30):** `ui/skins/dv-synth.slint` (skin 11,
theme "Pocket Synth", index 18 in `Theme.names`), art in `ui/skins/dv-synth/` (73 PNGs, 148 KB)
from `tools/skin-art/dv-synth.html` (`sh tools/skin-art/cut.sh dv-synth OUT`). Its other pages
are the classic ones in a white aluminium and orange palette (`Theme.pages` 0).
- Settings gallery: its tile is the Pocket Synth line in `Gallery.devices` (`ui/gallery.slint`), band #ff6a1a.
- The recorder is the mono screen (state, microphone, timer, segment meter) with keys 09 to 12
  beside it: REC (Stop while recording), Import (Pause and Resume while recording, lit orange
  while paused), Ask (with the local AI) and the widget. Portrait windows list the clips as
  numbered rows with a key each; wide ones show them as a pad bank, four across and two down
  (three in a tall window), with "BANK A" for clips 1 to 8, B for 9 to 16. A pad plays or
  pauses its clip (Ctrl or Shift picks it), its label highlights it and a double click there
  opens it. The footer is transport key 13, a screen and step LEDs for the progress (a click
  jumps).
- The PROJ knob opens the project menu, like its screen. The IN knob only shows: the
  microphone is picked in Settings, as in the classic look, so the main page gains no option.
- `Main.cols` (4 for the wide pad bank, else 1) makes the arrow keys and `clamp-list` scroll by
  rows of pads; `AppWindow.move-selection` uses it. Up and Down still step one clip.
- Only the status square moves: it blinks while recording (`RECORDING_FPS` 4, `EFFECT_FPS` 0).
- Fonts: "Synth Figures", Doto 700 (OFL, no Reserved Font Name) as a static instance with only
  the timer's characters, 7 KB (`tools/skin-art/dv-synth-fonts.py`). Its 5 has a flat top and a
  square corner, so it does not read as S; its digits are all 0.6 em, so the timer holds still.
  The rest is Inter Tight and JetBrains Mono, already in the app (a 600 label asks for 700).

**Lessons from the Pocket Synth:**
- **A grid list needs its row count:** keyboard scrolling worked out `selected * row-h`, which
  is wrong for four clips to a row. `Main.cols` comes from `Theme.skin` and `Win.width`, not
  from the page, so no other skin has to reset it when the theme changes.
- **A pad in both directions:** its soft shadow and orange glow are nine-slice PNGs (outside the
  box only, as Spatial Glass); its body is drawn: a 2px lip under it, solid rounded caps as tall
  as the radius and a square gradient between them, the inner ring and the top light line. The
  fixed keys (function keys, row keys, the transport key) are whole PNGs with their shadows.
- **The pad bank's room comes from the scroller:** the pad pitch is its visible height over the
  rows shown, set from its `changed visible-height`, and `Main.row-h` follows it. The content
  rectangle in the Flickable needs `x: 0; y: 0`, or it is centred and the bank opens scrolled.
- **A logo row in a layout stretches:** Text and nested layouts take spare width by default, so
  the badge drifted to the middle of the title bar until they got `horizontal-stretch: 0`.
- **Toolbar room:** the screen around the project name is 92px of chrome (padding, PROJ, the
  chevron, gaps); counting 60px let a long name push the search off a 420px window. Checked
  with a 43 character project name, with and without a query (ALL PROJECTS), at 420x560,
  700x700, 820x600 and 1100x700.
- **Checks:** the test microphone is silent, so the meter showed only its first segments; Ask is
  not offered there; tooltips do not show headless. Once a release build stopped with rustc's
  `STATUS_STACK_BUFFER_OVERRUN`; the same build ran clean again.

**Budgets measured on the Pocket Synth** (test copy, 2.25x display, 10 s samples, 2026-10-30):
- CPU idle 0.3% at 520x700 and 1100x700 (Ember 0.2% and 0.0%); recording 10.3% and 8.8% (Ember
  16.1% and 19.4%).
- Working set idle 46.1 vs Ember 42.4 MB (+3.7), wide 53.7 vs 50.1 MB (+3.6); recording 48.6 vs
  46.1 MB, wide 56.4 vs 53.7 MB.
- Exe +2.34 MB (65.61 to 67.96 MB): art 0.15 MB, font 7 KB, the rest generated code. Ember, Tape
  Deck, Phosphor and Pocket main pages at 520x700, 420x560, 700x700 and 1100x700 match pixel for
  pixel before and after.

### Recipe from the pilot (follow it for the other skins)

- **Art:** an element page `tools/skin-art/<skin>.html` draws each fixed piece of the
  mockup alone (`?el=&w=&h=`); `sh tools/skin-art/cut.sh <skin> <out>` shoots them with
  headless Edge at 1x and 2x on transparency. Tape Deck: 26 PNGs, 140 KB.
- **Textures are tiles** (one period, alpha) over a Slint gradient: walnut, aluminium,
  paper, grille. Slint tiles in physical pixels, so pick the 2x tile when `1phx < 0.7px`.
- **Software renderer limits:** no drop shadows, no rounded clipping, and gradients
  ignore `border-radius`. Shadows are stacked translucent rectangles or part of a PNG;
  round gradient parts (knobs, LEDs) are PNGs.
- **Motion:** a frame strip, one frame picked with `source-clip-x` from `Fx.t`. Use the
  part's symmetry: 8 frames turn a 6-toothed hub through its 60 degree repeat. A needle
  is a `Path` whose end is `sin`/`cos` of an animated angle.
- **Text in cells** (the tape counter): a string cannot be split, so each cell clips the
  whole mono string to one character's advance (0.6 em for JetBrains Mono).
- **Fonts:** static instances with `fontTools.varLib.instancer` (`--update-name-table`,
  or set the names by hand when the weight has no named instance), subset to Latin with
  `pyftsubset --layout-features='*' --name-IDs='*'`. A new weight of a family the app
  already uses can change existing text (CSS weight matching): JetBrains Mono 700 would
  have taken a weight-600 label, so that label now asks for 500. Caveat's 0 reads
  clearly in "clip_100" at 19 to 23px in Slint, so it is used as it is.
- **Controls:** every device control keeps the classic accessible name and tooltip
  (REC is "Record" / "Stop and save", tips "Record" / "Stop"). While recording, the key
  beside REC becomes Pause (Import is unavailable then anyway).
- **Checks:** `THRASHER_TEST_KEEP_WINDOW=1` keeps the test window from turning into the
  widget while recording behind other windows, so recording shots work headless.
- **Shared widgets with a skin look** (step 3): prefer `visible: Theme.tape` over
  `if Theme.tape:` for decoration in widgets used many times (keys, switches, menu rows).
  Every `if` builds its own sub-component at every use; switching 23 of them to
  `visible` took 3.1 MB off the exe. Keep `if` only inside layouts, where a hidden item
  would still take room.
- **Images in layouts:** an `Image` sized 100% reports a preferred height from its
  width, which loops inside a `FlexboxLayout` (height for width). Give such a tile
  `preferred-width: 0; preferred-height: 0` (see `Plain` in `tape-pages.slint`).
- **Light fields:** a `TextField` on cream needs dark selection text; it now uses its
  `ink` for the selection in the deck look.
- **Slint:** `parent.parent` is not allowed; name the element instead.

**Lessons from Desktop 95** (step 5, 2026-10-29):
- **Pixel art:** the icons are cut at 1x only and drawn with `image-rendering: pixelated`
  at a whole scale (`Px` in `dv-os95.slint`). One white PNG plus `colorize` gives the LED
  in three colours and the dotted focus line in any colour.
- **Bevels** are eight one-pixel rectangles (`Bevel`: raised, pressed, sunken, thin,
  etched); no PNG needed.
- **A clipped Image crashes the software renderer** (Slint 1.18): setting `source-clip-*`
  on one instance of a component that inherits `Image` turns every instance into a
  clipped image without the clip's defaults, the source rect is empty, and the renderer
  panics (`target_pixel_buffer.rs:116`, range end out of range). Use a plain `Image` for
  the clipped one.
- **Fonts:** Pixelify Sans' 5 reads as S, so `tools/skin-art/dv-os95-fonts.py` redraws the
  5 from pixel rectangles measured on its glyph grid (400 and 700) and renames the family
  "Pixel 95" (OFL, no reserved name). `hinting = False` in the subsetter halves a pixel
  font (Slint does not use TrueType hinting); VT323 shows only the timer, so it is
  printable ASCII only.
- **A live trace without a clock:** a 2px column per point from this point to the next
  makes a connected pixel line; its height follows the live bars and its carrier shifts
  with the latest level, so it moves only when `Main.live` changes.
- **Checks:** posted mouse moves do not show tooltips in the headless copy (for any theme),
  and posted characters do not reach a `TextField`; set its text with UI Automation's
  ValuePattern instead. Read UI Automation boxes a second after a change, or the click
  lands where the element was. Give each parallel agent's test copy its own exe name
  (`thrasher-os95.exe`), since the scripts stop the copy by process name.

**Budgets measured on the pilot** (test copy, 2.25x display, 10 s samples):
- CPU idle 0.2% (Ember 0.3%); recording 12.7% at 520x700 (Ember 18.4%), 18.4% at
  1100x700 (Ember 22.8%).
- Working set idle 42.3 MB vs Ember 38.6 MB (+3.7); wide 49.9 vs 47.6 MB (+2.3).
- Exe +2.19 MB: fonts 0.34 MB, art 0.14 MB, generated code about 1.7 MB. At that rate
  15 skins land near 25 to 30 MB, inside the 35 MB budget (Max, 2026-10-28).

**Budgets measured on step 3** (test copy, 2.25x display, 10 s samples, 2026-10-29):
- CPU idle 0.0 to 0.6% on every page in both looks. Playing a clip on its page: Tape Deck
  28.6% at 520x700 and 34.5% at 1100x700 (the reels), Ember 27.5% and 23.9%.
- Working set, Tape Deck over Ember: +1.3 to +4.5 MB per page (main 41.7 vs 39.2 MB,
  one clip 76.5 vs 73.1 MB, settings 225.6 vs 224.3 MB after a long clip was played).
- Exe +10.35 MB (34.45 to 44.80 MB): the other pages' generated code, and the deck look
  in the shared widgets; the art is 22 KB. This is far more than the pilot's main page
  (2.19 MB), so all 15 skins at this rate would pass the 35 MB budget. Built with
  `opt-level = "s"` for the app crate only, the same change is +6.82 MB (29.73 to
  36.55 MB, and the whole exe 4.7 MB smaller). A choice for Max before the other skins.

**Built in step 5, Phosphor main page (2026-10-29):**
- `ui/skins/phosphor.slint` (skin 2, theme "Phosphor", palette 9). The other pages use the
  classic blocks in the Phosphor palette for now: `app.slint` picks those by `Theme.pages`
  (0 for every skin without its own pages; see Desktop 95), so they fall back to them.
- Shared changes, no visible change for the other looks: `TextField.has-focus` (the
  prompt draws its own resting block cursor) and `ProjectMenu.up` (the picker sits in the
  prompt at the bottom, so its menu opens upwards).
- Fonts: VT323 (the title, the timer and the record key; its 5 has a flat top and reads
  clearly) subset to Latin, 78 KB; IBM Plex Mono Regular, 136 KB, shipped **unmodified**:
  its OFL names "Plex" as a Reserved Font Name, so a subset (a modified version) could not
  keep the name. The mockup uses only the regular weight inside the window.
- Art: 16 small PNGs, 43 KB (corner vignette, block glow, dashed rules, light shade).
- Rust: `RECORDING_FPS` 4 for the lamp blinking while recording; `EFFECT_FPS` 0.

**Lessons from Phosphor:**
- **Overlays over the whole window are paid on every redraw.** The scanlines as a tiled
  image plus the vignette as one stretched image took recording from 29% to 59% of a core
  at 1100x700: the software renderer blends them over every dirty pixel, and a bigger
  tile does not help. Scanlines as 1px `Rectangle`s on whole device pixels (only a third
  of the pixels blend) and the vignette as four unscaled corner images brought it to 27%.
- **Faking a glow with copies of a text** that changes (the timer, 10 times a second)
  cost 4 to 6%; Phosphor has no timer glow. Static glows (the lamp, the cursor) are a
  pre-blurred PNG behind the block.
- **Text cells:** Plex Mono and VT323 are monospaced (0.6 em and 0.4 em), so the meter's
  cells and the cursor after "Search" are placed by character count, like the tape counter.
- **`accessible-role` must be a constant:** a key that is sometimes a checkbox sets it
  where it is used (`--all`), not through a property.
- **A size that depends on the window** must come from outside the layout: the wide
  readout gets its height as `avail` from MainPage, or the timer size loops through it.
- **Checks:** posted mouse moves do not raise hover in the hidden test window (no tooltip
  shows in any look), and the wheel and scrollbar drag did not scroll there either; keys
  do (arrows scroll the list). Search text must be typed as key presses (`vk`), not
  `WM_CHAR`. Each parallel skin agent runs its own exe name (`thrasher-phos.exe`) and its
  own copies of `win.ps1` and `press.ps1`, so they never kill each other's window.

**Budgets measured on Phosphor** (test copy, 2.25x display, 10 s samples, 2026-10-29):
- CPU idle 0.0 to 0.3% (Ember 0.3%); recording 17.2% at 520x700 (Ember 18.7%), 26.7% at
  1100x700 (Ember 24.5%; the wide readout redraws its scope and the big timer).
- Working set idle 43.6 vs Ember 40.2 MB (+3.4); wide 53.0 vs 48.2 MB (+4.8).
- Exe +2.28 MB (44.83 to 47.11 MB): fonts 0.21 MB, art 0.04 MB, the rest generated code.

**Budgets measured on Desktop 95** (main page, test copy, 2.25x display, 10 s samples,
2026-10-29):
- CPU idle 0.3% at 520x700 and 0.6% at 1100x700 (Ember 0.2% and 0.0%); recording 8.4%
  and 14.8% (Ember 20.3% and 24.5%).
- Working set idle 42.6 MB vs Ember 39.9 MB (+2.7); wide 49.2 vs 48.1 MB (+1.1).
- Exe +1.98 MB (44.83 to 46.81 MB): fonts 80 KB, art 3 KB, the rest generated code.
- Review re-check: idle 0% at both sizes (Ember 0% and 0.3%), recording 7.8% and 11.7%
  (Ember 16.9% and 22.9%), working set 39.2 and 48.3 MB (Ember 38.0 and 46.3 MB).

**Budgets measured on Comic Book** (test copy, 2.25x display, 10 s samples, 2026-10-29):
- CPU idle 0.2 to 0.5% (Ember 0.2 to 0.3%); recording 7.0 to 8.6% at 520x700 (Ember
  18.1%), 10.2% at 1100x700 (Ember 21.9%).
- Working set idle 41.1 to 41.4 MB vs Ember 40.8 MB; wide 50.0 vs 48.6 MB; recording
  44.8 vs 43.6 MB, wide 55.4 vs 51.7 MB.
- Exe +2.35 MB (44.83 to 47.18 MB): fonts 0.09 MB, art 0.46 MB, generated code about
  1.8 MB. Ember and Tape Deck main pages at 520x700 and 1100x700 match before and after.

**Built in step 5, Spatial Glass main page (2026-10-29):** `ui/skins/x-spatial.slint`
(skin 5, theme "Spatial Glass", index 12 in `Theme.names`), art in `ui/skins/x-spatial/`,
cut by `sh tools/skin-art/cut.sh x-spatial OUT` from `tools/skin-art/x-spatial.html`. Its
other pages keep the classic layout in its palette for now (`Theme.pages` 0; only Tape
Deck has its own). It uses `ProjectMenu.up` (added with Phosphor) to open the project menu
above its picker (the ornament sits at the window's foot).
- **Pre-blurred room:** the room behind the glass is one picture per orientation at half
  the mockup's size (200x310, 500x310), stretched to the window. It is blurred, so the
  renderer's nearest-pixel scaling does not show.
- **Shadows as nine-slice:** `@image-url("x.png", nine-slice(t r b l))` works in the
  software renderer. A CSS `box-shadow` on a transparent box paints only outside the box,
  so the cut picture has a clear middle and the glass over it stays clear. The borders
  are in source pixels and drawn at 1x, so use it only for soft parts (shadows).
- **Glass light:** gradients ignore corners, so a pane's top light is a rounded band down
  to where the corners end, then a square gradient below it.
- **Paths are costly:** the renderer draws every Path into a mask of its whole box each
  time anything over it changes. The halo's 72 ticks as ring-sized paths and the tilted
  pane as one window-high path took recording at 1100x700 to 33.9% CPU; ticks in 14px
  boxes and the pane as two slanted path ends over plain rectangles brought it to 17.8%.
  Keep paths small; draw what is upright with rectangles.
- **Tilt without rotation:** the wide recorder pane's outline is a trapezoid (its far edge
  12px shorter at each end); the contents stay square. Its nine-slice shadow is cut into
  16 upright strips, each shrunk to the slant at its right end; one upright box left its
  clear middle showing past the slanted ends (a pale corner, a dark slab).
- **Stroked paths shrink:** Slint fits a path into its box less the stroke width, keeping
  the aspect, so a 1px outline in a 372x38 box came out 3% smaller (5px in from each
  side) and the halo's 3px ticks in 14px boxes 21% shorter. Give a stroked path a box
  larger by the stroke and move it back by half (see `RecorderTilt` and `Orb`).
- **No seams:** a gradient under a lit band starts at the band's brightness.
- **Tabular figures:** Inter Tight's digits are proportional, so the timer would jitter.
  `tools/skin-art/spatial-figures.py` maps the `tnum` glyphs as the plain ones and keeps
  only the timer's characters, as its own family "Spatial Figures" (300 and 400, 7 KB),
  so no other text can pick it up. Its 5 is clearly a 5.
- **Baselines:** a smaller tenth beside the timer sits on the same baseline when it is
  lifted by 0.242 em (Inter's descent) times the size difference.
- **Short windows:** below 470px of pane height the wide recorder takes the small orb and
  timer.
- **Checks:** typing reaches a field headless only as posted key-downs (VK codes); posted
  `WM_CHAR` does not. The wheel needs a posted mouse move first. Posted hovers do not
  show tooltips in any theme (checked on Ember too).

**Budgets measured on Spatial Glass** (test copy, 2.25x display, 10 s samples):
- CPU idle 0.0 to 0.2% (Ember 0.0 to 0.3%); recording 7.7% at 520x700 (Ember 17 to 19%),
  13 to 18% at 1100x700 (Ember 22 to 25%).
- Working set idle 41 to 42 vs 40 to 41 MB (about +1), wide 52 vs 48 to 49 MB (+3).
- Exe +2.0 MB (44.83 to 46.85 MB): art 0.41 MB, fonts 7 KB, the rest generated code.

**Built in step 5, Quest Menu main page (2026-10-29):** `ui/skins/px-quest.slint` (skin 8,
theme "Quest Menu", index 15 in `Theme.names`), art in `ui/skins/px-quest/` (23 PNGs, 4 KB)
from `tools/skin-art/px-quest.html` (`sh tools/skin-art/cut.sh px-quest OUT`). Its other
pages are the classic ones in its blue and gold palette (`Theme.pages` 0).
- Settings gallery: its tile is the Quest Menu line in `Gallery.devices` (`ui/gallery.slint`), band #2f5fd0.
- Every panel is a menu window; the recorder is a character card (mic sprite, status lamp,
  timer, IN bar, round REC); Import, Ask and the widget are a command menu, beside the
  project and search (portrait) or docked under the card (wide). While recording the
  menu's Import becomes Pause and Resume (Import is not offered then anyway). The player
  is the EXP bar; the highlighted clip has the glove.
- Fonts: "Pocket Small" (Silkscreen 400 with a square 5, shared with Pocket and Brick Phone,
  built by `px-pocket-fonts.py`) for the labels and lengths, since Silkscreen's own 5 differs
  from its S by two pixels; Silkscreen 700 (OFL, no Reserved Font Name, Latin subset, 13 KB)
  only for the title, which has no digits; Pixel 95 (Desktop 95's Pixelify Sans with the clear 5) for names and menus,
  now with a 500 weight for the names; "Quest Figures" (2 KB, with the same square 5) for the timer. Built by
  `tools/skin-art/px-quest-fonts.py` and `dv-os95-fonts.py`.
- Rust: `RECORDING_FPS` 4: the glove and the status lamp blink while recording (on and off
  every half second, as the mockup's 1 s blink); at rest nothing moves.

**Lessons from the Quest Menu:**
- **Pixel frames without art:** the mockup's `border-image` frame is three 3px rings, and the
  outer ink ring is the page colour, so a window is a white and a grey ring of four
  rectangles each plus four face bands (`Win`). `clip-path` corners are 3px squares in the
  colour behind (`Notches`). The pixel art itself is 1x PNGs drawn at 3x or 6x, pixelated.
- **Tabular pixel figures:** Silkscreen's 1 is a pixel narrower than the other digits, so the
  timer shifted whenever a 1 came or went. "Quest Figures" is its timer characters with the
  1 given the digits' advance and centred in it, under its own family name.
- **A new weight of a shared family:** Pixel 95 500 is a third of the way between the
  variable font's two masters (no avar table), with the 5 redrawn from the interpolated
  grid. Desktop 95 asks only for 400 and 700, so its text did not change (pixel match).
- **Fixed windows in a column:** items of a `VerticalLayout` stretch by default, and a
  window whose content is a layout takes that layout's sizes, so the card and the tools grew
  until they got `vertical-stretch: 0`. A width that is "fixed when wide, else as long as
  the text" is `min-width`/`max-width`, not `width: cond ? 180px : self.preferred-width`.
- **Rotated text is art** again: the wide menu's upright CMD title is a PNG at 1x and 2x.
- **What recording costs wide:** the timer's ink shadow (a second 52px text) about 2.5% at
  1100x700, the blinking about 1%; narrowing the timer's boxes to the text saved nothing
  measurable.
- **UI Automation:** a component that inherits `Text` still shows as a Custom element with
  its text when `accessible-role: none` is set where it is used; use a plain `Text`.
- **Checks:** a project folder is listed only with a `.thrasher-project` file in it, and a
  clip has "+ system" when `<name>.system.wav` sits beside it, so both can be staged in the
  scratch `THRASHER_DIR` (long project name, system rows). The test microphone gives
  silence, so the lit IN bar was not seen in the app.

**Budgets measured on the Quest Menu** (test copy, 2.25x display, 10 s samples):
- CPU idle 0.0 to 0.6% (Ember 0.0 to 0.2%); recording 5.5 to 6.7% at 520x700 (Ember 16.9
  to 20.9%), 16.3 to 20.0% at 1100x700 (Ember 22.2%).
- Working set idle 41.7 vs Ember 41.1 MB; wide 49.8 vs 48.6 MB; recording 45.0 vs 43.6 MB,
  wide 53.4 vs 51.9 MB.
- Exe +2.22 MB (55.54 to 57.76 MB): fonts 56 KB, art 4 KB, the rest generated code. Ember,
  Tape Deck, Phosphor and Desktop 95 main pages at 520x700 and 1100x700 match before and
  after.

**Built in step 5, Brick Phone main page (2026-10-29):** `ui/skins/dv-brick.slint` (skin 9,
theme "Brick Phone", index 16 in `Theme.names`, after Pocket and Quest Menu), art in `ui/skins/dv-brick/`
(112 PNGs, 138 KB) from `tools/skin-art/dv-brick.html` (`sh tools/skin-art/cut.sh dv-brick OUT`;
`ONLY="a b"` cuts just those pieces). Its other pages are the classic ones in a charcoal and
amber palette (`Theme.pages` 0).
- Settings gallery: its tile is the Brick Phone line in `Gallery.devices` (`ui/gallery.slint`), band #7a8a3a.
- The keys do the work: the green call key is Record, then Pause and Resume while recording;
  the red end key is "Stop and save" (off while idle); the Menu soft key opens Settings and
  Find puts the cursor in the search (as Ctrl+F); keys 1 2 3 are Import, Ask (a plain "2 ABC"
  key without the local AI) and the widget; the rocker's up and down move the highlight as the
  arrow keys do and its OK plays or pauses, like the footer's play. Keys 4 to # only show.
- Portrait windows under 540 px high keep only keys 1 2 3 of the keypad; wide windows under
  400 px of LCD leave out the big mic, and the right wing's gaps close up on short windows.
- Fonts: Chakra Petch 600 and 700 (keys and brand), "Pocket Small" (Silkscreen 400 with a square
  5, since its own 5 differs from its S by two pixels; LCD small caps, used at 8 px only, where
  one font pixel is one px; shared with Pocket, built by `px-pocket-fonts.py`), VT323 (LCD words) and "Brick Figures" for the timer
  (`tools/skin-art/dv-brick-fonts.py`), all OFL, Latin subset, 66 KB.
- Wide rows give up the waveform as the list narrows (18 bars from 340 px of row, 10 from 280
  px, none below), so the clip names keep their room at the 820 px minimum.
- Rust: `RECORDING_FPS` 4 for the status dot blinking while recording; `EFFECT_FPS` 0.
- Shared, no visible change elsewhere: `Scroller.bar` (off: the skin draws its own scrollbar)
  and `TextField.select-fill` / `select-ink` (the LCD inverts selected text).

**Lessons from the Brick Phone:**
- **Elements outside a layout are centred by default.** An item with no `y` in a plain
  Rectangle sits in the middle of its parent, not at the top (the keypad's first row and the
  LCD sheen did). Give every placed item both `x` and `y`.
- **`Image` sizes itself from its source** outside a layout: a tile with no `width` and
  `height` is one tile big. Set both on every tiled image.
- **Stretching rubber keys:** a CSS key (gradient, rounded, inset light, drop shadow) is cut in
  three: the left 30 px, one 1 px column from the middle and the right 30 px, each with its
  shadow margin. The column stretched with `image-fit: fill` is exact, so the key takes any
  width with no seam. Hover and press are rounded solid rectangles in the key's corner radii
  as CSS shrinks them (radii that add up past a side are scaled down together).
- **A body gradient with rounded corners:** solid top and bottom caps as tall as the radii and
  a square gradient between them, starting and ending on the caps' colours; the top light is a
  lighter rounded rectangle 2 px above the body.
- **Rounded LCD without rounded clipping:** four 6 px `Path` corners in the bezel's colour.
- **A digit narrower than the rest jumps the timer:** Pixel 95's 1 is 0.43 em, the other digits
  0.60. Brick Figures widens the 1 to the other digits and centres its outline, as its own
  family with only the timer's characters (3 KB).
- **Accessible roles are constants**, so a key that only shows is `KeyFace` without a
  TouchArea and a key that acts is `Key` (button, classic name and tip).

**Budgets measured on the Brick Phone** (test copy, 2.25x display, 10 s samples, 2026-10-29):
- CPU idle 0.0 to 0.2% (Ember 0.0 to 0.3%); recording 7.2% at 520x700 (Ember 21.1%), 16.4% at
  1100x700 (Ember 28.1%).
- Working set idle 44.6 vs Ember 40.6 MB (+4.0); wide 52.1 vs 48.2 MB (+3.9).
- Exe +2.79 MB (55.54 to 58.33 MB): art 0.14 MB, fonts 0.07 MB, the rest generated code. Ember,
  Tape Deck and Phosphor main pages at 520x700, 420x560, 700x700 and 1100x700 match before and
  after.

### Art

- **Chrome and textures** (walnut, leatherette, brass, bezels, pixel frames) become small
  PNGs, cut out of the mockups with headless Edge at 1x and 2x. They live in
  `ui/skins/<name>/`, are scaled or tiled in Slint, and are embedded in the exe.
- **Live parts** (timer, meter, waveforms, text, buttons, hover and pressed states) are
  drawn in Slint so they stay sharp and react.
- **Motion** (reels, vinyl, gears, the pet, fireflies) uses short frame strips stepped by the
  `Fx` clock, not rotation, since the software renderer cannot rotate items cheaply.
- **Pixel art** is drawn at whole-pixel scales with no smoothing.

### Fonts

- Each skin brings its fonts (OFL only), cut down to Latin characters to keep the exe small.
  Expected: about 12 new fonts, roughly 1 to 2 MB in all.
- A font where 5 looks like S is swapped for a clearer one.

### Budgets, checked per skin

- **CPU:** a still main page uses 0% CPU; an animated one stays under 20% of one core at
  520x700 (Lagoon, the busiest today, uses 30%).
- **Memory:** idle memory grows by at most 10 MB over Ember with the skin active. Skins that
  are not active load nothing.
- **Exe size:** no fixed cap. After step 3 (Tape Deck costs about 12.5 MB in all) Max chose
  the full look on every page for all 15 skins and accepted an exe near 200 MB (2026-10-29).
  Still keep generated code lean where it costs nothing in looks.

## 4. Theme picker

With 23 themes, a dropdown is too long. Settings gets a theme gallery: small thumbnails in
two groups, Classic and Devices. Clicking one switches at once. The thumbnails come from
the mockup screenshots.

Max picked layout **B, strips** (2026-10-29), mockup in `mockups/device-themes/gallery/`:
one sideways strip per group with page arrows, a thin scroll bar and a fade, so the card
stays about 250 px tall. Tiles 104x64 (device art at 1x and 2x, 64 colours, about 153 KB
in all); classic tiles are drawn live from the palette. Only built skins are listed.

**Built in step 4 (2026-10-29):** `ui/gallery.slint`, used by the classic settings page
(`app.slint`, so every skin without its own pages) and the Tape Deck one
(`skins/tape-pages.slint`). It replaces the Theme dropdown; the Appearance card now runs
across the whole page when it is wide, the other cards stay in their columns under it.
- One strip per group. Page arrows and the scroll bar show only while a strip has more
  than fits; when everything fits, the gap shrinks (12 px, not under 4 px) so no tile is
  cut. The mouse wheel scrolls the strip under the pointer until its end, then the page.
  A strip opens scrolled to the current theme.
- Tiles are radio buttons named after the theme (`accessible-checked` on the current
  one). Each strip is one Tab stop: Left and Right move the focus ring, Home, End,
  Page Up and Page Down jump, Up and Down go to the other group, Enter or Space picks.
- Ember look: framed picture, the name under it, accent frame and check on the current
  one. Tape Deck look: a sunk well and a cassette label with the theme's colour band,
  amber well and lit lamp on the current one, red frame on the focused one. The theme
  switches on click, no Apply. A tile scrolled into view (by key or on opening) clears the
  edge fade; the Ember fade is solid strips on whole device pixels, as a see-through
  gradient left bright specks on tile edges in the software renderer.
- Device art: `ui/skins/thumbs/<skin>.png` and `<skin>@2x.png` for all 15 skins (2x when
  `1phx < 0.7px`, like the other skin art), cut by `python tools/skin-art/thumbs.py`
  from `mockups/device-themes/<skin>.png` (the wide window, 1000x620, scaled and quantised
  to 64 colours). Re-run it when a mockup screenshot changes.
- Rust (`fill_gallery` in `main.rs`) lists the classic themes (`Theme.skins` 0) and the
  device lines whose name is in `Theme.names`, in that order, with their place there.
- Cost: exe +0.36 MB (55.54 to 55.91 MB, with 6 device tiles). Main pages of all 14
  themes unchanged (static ones pixel for pixel; the animated ones differ only by their
  animation, as much as two runs of the old build do).

**Adding a skin's tile** (one line): when a skin's name goes into `Theme.names`, add its
line to `Gallery.devices` in `ui/gallery.slint`:

```
{ name: "Pocket", art: @image-url("skins/thumbs/px-pocket.png"), art2: @image-url("skins/thumbs/px-pocket@2x.png"), band: #8bac0f },
```

The name must match `Theme.names` exactly; `band` is the colour stripe of its Tape Deck
label (the mockup's list: Pocket #8bac0f, Quest Menu #2f5fd0, Cozy Cabin #c9822e,
Answering Machine #d9452b, Pocket Synth #ff6a1a, Brick Phone #7a8a3a, Phonograph
#b48c4b, Pinball #e0316b, Fighter Select #d63a2a). The test
`every_device_skin_has_a_gallery_tile` fails while a device theme has no line, a line
names a theme that does not exist, or its art is missing.

## 5. Plan

Each step is one PR, tested on the headless test copy, and merged only after Max says yes.

1. **Skin system:** split the main page into regions and move the classic look into
   `classic.slint`. No visible change; screenshots must match.
2. **Pilot: Tape Deck.** It has textures, a layout change (piano keys take the toolbar
   buttons), a needle meter and spinning reels, so it proves the whole pipeline: art cut
   from the mockup, frame strips, fonts, budgets. Max reviews it in the app before the rest
   are built.
3. **Tape Deck, other pages:** browser mockups of the clip page, settings, merge, menus,
   dialogs and the widget in the Tape Deck look. Max picks, then they are built.
   Done 2026-10-29 (see "Built in step 3" above).
4. **Theme gallery** in settings. Done 2026-10-29 (see "Built in step 4" in section 4).
5. **The other 14 skins,** main page first, then their other pages (mockups first, in batches), built in parallel in separate worktrees, one PR each, in this
   order:
   - easier first: Phosphor, Desktop 95, Comic Book, Spatial Glass, Car Dashboard
   - then: Pocket, Quest Menu, Brick Phone, Answering Machine, Pocket Synth, Fighter Select
   - heaviest last: Cozy Cabin, Pinball, Phonograph
6. **Docs:** update `docs/plan.md` section 16 and the building notes.

### Checks for each skin PR

- Headless screenshots in portrait (520x700) and wide (1100x700), compared side by side with
  the mockup.
- Every main-page control works: record, import, ask, widget, settings, project picker,
  search, play, row menu and the scrollbar.
- Recording state shown (timer running, meter moving).
- CPU, memory and exe size within budget.
- `cargo fmt`, `clippy -D warnings`, tests.

## 6. Risks

- **Fidelity:** some mockup effects (blur on Spatial Glass, glows) need a cheaper version.
  Pre-blurred images stand in for live blur.
- **Layout at other sizes:** the mockups are 400x620 and 1000x620. Each skin must also look
  right between the portrait and wide sizes.
- **Size of the work:** 15 skins is a lot. The pilot shows the real cost per skin before the
  rest start.
- **Exe size** (pilot, 2026-10-28): Slint generates about 1.7 MB of code per skin main
  page, so 15 skins would reach 25 to 30 MB. Max raised the budget to 35 MB (2026-10-28).
  Step 3 then added 10.35 MB for the other pages; Max accepted the size for the full look
  (2026-10-29), see the budget above.
