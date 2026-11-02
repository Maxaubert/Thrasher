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
- `Theme.pages` says how the other pages are built: 1 for Tape Deck, 2 for Phosphor, 3 for
  Desktop 95, 4 for the Comic Book, 5 for Spatial Glass, 6 for the Car Dashboard, 7 for
  Pocket, 8 for the Quest Menu, 9 for the Brick Phone, 10 for the Answering Machine and 11
  for the Pocket Synth (each since its pages were built), else 0, the classic pages in the
  theme's palette. A skin whose other pages are not built yet keeps them classic this way;
  `app.slint` picks the main page by `Theme.skin` and the rest by `Theme.pages`. Desktop
  95's palette is grey and navy for those pages.
- The scope and the LED level follow `Main.live` like the classic meter, so the skin needs
  no clock (`EFFECT_FPS` and `RECORDING_FPS` 0). Nothing else moves.
- `Main.transcribed-count` (set in `refresh_clips`) gives the wide status bar its
  "N transcribed". The Type column shows from a 560px file window, so the mockup's 1000px
  wide window has Type and Wave.

**Built in step 5, Desktop 95 on every other page (2026-10-30):** mockups in
`docs/mockups/device-themes/dv-os95-pages/` (Max approved them, widget variant A).
`Theme.pages` is 3 for it (Tape Deck keeps 1, every other skin 0), and `Theme.os95` says the
shared widgets draw the 95 look.
- `ui/skins/dv-os95-parts.slint`: the parts the main page and the other pages share (the
  push button, a centred icon, a window with its title bar and hint, the app's title bar,
  the teal desktop, `Frame` round a page, the yellow `Message`). The colours, the bevels,
  the pixel icons and the dotted line moved to `common.slint` (`Os`, `OsBevel`, `OsPx`,
  `OsDotted`, `OsIcons` for icons by name) so the shared widgets can draw them;
  `dv-os95.slint` imports them under its old names, so the main page is drawn as before.
- `ui/skins/dv-os95-pages.slint`: `ClipPage` (a player window with the black display, the
  white seek field with two grey trim thumbs and the transport; Notepad round the transcript,
  active blue with the change count in its status bar while editing; a Parts window for a
  merged clip; the taskbar with Export as the Start button), `SettingsPage` (a properties
  sheet: two rows of tabs in portrait, the picked row in front; every group box at once in
  three columns when wide), `ModelsPage` (Add or Remove Programs: filter tabs, or a folder
  tree when wide; LED ladders; In use on the navy row), `MergePage` (a setup wizard), `AskPage`
  (the Find dialog with a Help page answer), `Widget` (the mini Sound Recorder), `Dialog`,
  `Speaking` and `Drop` (message boxes over the shutdown dither).
- The shared widgets' 95 look: `Btn` (grey key, the main action the default button with its
  black ring, etched grey words when off, `least` for the 74 px message box keys), `IconBtn`
  (a small key with the pixel icon from `OsIcons`, pushed in on a checker while active),
  `Toggle` (a checkbox with its `caption` beside it), `Select` (white drop down box, a white
  list with the row under the mouse navy), `ShortcutBox` (a hot key field, "Ctrl + Alt + R"),
  `MenuCard`/`MenuRow`/`MenuRule` (grey 95 menus with icons, navy rows; `list` makes the project
  menu a white drop down list with folders), `ExportButton` (the Start button and a Start menu
  with the upright "Thrasher Export" band), `OrderList` (grip, navy place, play key, wave icon,
  a lifted row raised with a dotted line), `TranscriptCard` (Notepad's toolbar and white page in
  VT323), `Scroller.bar95` (the grey scroll bar with arrows; the content leaves 16 px),
  `Tip` (the yellow tip) and the theme gallery (pictures on a white list, the name navy on the
  current one, a 95 scroll bar). `Theme.menu-h` is 22 px for its menus, else 34 px.
- Rust: the editor's pause tags are blue (#1c3fa8) on Notepad's page.
- Art: 28 new pieces at 1x (pixel icons from the mockups' maps, the Start menu band words at
  1x and 2x), `sh tools/skin-art/cut.sh dv-os95 OUT` (`ONLY="a b"` for some).

**Budgets measured on Desktop 95's other pages** (test copy, 2.25x display, 10 s samples,
2026-10-30; Ember in brackets):
- CPU idle 0.0 to 0.5% on every page at 520x700 (Ember 0.0 to 0.5%), the widget 0.9% (2.2%);
  0.0 to 0.3% at 1100x700 (Ember 0.2 to 0.9%). Nothing moves on a clock; only Speaking steps
  its six frames while it shows.
- Working set at 520x700: main 42.3 (41.1), one clip 77.3 (76.8), settings 50.9 (51.5),
  models 52.1, Ask 52.3 (50.9), widget 45.4 (45.0) MB; at 1100x700: main 51.2 (49.6), one
  clip 85.7 (83.8), settings 60.8 (60.5), models 62.0 (60.1), Ask 61.7 (60.2), widget 47.4
  (46.3) MB. At most +2 MB.
- Exe +12.75 MB (77.76 to 90.50 MB): the pages' generated code and the 95 look in the shared
  widgets; art 10 KB. More than Tape Deck's step 3 (+10.35 MB): the shared widgets now carry
  two device looks each.
  Ember and Tape Deck: main page, one clip, settings, models, merge, the delete question, Ask
  and the widget at 520x700 and 1100x700 match before and after, pixel for pixel (32 shots;
  two runs of the old build match too).

**Lessons from Desktop 95's other pages:**
- **`slint-viewer --check app.slint`** compiles the UI in a second with no cargo lock, and
  `slint-viewer --screenshot out.png --size WxH --load-data state.json preview.slint` renders a
  page off screen with sample state set from a small wrapper (set the state in `changed`
  handlers: `--load-data` comes after `init`). Both saved most of the waits on the shared target.
- **Pure black did not colorize** in the off-screen preview: a white checker tile with
  `colorize: #000000` stayed white (no dither); `#010101` works there and in the app.
- **No `parent` in a component's own bindings:** a component placed by its user (the
  message tip) gets its area as properties (`area-w`, `area-h`).
- **An `if` element cannot be named from outside it:** the rename field in the player's
  title bar reports its text through a callback instead of being read by id.
- **Overriding `min-width` at a use site replaces the widget's own:** `Btn { min-width: 74px }`
  would cut "Discard changes"; `Btn.least` keeps the larger of the two.
- **Width from the parent in a layout loops:** `width: min(240px, root.width - 40px)` inside the
  transcript card was a binding loop; `preferred-width` and `min-width` do it.
- **Behind a test copy:** the transcription and AI workers (`thrasher-transcribe.exe`,
  `thrasher-ask.exe`) next to the test exe turn on Transcribe, Models and Ask, so those pages
  can be reached headless (never press Download there: models go to the real models folder).

**Built in step 5, Comic Book main page (2026-10-29):**
- `ui/skins/x-comic.slint` (skin 4, theme "Comic Book") and `ui/skins/x-comic-parts.slint`
  (the `Comic` inks, `Box`, `Glyph`, `InkBtn`, `Caption`, `Tag`, `Bubble`, `Burst`,
  `Meter`, `InkBars`, `PlayDot`), ready for the other pages. Fonts Bangers 400 and Comic
  Neue 700 (OFL, Latin subset, 92 KB); Bangers' 5 has a flat top and a sharp corner, so it
  does not read as S. Art: 23 PNGs, 459 KB (`sh tools/skin-art/cut.sh x-comic OUT`).
- The theme is index 11 in `Theme.names`, `Theme.skins` 4. Its other pages have their own
  look since 2026-10-30 (`Theme.pages` 4, see "Built in step 5, Comic Book other pages").
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

**Built in step 5, Car Dashboard on every other page (2026-10-30):** mockups in
`docs/mockups/device-themes/x-dash-pages/` (approved by Max 2026-10-30, flavour labels kept).
- `ui/skins/x-dash-pages.slint`: `ClipPage`, `SettingsPage`, `ModelsPage`, `MergePage`,
  `AskPage`, `Widget`, `Dialog`, `Speaking` and `Drop`, the same interface as Tape Deck's.
  `Theme.pages` is 6 for the skin (Tape Deck stays 1, every other skin 0); `app.slint` has a
  `Theme.pages == 6` block next to each Tape Deck one, with the same bindings.
- `ui/skins/dash-parts.slint`: the parts the main page and the other pages share, moved out
  of `x-dash.slint` unchanged (`Dash`, `Art`, `WinKey`, `Title`, `Spoke`, `Tile`). `Title`
  takes the mode words (REC MODE on the main page, PLAY, SETUP, MODEL STORE, MERGE, ASK)
  and an optional back chip (CLIPS on the clip page, "Back to Clips" as in the classic look).
- The clip page is the car's now playing screen: the arc gauge is the trip meter (play
  position, PLAYING, PAUSED or STOPPED, the length; wide windows label both ends of the
  scale), the waveform is the route strip in a well (trim grips and play head in ice),
  speed is a drive mode selector, Rename and Delete are spoke keys under the name. Settings
  is the vehicle menu: glass cards headed by an ice icon tile; wide puts the gallery card
  on the left and the other cards in two columns. Models is the app store (shelf chips, or
  a shelf list and the DISK readout when wide). Merge is a route planner: the parts are
  numbered stops on an ice route line, and wide windows show the merged total on the gauge
  with the stops as coloured legs. Ask is the voice assistant (ANSWER card, numbered
  sources). The widget is the mini cluster (variant A): the level arc with the timer,
  the status lamp and word, Pause while recording and the red hub.
- The shared widgets draw the dash look when `Theme.dash` (`Theme.pages == 6`) is set, so
  the main page's menus get it too: `MenuCard` (glass with an ice light on its top edge),
  `MenuRow` (an icon before every row, a folder tile before each project; icons are passed
  as `Theme.dash ? "pencil" : ""`, so the other looks get none), `Select` (glass chip with a
  chevron), `Btn` (pills; `red` is the red Delete pill), `IconBtn` (glass keys), `Toggle`
  (blue track, white knob), `ExportItem`, `ExportButton`, `OrderList` (route stops),
  `TranscriptCard` (small caps, EDITING in ice, an amber change chip), `Tip` (ice top line),
  the gallery's strip heads, and `TextField` takes Manrope. The window's default font is
  Manrope while the skin is on.
- Rust: `Pages.merge-total` and `Pages.merge-legs` (the merged length and each clip's part
  of it, `merge_legs` in `main.rs`, tested). `THRASHER_TEST_COVER=speaking` or `dropping`
  (with `THRASHER_TEST_WINDOW`) shows a cover at start, for headless shots of the covers.
- Art: two new pieces, `gauge-c` (the gauge's scale with all ticks grey) and `ctile` (the
  icon tile's gradient), from `tools/skin-art/dash.html` (`ONLY="gauge-c ctile" sh
  tools/skin-art/cut.sh dash OUT`).

**Lessons from the Car Dashboard's other pages:**
- **Mockup glitches fixed, not copied:** the portrait Loop pill ran out of the window and
  the transcript card was cut at the right; the transport is sized to fit 420px (keys 32px,
  speed steps 27px) and the wide one to fit the 420px cluster. A gallery in a narrow column
  shows two tiles and pages through the rest.
- **A positioned layout needs a width:** a `HorizontalLayout` placed with
  `x: parent.width - self.width - 8px` outside a layout takes the parent's width, so `x`
  went negative; give it `width: self.preferred-width`.
- **An `Art` (Image) without a size draws at its natural size,** the 2x cut twice as big;
  give icon tiles their width and height.
- **The wheel over a gallery strip scrolls the strip,** not the page: scroll settings with
  the pointer below the Appearance card in checks.
- **Checks:** posted clicks can land where an element was a moment ago; one of the first
  runs started a recording that way (deleted). The scripts now retry a name for up to 4 s,
  wait 1.4 s after every press, and list new clips at the end. The list rows of the dash
  main page in a portrait window end around the eighth clip: drive the short clips at the
  top (clip_106, clip_108).

**Budgets measured on the Car Dashboard's other pages** (test copy, 2.25x display, 10 s
samples, pages visited in turn in one run, 2026-10-30):
- CPU idle 0.0 to 0.6% on every page (Ember 0.0 to 0.9%).
- Working set over Ember at 520x700: main +1.1, clip -4.2, settings +3.3, models +3.8,
  ask +4.9, merge +3.2, widget +2.8 MB; at 1100x700: +2.4, +5.7, +5.9, +6.0, +5.8, +4.5,
  +7.2 MB (clip page 158.2 vs 162.4 MB and 170.6 vs 164.9 MB after a long clip loaded).
- Exe +8.77 MB (77.76 to 86.52 MB): the pages' generated code and the dash look in the
  shared widgets; the art is 35 KB, no new fonts.
- Ember and Tape Deck: every page, menu and question at 520x700 and 1100x700 matches before
  and after pixel for pixel (the Ask field's blinking cursor aside); so does the Car
  Dashboard main page, whose menus and questions take the new look.

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

**Built in step 5, Pocket on every other page (2026-10-31):** mockups in
`docs/mockups/device-themes/px-pocket-pages/` (approved by Max 2026-10-31, widget variant A, the
bezel keys act).
- `ui/skins/px-pocket-parts.slint`: the parts the main page and the other pages share, moved out
  of `px-pocket.slint` with no visible change (`Px`, `Tile`, `Step4`, `Step8`, `Outline`,
  `Small`, `Big`, `HKey`, `Title` (now with `gear`), `SurTop`, `KeyLabel`, `RoundKey` (now
  with `off`, `lit` and `s`), `Pill` (now with `off`)). The colours (`Pk`) moved to
  `common.slint` with `PkIcons` (pixel icons by name), `PkPx`, `PkTile` and `PkRing`, so the
  shared widgets can draw the LCD look.
- `ui/skins/px-pocket-pages.slint`: `ClipPage`, `SettingsPage`, `ModelsPage`, `MergePage`,
  `AskPage`, `Widget`, `Dialog`, `Speaking` and `Drop`, picked in `app.slint` by
  `Theme.pages == 7` (`Theme.pocket`). Every page is the handheld (`Shell`): the shell, the
  top edge without the Settings key, the surround and the page on the LCD, and the `Bezel`,
  which puts the keys exactly where the main page has them. The keys act: A does the page's
  main thing (Play or Pause on the clip page, Merge, Ask), B goes back ("Back to Clips" on
  the clip page, with the unsaved-changes question while editing), the pills are Rename and
  Delete on the clip page, and a key a page has no use for is faded, unlabelled and silent
  (`accessible-enabled` false). The D pad skips 5 s back or forward on the clip page (its left
  and right halves, "Back 5 seconds" and "Forward 5 seconds" for screen readers), as the arrow
  keys do there; it is faded elsewhere. A on Merge and Ask stays dim until there is a name or
  a question, and is held down while it runs. Merge and Ask lost their LCD Merge and Ask keys
  to A (Enter in the field still works); Cancel stays in the merge footer.
- A question (`Dialog`) and Speaking new words draw the bezel again over the page
  (`Bezel.cover`): A takes the answer's word and icon (a bin for Delete), B is Cancel, the
  rest are faded; the LCD under the window is a dither (`dimL`, `dimD`), the window has a dark
  title bar (a single clip's name in a light box), and its LCD keys carry the letters A and B.
  Drop to import is a dashed dither field in the LCD with the big import sprite.
- The shared widgets draw the LCD look when `Theme.pocket` is set, so the main page's menus
  and questions take it too: `Btn` (LCD key, a dark ring with a mid green foot; the main action
  dark, Delete with a light inner ring; held down while on), `IconBtn` (small key with a pixel
  icon from `PkIcons`), `Toggle` (the two cell OFF ON switch), `Select` (the dark picker; its
  list an LCD window, the current and hovered rows dark with the arrow cursor and a check),
  `MenuCard` (an LCD window with a dark ring and a mid green hard shadow), `MenuRow` and
  `ExportItem` (Jersey rows, dark with the arrow cursor under the pointer, the current one mid
  green; icons before the project actions and Delete), `MenuRule` (a dotted rule),
  `MenuGroup` (AUDIO and TRANSCRIPT on a mid green band, new `MenuBand`), `ShortcutBox` (small
  LCD key caps, PRESS THE KEYS on a dark tag), `ExportButton` (the dark key with the up arrow, a
  dithered bar while exporting, then a check and Done), `OrderList` (grip, the place on a dark
  tag, a play key, a pixel wave, the length and an x; the parts with their note and dots; a
  dragged row lifts dark with a hard shadow over a mid green frame), `TranscriptCard` (the key
  rail, TYPING and the change count as dark tags, a double edged text box and a block caret
  while editing, Jersey text, the spoken word dark), `Scroller.pk-rail` (a dither rail with a
  dark thumb), `Tip` (a dark tag with a light edge) and the gallery strips (`gallery.slint`:
  2px dark frames, the current tile double framed with its name on a dark tag and a corner
  flag, mid green for the focus, a dither scroll bar, no fades). `Theme.menu-h` is 22px for it.
- Rust: the editor's pause tags are the LCD's mid green (#4e6127).
- Art: 19 new pieces at 1x (pixel icons, the big drop sprite, the widget's 8px key and the
  two dims), 2.0 KB, from `tools/skin-art/px-pocket.html` (`sh
  tools/skin-art/cut.sh px-pocket OUT`, only the 1x cut is used); the old pieces cut again
  identically. No new fonts.

**Lessons from Pocket's other pages:**
- **One shell for every page:** the pages inherit a `Shell` that draws the handheld and takes
  the page as `@children` into its LCD, so each page only sets its key words (`KeySpec`) and
  callbacks. The bezel positions are copied from the main page, not shared with it, so the
  main page could not move; a question draws the same `Bezel` with `cover` over the page's
  keys, which also blocks them while it is up.
- **`accessible-role` must be a constant:** an off key keeps the button role and sets
  `accessible-enabled: false` and an empty label instead.
- **A hidden item in a layout still takes room, and a bound `width` pins it:** the menu's group
  band (`MenuBand`) takes no room in the other looks through `max-width: 0px`, and the plain
  group words take none in Pocket the same way; binding `width` to `self.preferred-width`
  on a Rectangle whose children are placed by `x` gave 0 (the shelf chips): use
  `preferred-width` and `min-width`, so a VerticalLayout can still stretch them.
- **Cannot set both `width` and `max-width`:** the dropdown's line icon gives Pocket's pixel
  arrow its room with `width: Theme.pocket ? 0 : ...`.
- **Preview first:** a small wrapper with sample state set in `init` and `changed state`,
  rendered with `slint-viewer --screenshot --load-data -I ui` on the hidden desktop, showed
  every page at both sizes in seconds before the build; the transcript editor's text needs the
  Rust markup callback, so it shows empty there.

**Budgets measured on Pocket's other pages** (test copy, 2.25x display, 10 s samples,
2026-10-31):
- CPU idle 0.0 to 0.3% on every page at 520x700 and 1100x700 (Ember 0.0 to 0.6%). Nothing on
  these pages moves on a clock.
- Working set within 3 MB of Ember on every page, mostly under it: at 520x700 main 37.7
  (38.2), one clip 160.4 (166.1), settings 54.1 (56.2), models 54.1 (56.3), Ask 66.1 (68.6), merge 67.9 (69.1),
  widget 61.1 (63.7) MB; at 1100x700 main 45.6 (46.7), one clip 171.5 (168.6), settings 63.0
  (64.2), models 65.3 (65.2), Ask 75.9 (76.9), merge 77.4 (78.5), widget 63.8 (64.7) MB.
- Exe +17.49 MB (136.41 to 153.90 MB): the nine pages' generated code and the LCD look in
  the shared widgets; art 2.0 KB, no fonts. About twice the Car Dashboard's (+8.77 MB): every
  page and every question carries the whole handheld and its bezel. A candidate for the size
  pass after the nine.
- Ember, Tape Deck and Phosphor: every page, both menus, the export menu, the questions,
  settings scrolled and the widget at 520x700 and 1100x700 match the build before pixel for
  pixel (38 shots each), apart from a blinking text cursor and a scroll bar caught mid fade.
  The Pocket main page matches too; its menus and questions take the new look.
- Not verified headless: tooltips, a real file drop (the cover was shown with
  `THRASHER_TEST_COVER`), speaking new words for real, a merged clip's parts, a download, Ask's
  Thinking (the answer comes at once), and CPU while a clip plays. The shared page script's
  "Menu for" key on the Pocket main page does not take a posted click (before this change too);
  `pocket.sh` in the step's kit opens the menus with a right click on the row instead.

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
- Its other pages are its own since 2026-10-31 (`Theme.pages` 10, see "Built in step 5,
  Answering Machine on every other page" below).
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

**Built in step 5, Answering Machine on every other page (2026-10-31):** mockups in
`docs/mockups/device-themes/dv-answer-pages/` (approved by Max 2026-10-31, widget variant A).
- `ui/skins/dv-answer-parts.slint`: the parts the main page and the other pages share, moved
  out of `dv-answer.slint` unchanged (`Art`, `Slice3`, `Lab`, `Seg`, `Nub`, `Title`, `Lamp`,
  `Display`, `Key`, `Micro`, `Grille` and the rest). New, with defaults that keep the main page
  as it was: `Title.page` (SETTINGS, MODELS, ASK after the badge) and `Title.settings` (the
  other pages leave the SET nub out, as the classic pages have no Settings button there),
  `Micro.turning`, `zoom` and `label` (the clip's name on the cassette), `Grille.broad`.
  The machine's colours (`Am`) moved to `common.slint`.
- `ui/skins/dv-answer-pages.slint`: `ClipPage`, `SettingsPage`, `ModelsPage`, `MergePage`,
  `AskPage`, `Widget`, `Dialog`, `Speaking` and `Drop`, picked in `app.slint` by
  `Theme.pages == 10` (Tape Deck 1, Phosphor 2, Desktop 95 3, Comic Book 4, Spatial Glass 5,
  Car Dashboard 6 as before). One clip plays back like a message: CLIPS, RENAME, DELETE and
  EXPORT as rubber keys, the smoked display (message number of how many, the name, when and how
  long, the counter in red segments with PLAY, STOP or EDIT), the tape as a waveform in a smoked
  well (two cream trim tabs, an amber play line, `TrimTouch` as before), the transport keys in a
  well with the speed chip and Loop as a slide switch (still the classic Loop button), Clean voice
  as a slide switch, and the transcript on ruled message paper. Wide puts the machine on the left
  with the micro cassette, the clip's name on its label, its reels turning while the clip plays.
  Settings, models and Ask are cards of message paper (three columns when wide); the models are
  tapes with LED ladders and rubber filter keys (latched with an amber lamp); merging is a short
  message log, the merged total on the display and, wide, the new clip as a large cassette; Ask
  is a cream label with the red ASK key and the answer and sources on paper. The widget is the
  tiny machine front; questions and covers are machine panels with a smoked title strip over a
  dark cover; messages are the smoked strip with a green lamp (on the main page too).
- `Theme.answer` (`Theme.pages == 10`) makes the shared widgets draw the machine: `Btn` and
  `IconBtn` are rubber keys and nubs (`AmKey`, red for the main action, held down while on),
  `Toggle` a slide switch with ON or OFF printed beside it, `Select` the cream picker chip
  (`AmChip`) with a paper list, `MenuCard`, `MenuRow`, `ExportItem`, `MenuRule` and `MenuGroup`
  paper cards with icons, the hovered row lit with a red edge (`Theme.menu-h` 28px), `Tip` a
  paper slip, `ExportButton` a rubber key, `OrderList` messages with the place on a small red
  display, `ShortcutBox` a smoked display (`AmSmoke`) that reads CTRL + ALT + R,
  `Scroller.rubber` a sunk track with a rubber thumb, `TranscriptCard` the ruled paper (Courier
  Prime, the word playing lit pale red, pause tags as grey stamps, a red edge and EDITING while
  typing), and the theme gallery strips (paper tiles, the current one framed red with its lamp,
  the count on a small red display, rubber nubs, a tape slider). New icons: `rew`, `ff`,
  `tape`, `warn`.
- Rust: the editor's pause tags are the machine's grey print (#86795f).
- Art: two new pieces from `tools/skin-art/dv-answer.html`, `mc-l` (the merge page's large
  cassette, its reels drawn in, the label blank) and `grille-m` (the speaker cut 366 wide),
  1x and 2x, 95 KB: `ONLY="mc-l grille-m" sh tools/skin-art/cut.sh dv-answer OUT`. The old
  pieces cut again match pixel for pixel. No new fonts.

**Lessons from the Answering Machine's other pages:**
- **Rounded gradients in caps** again, now shared: `AmFace` is a solid top cap, a solid bottom
  cap and a square gradient between them; `AmKey` (rubber or red key with its lip and shadow),
  `AmChip` and `AmSmoke` are built from it, so any width and height works without art.
- **A drop shadow drawn over the face:** a `drop-shadow` on a see-through box placed after the
  face filled under the box and greyed the panel. Put the shadow on a solid box drawn first.
- **`alignment: start` stops stretching:** in a row with `alignment: start` the models search
  pill (stretch 1) shrank to its smallest beside BACK, and the settings columns stayed at
  their preferred width in the Scroller. Leave alignment out where an item should fill.
- **A click blocker given as a child** lands in the component's layout when its `@children`
  sit in one; the panel holds its own `TouchArea` instead.
- **Fixed-width art does not stretch:** the speaker grille is a 256 wide cut built up in rows,
  so a wider well needs its own cut (`grille-m`); a cassette scaled 1.6x from the 2x cut was
  soft, so the merge page's large one is cut at its size (`mc-l`).
- **Previews:** `slint-viewer --screenshot` worked on the hidden desktop with the default
  backend; `--backend winit-software` failed there ("take_snapshot() called on window with
  invalid size"). A wrapper that sets `Main` and `Pages` in a `changed pg` handler showed every
  page in seconds while the shared target was busy.
- **Checks that touch clips:** saving an edited transcript speaks the new words and rewrites
  the clip's audio. A check that typed into the editor and then pressed a `Cancel` hit the
  wrong one, and the scratch copy of clip_100 was rewritten (restored from the scratch master).
  Leave the editor with "Discard changes", and compare the scratch clips' sizes after a run.
  A restored clip gets a new creation time and moves down the list, out of UI Automation's
  reach: drive clips at the top (clip_001, clip_006, Meeting).
- **Not verified headless:** tooltips, a real file drop, speaking new words, a merged clip's
  parts (scrolled out of reach), dragging an order row, Ask's THINKING and a model download.

**Budgets measured on the Answering Machine's other pages** (test copy, hidden desktop,
10 s samples, 2026-10-31; Ember on the same exe in brackets):
- CPU idle 0.0 to 0.8% on every page at 520x700 and 1100x700 (Ember 0.0 to 0.9%). Nothing moves
  on a clock but the cassette's reels while a clip plays on its page (the existing
  `RECORDING_FPS` 12 for the theme).
- Working set over Ember at 520x700: main +1.2, clip +0.3, settings +1.5, models +2.1, ask +1.9,
  merge +1.6, widget +1.4 MB; at 1100x700: +2.9, +4.3, +3.6, +4.0, +2.8, +5.6, +3.6 MB (clip
  page 77.7 vs 77.4 MB and 87.5 vs 83.2 MB).
- Exe +15.55 MB (136.41 to 151.96 MB): the pages' generated code and the machine look in the
  shared widgets (every `Btn`, `IconBtn` and `Toggle` carries an `AmKey` or a slide switch);
  art 95 KB, no new fonts. More than the batch 1 skins (+8.4 to +12.8 MB); the size pass
  should look at the key faces first.
- Ember, Tape Deck and Phosphor: every page, menu and question the scripts reach at 520x700
  and 1100x700 (38 shots each) match before and after pixel for pixel, but for the blinking
  cursor in the Ask and New project fields and pages scrolled by the wheel (settings, Phosphor's
  list), which land a few pixels apart from run to run. A second run at 520x700: Ember and Tape
  Deck 19 of 19 identical.

**Built in step 5, Pocket Synth main page (2026-10-30):** `ui/skins/dv-synth.slint` (skin 11,
theme "Pocket Synth", index 18 in `Theme.names`), art in `ui/skins/dv-synth/` (73 PNGs, 148 KB)
from `tools/skin-art/dv-synth.html` (`sh tools/skin-art/cut.sh dv-synth OUT`). Its other pages
are its own since 2026-10-31 (`Theme.pages` 11, see "Pocket Synth on every other page" below).
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
- `Main.list-cols` (4 for the wide pad bank, else 1; shared with Fighter Select) makes the arrow keys and `clamp-list` scroll by
  rows of pads; `AppWindow.move-selection` uses it. Up and Down still step one clip.
- Only the status square moves: it blinks while recording (`RECORDING_FPS` 4, `EFFECT_FPS` 0).
- Fonts: "Synth Figures", Doto 700 (OFL, no Reserved Font Name) as a static instance with only
  the timer's characters, 7 KB (`tools/skin-art/dv-synth-fonts.py`). Its 5 has a flat top and a
  square corner, so it does not read as S; its digits are all 0.6 em, so the timer holds still.
  The rest is Inter Tight and JetBrains Mono, already in the app (a 600 label asks for 700).

**Lessons from the Pocket Synth:**
- **A grid list needs its row count:** keyboard scrolling worked out `selected * row-h`, which
  is wrong for four clips to a row. `Main.list-cols` comes from `Theme.skin` and `Win.width`, not
  from the page, so no other skin has to reset it when the theme changes.
- **A pad in both directions:** its soft shadow and orange glow are nine-slice PNGs (outside the
  box only, as Spatial Glass); its body is drawn: a 2px lip under it, solid rounded caps as tall
  as the radius and a square gradient between them, the inner ring and the top light line. The
  fixed keys (function keys, row keys, the transport key) are whole PNGs with their shadows.
- **The pad bank's room comes from the scroller:** the pad pitch is its visible height over the
  rows shown, set from its `changed visible-height`, and `Main.row-h` follows it. The content
  rectangle sits in a `VerticalLayout` (alignment start) in the Flickable: placed pads alone
  leave the Flickable the height of its view, so the wheel did nothing and there was no bar
  (arrow keys still worked, as they set `list-y` directly). Review fix, 2026-10-30.
- **Nothing in the layout may be bound to the scroll position:** with "BANK A" bound to
  `Main.list-y`, the first wheel step panicked with "Recursion detected" (the Flickable reads
  the layout while it scrolls). `top-row` is now set in `changed` handlers. The bank keeps its
  last 8px for the scroll bar, so the bar does not run over the pads.
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

**Built in step 5, Pocket Synth on every other page (2026-10-31):** mockups in
`docs/mockups/device-themes/dv-synth-pages/` (approved by Max 2026-10-31, widget variant A,
Delete in the questions stays the dark key).
- `Theme.pages` is 11 for skin 11 (`Theme.synth`); every other value stays as it was.
  `app.slint` has a `Theme.pages == 11` block next to each Car Dashboard one, with the same
  bindings, and picks `ui/skins/dv-synth-pages.slint` (`ClipPage`, `SettingsPage`,
  `ModelsPage`, `MergePage`, `AskPage`, `Widget`, `Dialog`, `Speaking`, `Drop`).
- `ui/skins/dv-synth-parts.slint`: the parts the main page and the other pages share, moved out
  of `dv-synth.slint` unchanged (`Ps`, `Art`, `Slice3`, `Lcd`, `Glyph`, `Silk`, `TitleKey`,
  `Knob`, `Grille`, `Tag`, `Dots`, `Led`; `Title` gained `gear`, off on the other pages as on
  the classic ones), plus the pages' parts: `Key` (words in capitals, an icon before them),
  `IKey` (an icon and a printed number: 13 the transport key, 09 REC in the widget), `Latch`
  (LED and words: Loop, the model shelves, All projects), `Oled`, `State`, `Chip`, `PageStrip`
  (BACK or CLIPS, the page's name, a chip or SETUP) and `Segments` (LED bars).
- Every page sits on the brushed aluminium. The clip page is the sampler with the clip loaded:
  the mono screen holds its name, PLAY, PAUSE or STOP, the dot matrix counter, "of 2:03" and a
  pixel waveform (3px blocks, the played part white, the trimmed ends darker, orange trim
  brackets and play head; wide also the time at both ends), and the name field while renaming,
  with Cancel and Rename under it. Rename and Delete are keys on the page strip; the transport
  is back 5, key 13, forward 5, the speed as five LEDs and the LOOP latch. The transcript is a
  second screen under a rail of keys. Settings are panels with an orange square, small screens
  for the dropdowns and LED keys for the switches; wide puts the gallery across the top and the
  panels in three columns. Models are long keys with LED ladders, the one in use orange, the
  download on a screen strip; wide shows the shelves as latches and the DISK screen. Merge is a
  chain of long keys (place on a little screen, play key, round x), wide with the CHAIN screen
  (each clip a block by its length, the total) and the name on a screen strip. Ask types on a
  screen strip; the answer is on the mono screen, the sources long keys with their number on a
  little screen. Messages are a screen strip in the footer (clip, merge) or floating over the
  bottom (settings, models, Ask).
- The shared widgets draw the sampler when `Theme.synth` is set: `SynKey` (new, in
  `common.slint`: the key's body drawn, the soft shadow the main page's `pad-shade` nine-slice,
  `pad-glow` for a lit key) under `Btn` (capitals in JetBrains Mono, orange main action, `red`
  the dark key), `IconBtn` (a small key, round when round, orange while on), `Toggle` (a square
  key with an LED, with its words when it has a `caption`: Show marks, Clean voice),
  `SynScreen` (new: a black screen in its bezel and rim, drawn round its box) under `MenuCard`,
  the `Select` list and the transcript; `Select` (a small screen with an arrow), `MenuRow`
  (28px rows, `Theme.menu-h`; mono words, a dark line under the pointer, the open row and the
  current project orange, projects numbered 01 02 by the new `num`, icons before the actions),
  `MenuRule`, `MenuGroup`, `ExportItem` (tags in capitals, the current format an orange
  square), `ExportButton` (the orange key, a dark key filling orange under the percent, DONE),
  `OrderList` (long keys in a chain), `ShortcutBox` (key caps, a screen strip while it
  listens), `TranscriptCard` (mono words, the playing word orange, an orange ring and block
  cursor while editing, the change count on a screen chip, the head line ORIGINAL, EDIT,
  SUMMARY, TRANSLATION, EMPTY or WORKING), `Tip` (a tiny screen label), the orange scroll
  thumb, and the gallery (`gallery.slint`: tiles as pads, the current one orange and glowing,
  an orange ring on the focused one, a row of LEDs for where the strip is).
- Rust: the editor's pause tags are the screens' orange (#ff5a1e).
- No new art or fonts: the pages reuse the main page's PNGs (pad shade and glow, LEDs, brush,
  screws, grille) and its fonts.

**Lessons from the Pocket Synth's pages:**
- **Draw the keys, do not cut them:** every key size and colour would have been a set of
  slices. `SynKey` draws the body (caps and gradient, as the pads) and reuses the main page's
  pad shadow and glow as nine-slice pictures (22 and 30 px borders fit keys from 20 px high),
  so one component serves the text keys, the icon keys, the LED keys, the model rows and the
  gallery tiles. `SynScreen` draws the bezel and rim round its box the same way; a popup may
  draw it inside its own edge (`MenuCard` insets it 4px).
- **A pixel waveform without a rectangle per block:** each column is one bar on the 4px cell
  grid, and a dark 1px row every 4px across the strip cuts the bars into blocks (about 90
  bars and 20 rows instead of a thousand blocks).
- **Change shared files only after the old build has read them:** the "before" build waited
  over an hour on the shared target's lock, so the work went into a copy of `ui/` checked with
  `slint-viewer --check` and shot off screen with a preview wrapper (`--load-data`), and was
  copied back once the old build's build script had finished (the thrasher rustc running).
- **Components must be declared before use in a file,** and children of a component that
  inherits another go to the base's `@children` (so `FilesCard inherits Card { SRow {} }`
  works).
- **`clip` is a property of every element:** a component's own `in property <string> clip`
  fails ("Cannot override property"); name it otherwise.
- **The brushed tile can show a seam** where the renderer redrew only part of the window (the
  3px stripes do not line up across the edge of the redrawn area); it is the main page's tile,
  seen there too.
- **Checks:** a pad of the wide main page plays its clip on a plain click, so picking two clips
  for a merge is a press on one and a Ctrl press on the other in view (clip_105 and clip_106:
  the bank shows eight pads and only those have a menu key). Two runs of the old build also
  differ in the settings shots scrolled by the wheel (a fractional offset) and in the blinking
  caret, so those are not counted as changes.
- **A Text with only `accessible-role: none` is still read out** (UI Automation lists it as a
  Custom item named by its words, so every key said CLIPS or DOWNLOAD twice). `Caps` also sets
  `accessible-label: ""`, as `Silk` does; a word that classic shows as text (IN USE) sets its
  own role and name. The Ask page's ALL PROJECTS key is a check box, as on the classic page.
- Review (2026-10-31): Desktop 95, Comic Book, Spatial Glass, Car Dashboard and Pinball pages at
  520x700 match before and after (but for the caret, the wheel and a live level). A merged
  clip's parts, its part menu and tooltips were checked headless. Ember's working set rose
  about 2 MB on every page (the sampler keys sit hidden in the shared widgets).

**Budgets measured on the Pocket Synth's pages** (test copy, 2.25x display, 10 s samples, pages
visited in turn in one run, Ember on the same exe, 2026-10-31):
- CPU idle 0.0 to 0.8% on every page at 520x700 and 1100x700 (Ember 0.0 to 0.5%). Only the
  widget's state square blinks, while recording.
- Working set over Ember at 520x700: main +2.1, clip -7.3, settings +1.2, models +1.9, ask
  +2.0, merge +2.5, widget +2.0 MB; at 1100x700: +1.7, +1.3, +0.4, +2.3, +2.2, +5.2, +5.3 MB.
- Exe +16.1 MB (136.4 to 152.5 MB): the pages' generated code and the sampler look in the
  shared widgets (keys drawn per instance); no new art or fonts.
- Ember, Tape Deck and Phosphor: every page, menu, dropdown and question at 520x700 and
  1100x700 (42 shots each) match before and after pixel for pixel, but for the caret and the
  wheel-scrolled settings shots, which differ the same way between two runs of the old build.
  The Pocket Synth main page differs only by its scroll thumb, now orange.
- Not verified headless: a model download and the AI download, dropping files (shown with
  THRASHER_TEST_COVER), the answer while thinking.
**Built in step 5, Fighter Select main page (2026-10-30):** `ui/skins/x-fighter.slint` (skin 12,
theme "Fighter Select", index 19 in `Theme.names`, after Pocket Synth), art in
`ui/skins/x-fighter/` (12 PNGs, 0.10 MB) from `tools/skin-art/x-fighter.html`
(`sh tools/skin-art/cut.sh x-fighter OUT`). Its other pages are its own since 2026-11-01
(`Theme.pages` 12, see "Built in step 5, Fighter Select on every other page" below).
- Settings gallery: its tile is the Fighter Select line in `Gallery.devices` (`ui/gallery.slint`), band #d63a2a.
- The recorder is P1's HUD: the microphone, the health bar (the live level in gold over a red
  peak that falls behind it, -60 to 0 dB), the round clock and READY (RECORDING, PAUSED), and
  FIGHT, the turned record diamond (a gold STOP while recording). Wide windows make it the fight
  stage on the left, with a fighter, the floor and a faint VS. The Stage chip is the project
  picker (its STAGE word is left out when the toolbar is squeezed); while recording, Import is
  PAUSE and RESUME (Import is not offered then).
- The clips are a select grid of fighter tiles: the mockup's eight stick fighters and backdrops by
  clip number, TRANSCRIBED, play, the name plate (the name on one or two lines, when it was made
  with "+ system", on a line of its own when the tile is narrow, the waveform, the length, the
  menu). The highlighted clip has the P1 cursor;
  it and the picked ones have a red frame and plate. Columns as fit tiles of about 86 px
  (portrait) or 130 px (wide), rows as tall as fill the view. Up and Down still step one clip.
- The footer is REPLAY with a four segment super meter; LV is how many segments are full.
- Only the P1 cursor and the status dot move, blinking while recording (`RECORDING_FPS` 4).
- Fonts (48 KB): Anton (OFL, no Reserved Font Name, Latin subset; its 5 has a flat top and a
  square corner), "Fighter Oblique" (Anton's ASCII leaned 10 degrees) and "Fighter Figures" (the
  timer's characters with the 1 widened to the other digits), all from
  `python tools/skin-art/x-fighter-fonts.py SRC OUT`; Chakra Petch 600 and 700 come from the
  Brick Phone.
- Shared, no visible change elsewhere: `Main.grid-cols` and `Main.list-cols` (clips per row of
  the list, 1 for every skin but this one; `move-selection` and `clamp-list` scroll by rows) and
  `TextField.bare-focus` (no accent edge or ring; the skin's own frame shows the focus).

**Lessons from Fighter Select:**
- **What changes while recording must stay out of the page's layout.** The clock, the status
  word and the dB label sat in layouts beside the grid; every tick their bindings marked the
  page's layout dirty (Slint passes the change on without comparing values), so every tile's
  paths were drawn again: recording took 80% of a core at 1100x700. Fixed sizes on the texts
  and `min-height`/`preferred-height` on the panel did not help. Drawing the HUD and the stage
  over empty slots of the layout (siblings after it, placed by `x`/`y`) brought it to 8.9%.
  A width that must follow a rare change (the timer passing an hour) is set in a `changed`
  handler only when it differs, not bound.
- **`Path { fit: preserve; }` takes its commands in px**, with no scaling and no shrinking by the
  stroke. Skewed boxes of any width (chips, keys, the bars) keep the mockup's angle this way,
  with commands built from the box's size; the health bar's fill is a band worked out from the
  level (`F.band`).
- **Slanted type is a font:** Slint cannot skew text, so Fighter Oblique leans Anton's outlines
  by tan(10 degrees) with fontTools (`TransformPen` into `TTGlyphPen`, composites decomposed).
  READY, RECORDING, the logo and CLIP SELECT stay live text instead of art.
- **No smooth scaling in the software renderer** (Slint 1.18 picks the nearest pixel), so the
  tile fighters are paths on the mockup's 100 unit box (a glow, a dark body, the head, an
  extra), not pictures that would alias at every tile size. Their glow is the accent mixed
  with the backdrop, as one opaque gradient, so crossing strokes do not brighten as two
  see-through paths would.
- **`max-height` did not stop a wrapping Text** in a VerticalLayout from taking three lines: the
  plate measures the name on one line in a hidden Text and gives it one or two lines' height.
- **Checks:** `SLINT_DEBUG_PERFORMANCE=refresh_lazy,console` prints frames per second to the
  trace: every skin draws about 60 frames a second while recording, so what costs is how much
  each frame covers. A temporary bit mask from an environment variable that hides parts of
  the page found the cost in one build.

**Budgets measured on Fighter Select** (test copy, 2.25x display, 10 s samples, 2026-10-30):
- CPU idle 0.2% at 520x700 and 0.0% at 1100x700 (Ember 0.0% and 0.2%); recording 7.8% and 8.9%
  (Ember 14.4% and 20.9%).
- Working set idle 45.6 vs Ember 41.2 MB (+4.4), wide 54.2 vs 49.0 MB (+5.2); recording 49.5 vs
  44.5 MB, wide 58.0 vs 52.5 MB.
- Exe +2.33 MB (65.61 to 67.94 MB): fonts 48 KB, art 0.10 MB, the rest generated code. Ember,
  Tape Deck, Phosphor and Pocket main pages at 520x700, 420x560, 700x700 and 1100x700 match
  before and after.

**Built in step 5, Fighter Select on every other page (2026-11-01):** mockups in
`docs/mockups/device-themes/x-fighter-pages/` (approved by Max 2026-10-31, widget variant A,
flavour labels kept: PROFILE, OPTIONS, ROSTER, TAG TEAM, STORY MODE, STAGE SELECT, K.O., WAIT,
NEW STAGE, LOADING, NEW CHALLENGER).
- `Theme.pages` is 12 for skin 12 (`pages: skin <= 12 ? skin : 0`, still one comparison) and
  `Theme.fighter` is the flag the shared widgets read. `app.slint` has a `Theme.pages == 12`
  block next to each Pocket Synth one and picks `ui/skins/x-fighter-pages.slint` (`ClipPage`,
  `SettingsPage`, `ModelsPage`, `MergePage`, `AskPage`, `Widget`, `Dialog`, `Speaking`, `Drop`).
- `ui/skins/x-fighter-parts.slint`: the parts the main page and the other pages share, moved
  out of `x-fighter.slint` unchanged (`F`, `G`, `Glyph`, `Skew`, `Key`, `Label`, `TitleKey`,
  `Health`, `P1Tag`). The fighters (`Pose`, `Fighter`) moved to `common.slint`, as the merge
  rows draw them too, with `FiPortrait` (a fighter in its backdrop, cut to its box).
- `common.slint` gains the `Fi` global (colours, fonts, the lit red plate), `FiSkew` (a
  slanted box in px), `FiKey` (a slanted key: dark, gold, red, cyan or gold rimmed),
  `FiSwitch` (the two cell switch), `FiExportArt` (EXPORT, the gold bar filling under the
  percent, DONE) and `FiOrderRow` (grip, red place tag, name plate with the fighter, play key,
  length, x or dots). Each is one skin-only component placed inside the shared widget's own
  type (`if Theme.fighter: FiKey { }`); the rest of the look is one more branch in the
  existing chains: `Btn` (Anton capitals on a slanted key, `red` the red one), `IconBtn`,
  `Toggle`, `Select` (the Stage chip's face with the gold arrow, its list a dark panel with a
  red top edge, the current choice on the red plate with a gold edge, opened flush right so it
  stays in the window), `MenuCard`, `MenuRow` (projects in Fighter Oblique capitals, the open
  row and the current project on the red plate, icons before the project actions),
  `ProjectMenu` (STAGE SELECT over it), `ExportItem`, `MenuRule`, `MenuGroup`, `ShortcutBox`
  (white slanted key caps, Press the keys in gold by a red square), `Tip` (black, framed gold),
  `TranscriptCard` (Chakra Petch, the playing word on gold, pause tags cyan, the change count
  on a red tag, EDIT on a slanted key, No transcript yet and Transcribing in the slanted type
  over a health bar), the scroll thumb (red) and the gallery (`gallery.slint`: fighter tiles in
  select rows, the current one red framed with the P1 cursor and tag, gold when focused, the
  heads in the slanted type with a red rule, a red scroll bar).
- The pages draw the rest: the title bar with the gold Back key, tagged panels (red tags, gold
  for STAGE, OPTIONS and NEW CLIP, gold frame and tag while editing), the clip's profile
  (portrait in a red frame, the name in the slanted type, the counter in Fighter Figures gold),
  the stage waveform (gold played part, gold trim brackets, red head), slanted speed cells,
  the roster with quality and speed as slanted pips and a health bar while downloading, the tag
  team cards joined by gold plus signs (wide), the answer in a dialogue box with a portrait,
  announcer panels for the questions and the speaking cover, and the drop cover with NEW
  CHALLENGER. Messages are dark plates with a gold edge.
- The widget (variant A): the main page's FIGHT diamond (its `rec-p`/`stop-p` art), the status
  word and dot, the round clock and a health bar, Pause while recording, Open window; the top
  edge red while recording.
- Rust: the editor's pause tags are the cyan of the tags (#27e0ff).
- No new art or fonts: the pages reuse the main page's FIGHT art and its four fonts; everything
  else is drawn.

**Mockup faults fixed, not copied (Fighter Select pages):** the portrait clip page did not fit
700px with a merged clip's parts, so the transcript panel shrinks (to 60px) and cuts its words
to the panel; the wide profile's counter sits beside OF 2:03 instead of under the name so the
stage heading is not covered; a short dropdown list opens flush with the chip's right edge,
not past the window. Deliberate differences: the merge rows show the fighter of their place,
not of the clip's tile; the widget's Open window key is the shared expand icon; the Ask key is
a cyan slanted key without the dim state's own colour (it fades when off, as every key).
Review fixes (2026-11-01): the translate menu's INSTALL is the mockup's cyan tag (it was the
classic red word); on the wide clip page the counter sits in a start aligned row (its fixed
width capped the name column at about 170px, so "clip_101 merged" and the file name were cut
short and RENAME and DELETE sat mid panel), and a name over 10 characters steps down from 40px
to 30px. Lesson: a child with a fixed `width` in a layout caps the whole layout's width.

**Lessons from the Fighter Select pages:**
- **An `if` whose root is a component copies that component.** `if !root.wide: FilesCard { }`
  made a second FilesCard inside the `if` (266 KB of `app.rs`), as did the cards, the parts,
  the answer and the sources. Wrapping them, `if !root.wide: VerticalLayout { FilesCard { } }`,
  keeps one shared type; with that and the next point the pages went from +17.0 MB of
  `app.rs` (exe +5.98 MB) to +13.6 MB (exe +4.74 MB).
- **`component Transcript inherits TranscriptCard` copies the whole transcript card** (its
  popups and loops, about 0.8 MB of `app.rs`). The pages use `TranscriptCard { }` with the
  bindings at each use instead. The other skins still inherit it; changing them would save the
  same again per skin.
- **A panel used with children is copied into every use,** so its tag is `visible:` rather
  than `if` (about 1 KB instead of 15 KB per panel).
- **A chain edit can move a classic look:** giving the classic ShortcutBox's "Not set" an
  explicit size and alignment moved it in Ember and Tape Deck; Fighter Select got its own
  Text instead. Comparing the classic settings shots before and after caught it.
- **`slint-viewer --check app.slint` and `--screenshot`** of a preview wrapper (sample state
  through `--load-data`, run on the hidden desktop) check and show every page in seconds, so
  only three release builds (about 30 minutes each) were needed.
- **Checks:** the test build has no transcription and no local AI, so the models page and Ask
  were checked in the previews only; the clip list here has no clip_005, so the state script
  used clip_100. A hidden desktop run started with a plain `&` from the agent's shell was cut
  off after about 19 minutes; long runs go through the tool's own background mode.

**Budgets measured on the Fighter Select pages** (test copy, 2.25x display, 10 s samples, pages
visited in turn in one run, Ember on the same exe, 2026-11-01):
- CPU idle 0.0 to 0.5% on every page at 520x700 and 1100x700 (Ember 0.0 to 0.6%).
- Working set over Ember at 520x700: main +2.7, clip -6.0, settings -1.2, merge 0.0, widget
  -1.2 MB; at 1100x700: +2.3, -0.1, +3.0, +4.1, -0.2 MB.
- Exe +4.74 MB (106.12 to 110.85 MB), `app.rs` +13.6 MB (187.9 to 201.5 MB); no new art or
  fonts. The release rustc of the UI peaked at 26.7 to 28.2 GB.
- Ember, Tape Deck and Pocket Synth: every page, menu, dropdown and question at 520x700 and
  1100x700 (38 shots each) match the main build pixel for pixel. The Fighter Select main page
  differs only by its scroll thumb, now red, and its menus.
- Not verified in the app: the models page and Ask (no transcription or AI in the test build;
  previews only), a merged clip's parts (the test clips carry none; previews only), a model
  download, the answer while thinking.

**Built in step 5, Cozy Cabin main page (2026-10-30):** `ui/skins/px-cozy.slint` (skin 13, theme
"Cozy Cabin", index 20 in `Theme.names`, after Fighter Select; `fill_gallery` skips empty names,
should a place ever be held for a skin not in yet), art in `ui/skins/px-cozy/` (35 PNGs at 1x, 18 KB)
from `tools/skin-art/px-cozy.html` (`sh tools/skin-art/cut.sh px-cozy OUT`, only the 1x cut is
used). Its other pages were the classic ones in a wood and honey palette until step 5 below gave them their own.
- Settings gallery: its tile is the Cozy Cabin line in `Gallery.devices` (`ui/gallery.slint`), band #c9822e.
- The recorder is the cabin wall: the little window (dusk pines and fireflies), the hanging lantern
  with its tag (Record, "Stop and save" while recording; the tag says REC or STOP) and the
  parchment readout (status, microphone, timer, meter). Wide windows put it on the left with the
  cabin shelf; under 490 px of body height the left side takes the portrait's smaller window,
  lantern and readout. While recording, Import in the toolbar is Pause and Resume.
- The clips are inventory slots; the highlighted one is parchment with a honey frame and a honey
  nub at the list's edge, other picked ones parchment. Rows show "transcribed" with the scroll
  icon and "+ system". The waveform drops to 5 bars, then none, as a row narrows.
- Motion only while recording (`RECORDING_FPS` 8): the window scene is a 24 frame strip (stars and
  fireflies on whole turns, so it loops), the lit lantern a 4 frame flame strip with the glow's
  opacity stepping with it, and the status dot blinks. At rest, and paused, nothing moves.
- Fonts: Jersey 10 and Pixel 95 were already in; Tiny5 400 (the small capitals; the variable
  font's default instance cut to capitals, so it has no 5) and Cozy Figures (Jersey 10's timer
  characters with every digit as wide as the widest, 2 KB), built by `tools/skin-art/px-cozy-fonts.py`.

**Lessons from the Cozy Cabin:**
- **Index gaps:** a skin built beside others can take a later index; while it is on its own the
  places between get empty names and Ember's palette, and the gallery skips empty names, so
  nothing shows for them. The merge filled 18 and 19 with Pocket Synth and Fighter Select.
- **`1phx` is not allowed in a global** (the scale factor is not known there): a whole device
  pixel art size (`max(1, round(3 * 1px / 1phx)) * 1phx`) lives in the component that draws.
- **Looping scenes:** give every sine in the mockup's scene a whole number of turns over the
  strip, so the last frame runs into the first; fill the canvas colour first, or the gap between
  the sky bands and the hills cuts as transparent.
- **CSS box shadows as an outline:** the mockup's 3px `.pb` ring is four rectangles outside the
  box (`Outline`, drawn by the box itself, children may draw outside their parent); inset bevels
  are four bands with the top left on top, as CSS layers the first shadow over the second.
- **A layout item needs a height:** a `HorizontalLayout` placed by x and y with no height takes
  its parent's, so a `VerticalLayout { alignment: center }` inside it put the status dot and the
  mic icon in the middle of the card.
- **Heights that hold pixel art grow with it:** art on whole device pixels is about 4% larger at
  2.25x (and 11% at 1.5x), so the portrait recorder's fixed 243 px let the toolbar cover the
  readout's foot; its height now follows the window art (review, 2026-10-30).
- **TextField is 36 px tall by default:** in a 30 px well its text sat 3 px low until it got
  `height: 30px`.
- **Hidden measuring copies:** an invisible `AllBox` used to measure the toolbar's room still
  showed in UI Automation as a second "Search all projects" checkbox, and a press landed on it;
  measure with a plain silent `Text` instead.
- **Checks:** the shared scratch clips gain a clip per recording; clean them before the
  before/after comparison, or the other themes differ by a row (their scrollbar). A long project
  folder with `.thrasher-project` and a `<name>.system.wav` were staged and removed after.

**Budgets measured on the Cozy Cabin** (test copy, 2.25x display, 10 s samples, 2026-10-30):
- CPU idle 0.3% at 520x700 and 0.0% at 1100x700 (Ember 0.3% and 0.3%); recording 5.6% and 10.8%
  (Ember 16.9% and 19.7%).
- Working set idle 43.6 vs Ember 41.3 MB (+2.3), wide 51.4 vs 49.5 MB (+1.9); recording 46.2 vs
  44.3 MB, wide 54.2 vs 52.5 MB.
- Exe +1.77 MB (65.61 to 67.38 MB): art 18 KB, fonts 8 KB, the rest generated code. Ember, Tape
  Deck, Phosphor and Pocket main pages at 520x700, 420x560, 700x700 and 1100x700 match pixel for
  pixel before and after.

**Built in step 5, Cozy Cabin on every other page (2026-11-02):** mockups in
`docs/mockups/device-themes/px-cozy-pages/` (approved by Max 2026-10-31, widget variant A,
flavour labels kept: WRITING, THE PANTRY, FIND A MODEL, WORKBENCH, NEW CLIP, ORDER).
- `Theme.pages` is 13 for skin 13 (`pages: skin <= 13 ? skin : 0`, still one comparison) and
  `Theme.cozy` is the flag the shared widgets read. `app.slint` has a `Theme.pages == 13`
  block next to each Fighter Select one and picks `ui/skins/px-cozy-pages.slint` (`ClipPage`,
  `SettingsPage`, `ModelsPage`, `MergePage`, `AskPage`, `Widget`, `Dialog`, `Speaking`, `Drop`).
  No parts file: the pages draw their own small parts and the main page is untouched.
- `common.slint` gains the `Cz` global (colours, fonts, the key faces by kind), `CzBevel` (the
  3px bevel), `CzKey` (a key in its ink ring: wood, honey, ember, parchment or dark wood),
  `CzNote` (a parchment note in its ink ring, honey or ember ringed), `CzWell` (a dark well),
  `CzBar` (9px honey blocks with 3px gaps), `CzSwitch` (the wooden latch on moss), `CzRule`
  (the dotted rule, a tile), `CzExportArt` (EXPORT, the blocks under the percent, DONE),
  `CzSlotKey` and `CzOrderRow` (inventory slot: grip, honey place tile, play slot, name,
  length in Jersey, a small x or the dots). Each is placed inside the shared widget's own
  type (`if Theme.cozy: CzKey { }`); the rest is one more branch in the existing chains:
  `Btn` (capitals, parchment unless main, red or on), `IconBtn`, `Toggle`, `Select` (the
  Inbox parchment picker, its list a parchment note, the current choice honey with an ember
  edge, opened flush right), `MenuCard` (parchment note with a hard shadow), `MenuRow`,
  `ExportItem`, `MenuRule` (dotted), `MenuGroup` (brown capitals), `ShortcutBox` (parchment
  key caps, PRESS THE KEYS on a honey key, Not set in brown), `Tip` (pale parchment in an ink
  ring), `TranscriptCard` (Pixel 95 ink on the letter, the spoken word on honey, pause tags
  and spoken words in moss ink, WRITING count on an ember tag, INSTALL on a wood tag, No
  transcript yet and Transcribing in Jersey over the block bar), the scroll thumb
  (parchment brown) and the gallery (`gallery.slint`: tiles on a dark shelf in an ink ring,
  the current one ringed honey with a honey nub and its name on a honey label, focused pale
  parchment, the heads in brown capitals). The main page's menus take the look too.
- The pages draw the rest: the plank wall, the dark plank title bar (flame, name with its
  ink shadow, the wooden CLIPS key on the clip page), the honey headings with the wooden
  Back key, parchment cards pinned with ember pins under wooden plaques, the groove
  waveform with nailed wooden trim tabs, plank speed cells, the pantry's inventory slots
  with honey pips (ember on the model in use, a moss IN USE tag), the workbench (wide) with
  the tapes in numbered slots, the answer as a letter, the sources as slots with ember
  numbers and moss project tags, pinned parchment questions over the dimmed cabin, the
  speaking card, and the drop cover (dashed honey frame, a wooden crate, DROP TO IMPORT).
  Messages are dark plank plaques with two nails; the status sits in a dark well.
- The widget (variant A): the main page's lantern (unlit, lit, and its four frame flame
  strip while recording, on the Fx clock) on a plank with its REC or STOP tag, beside a
  pinned readout with the status in the small capitals, Open window, the timer in Cozy
  Figures and the level as moss blocks; Pause by the level while recording; an ember frame
  round the widget while recording.
- Rust: the editor's pause tags are moss ink (#4c6c20).
- New art via `tools/skin-art/px-cozy.html` (`ONLY="up left x5 pencil bin grip rule dash
  vdash" sh tools/skin-art/cut.sh px-cozy OUT`, the 1x cut used): Export's arrow, the Back
  arrow, a small x, pencil, bin, grip, and the dotted rule and dash tiles. No new fonts.

**Mockup faults fixed, not copied (Cozy Cabin pages):** the dropdowns size to their choice
(the mockup's Export format and Stop recording after chips cut their words), the clip name on
the delete question sits on a label as wide as the name, the merge sum elides instead of
running out of the workbench, and the portrait pantry scrolls inside its box instead of
cutting the last slot. Deliberate differences: the skip keys carry the shared rewind and
forward glyphs; the workbench shows the parts' lengths, not their total (the page has no
total); the answer is plain text, so its footnotes are not ember squares; the order rows have
no little waveform (an order row carries none); the letter has no ruled lines; All projects
on Ask is the main page's check well with the same name and role as before.

**Lessons from the Cozy Cabin pages:**
- **List a new skin beside the last one in the shared widgets with a script, then hand edit
  its own values.** `Theme.cozy` was added next to every `Theme.fighter` in an OR list (radius
  0, no border, transparent, the classic part hidden) by a small script, and every value that
  was Fighter Select's own (`Theme.fighter ? X :`) got its own cozy branch by hand.
- **A font cut to capitals needs capitals.** Tiny5 has no lower case here, so the menu group
  heads (Audio, Transcript) are put in capitals for the skin.
- **`visible` on a component instance copies it too**, like an `if`: the parchment note inside
  `MenuCard` and the panels is placed in a plain box that carries the `visible`.
- **The base build must not see the new Rust.** With `main.rs` already calling
  `get_cozy()`, the base build failed after its UI compile; the work was stashed for it.
- **Git Bash rewrites arguments that start with `//`** for Windows programs (a Slint comment
  passed to a Python script lost a slash); `MSYS_NO_PATHCONV=1` keeps them.
- **Checks:** the portrait main page shows six rows, so the drive uses clip_005 and clip_006
  at 520 wide (clip_105 is out of view there).
- **The release rustc of the UI now peaks at 29.7 GB**, a hair under the 30 GB stop. The next
  skin's pages will likely pass it: a size pass (fewer copied panels, the other skins'
  `inherits TranscriptCard`) should come before them.

**Budgets measured on the Cozy Cabin pages** (test copy, 2.25x display, 10 s samples, pages
visited in turn in one run, Ember on the same exe, 2026-11-02):
- CPU idle 0.0 to 0.5% on every page at 520x700 and 1100x700 (Ember 0.0 to 0.3%).
- Working set against Ember at 520x700: main -1.8, clip -8.7, settings -3.1, merge -2.9, widget
  -2.4 MB; at 1100x700: -1.2, -4.1, -1.3, -0.8, -1.2 MB.
- Exe +5.01 MB (110.86 to 115.87 MB), `app.rs` +14.6 MB (201.5 to 216.1 MB); art 1 KB (nine
  pieces), no new fonts. The release rustc of the UI peaked at 29.7 to 29.8 GB.
- Ember, Tape Deck and Pocket Synth: every page, menu, dropdown and question at 520x700 and
  1100x700 (38 shots each) match the Fighter Select build pixel for pixel.
- Not verified in the app: the models page and Ask (no transcription or AI in the test build;
  previews only), a merged clip's parts (the test clips carry none; previews only), a model
  download, the answer while thinking, a real file drop (the cover is shown with
  `THRASHER_TEST_COVER`), tooltips, dragging an order row.

**Built in step 5, Pinball on every other page (2026-11-02):** mockups in
`docs/mockups/device-themes/x-pinball-pages/` (approved by Max 2026-10-31, widget variant A,
flavour labels kept: MULTIBALL, WIZARD MODE, DRAIN, TILT, NEW GAME, BONUS, SHELF, DISK, BALLS
IN PLAY).
- `Theme.pages` is 14 for skin 14 (`pages: skin <= 14 ? skin : 0`, still one comparison) and
  `Theme.pinball` is the flag the shared widgets read. `app.slint` has a `Theme.pages == 14`
  block next to each Cozy Cabin one and picks `ui/skins/x-pinball-pages.slint` (`ClipPage`,
  `SettingsPage`, `ModelsPage`, `MergePage`, `AskPage`, `Widget`, `Dialog`, `Speaking`, `Drop`).
- `ui/skins/x-pinball-parts.slint`: the main page's `Pb` colours, `Art`, `Lab`, `DotText` and
  `DotGrid`, moved out of `x-pinball.slint` unchanged and exported, plus `Lamp` (a playfield
  lamp, off, orange, yellow or red), `Lanes` (rollover lanes lit up to a value), `Insert` (an
  arrow insert in its colour or lit), `Dots` (a word in the dot font) and `DotBox` (a word on a
  small dot matrix). It imports nothing, so `common.slint` imports it too.
- `common.slint` gains `PbKey` (the lit plasma start button, the dark chrome plate, red, lit
  plate, green), `PbRing` (a round chrome ring, plasma ringed when on), `PbSwitch` (a dark slot
  with a lamp sliding right and lighting), `PbExportArt` (EXPORT, the ball running along a rail
  with the percent, DONE in green), `PbOrderRow` (grip, numbered arrow insert, play ring, name
  with its length under it, an x or the dots), `PbXKey` and `PbShortcutArt` (PRESS THE KEYS on
  a small dot matrix, NOT SET on a dark plate, plasma key caps joined by plus signs). Each is
  placed inside the shared widget's own type (`if Theme.pinball: PbKey { }`). The rest is one
  more branch in the existing chains: `Btn`, `IconBtn`, `Toggle`, `Select` (a dark well with a
  chrome edge and a plasma chevron, its list a dark plate with a plasma top edge, the current
  and the hovered choice lit plasma with an edge, opened flush right), `MenuCard`, `MenuRow`
  (the hovered and the open row lit plasma), `ExportItem`, `MenuRule`, `MenuGroup` (blue
  capitals), `Tip` (a black plate with a plasma top line), `TranscriptCard` (Barlow Condensed,
  played words brighter, the spoken word on a plasma plate, pause tags as blue mono chips, the
  change count on a plasma tag, EDIT on a chrome plate, TRANSCRIBING in the dot font over the
  lane lamps), the scroll bar (a wire rail with the ball as its thumb) and the gallery
  (`gallery.slint`: the Ember tile, square, with plasma heads and the ball on its rail). The
  main page's menus and list take the look too.
- The pages draw the rest: the title bar with the neon brand and, on the clip page, the lit
  CLIPS insert; slim backglass bands with the lit Back insert for Settings, Models and Ask; the
  clip's backglass (PLAYING, PAUSED or STOPPED with OF the length on the dot matrix, the time
  big, wide with the speakers and a dotted line of where the ball is); the nameplate with
  Rename and Delete as yellow and red drop targets; the waveform as a wire ramp with chrome trim
  posts and the ball on the play head; the transport on the playfield (the red flipper button,
  chrome rings for 5 s with their 5, the speed as five rollover lanes, Loop with a lamp; wide
  with the guides and flippers); instruction cards with screws for the transcript, the parts,
  the settings (their names on a small dot matrix), the order, the answer and the sources; the
  parts catalogue with lamp pips (wide on the playfield with the shelves as a lit column and
  the disk on a big dot matrix); MULTIBALL with the ball count and the total, and the balls'
  shares of the length in insert colours (wide); WIZARD MODE; small backglass sheets for the
  questions and Speaking; the drop cover (chrome rails, a lit arrow, the red Import bumper,
  DROP TO IMPORT on the dot matrix). Messages run on a dot matrix strip in the apron (clip,
  settings, merge), or float on a small one (models, Ask).
- The widget (variant A, the mini backglass): READY, REC or PAUSED with the blinking lamp and
  the timer big on the dot matrix, the input level on eight lane lamps, the red plunger knob
  (REC, STOP while recording), Pause by the lanes while recording, Open window top right; the
  frame red while recording.
- Rust: the editor's pause tags are the blue of the mark chips (#8fb4ff).
- New art via `tools/skin-art/x-pinball.html` (`ONLY="back-w" sh tools/skin-art/cut.sh
  x-pinball OUT`): back-w, the lit insert pointing left (1x and 2x, 13 KB). No new fonts.

**Mockup faults fixed, not copied (Pinball pages):** the wide clip page's Export sat in the
middle of the apron; it is flush right, after a dot matrix strip that takes the rest. The
portrait main-page overlaps of the chrome mockup are popups over the page in the app, which
places them itself. Small fixes: the order and source inserts carry their numbers, the wide
speakers sit inside the backglass, the sources card is only as tall as its rows. Deliberate
differences: the export menu has no icons before the formats (`ExportItem` draws none in any
skin); the shared Open window icon; the new clip's well has no merge icon (the app has no such
icon); the model rows' inserts are coloured by their place in the catalogue; the current
choice of a dropdown is lit like the hovered one and keeps the check, not a lamp.

**Review fixes (Pinball pages, 2026-11-02):** the scroll ball sat in the middle of a long
invisible thumb, so it never reached the ends of its rail (half way down at the top of a
page); the thumb is now the 13 px ball itself (`ScrollDecor.thumb-h`), top at the start and
bottom at the end, as the mockup. The transcript's words ran under the rail and the ball;
its text keeps 16 px clear on the right. Settings shows its last message on a dot matrix in
a 40 px apron, as the settings mockup, not in a floating box. Exe 115.73 MB, `app.rs`
215.81 MB (+0.03 MB); release rustc of the UI peaked at 27.3 and 29.5 GB in two builds.

**Generated code size, Pinball pages:** the pages add about 10 MB of `app.rs` on their own.
Before the build, the row pickers of eleven skins that inherited `Select` (RowSelect, Combo,
LineSelect, RackSelect, GroupSelect) became `Select` with the same width and height at each
use: a component that inherits `Select` copies its whole tree and popup (about 0.87 MB of
`app.rs` per skin). With that, `app.rs` did not grow at all: it is 0.3 MB smaller (216.1 to 215.8 MB) and the
release rustc of the UI peaked at 28.0 to 28.2 GB (the Cozy Cabin build: 29.7). The Cozy Cabin's pages keep their RowSelect
(they were not to change in this step); changing it would save the same again.

**Lessons from the Pinball pages:**
- **`slint-compiler` measures a change in seconds.** `cargo install slint-compiler --version
  1.18.1` (into a scratch folder, on the shared target) and `slint-compiler -f rust --style
  fluent-dark --embed-resources as-absolute-path app.slint -o app.rs` give the generated code
  without a build (about 32 MB smaller than the build's, the images are not in it). Comparing
  the sizes of the `struct Inner<Name>` sections before and after shows what a change costs.
- **A component that inherits a shared widget copies it** (`component RowSelect inherits
  Select`): bind the width and height at the uses instead.
- **A property named `right` hides the alignment word:** in a component with `in property
  <string> right`, `horizontal-alignment: right` reads the property and fails; write
  `TextHorizontalAlignment.right`.
- **A layout cannot carry an accessible role:** the speed lanes are a box (`accessible-role:
  list`) with the lanes placed by x.
- **Insert after a block, not after its first line:** an edit script that added a line after
  `if Theme.cozy: CzBar {` put the Pinball lanes inside the Cozy bar, where they never showed;
  only the preview caught it.
- **A text in a box with only an `x`** keeps its preferred width, so a centred number stuck to
  the left of its insert; give it the box's width and height.
- **Checks:** the scratch clips copied without their times show in another order (their
  creation time is the copy's), so the drive uses clip_106 and clip_108, which are in view in
  every theme at both sizes.

**Budgets measured on the Pinball pages** (test copy, 2.25x display, 10 s samples, pages
visited in turn in one run, Ember on the same exe, 2026-11-02):
- CPU idle 0.0 to 0.6% on every page at 520x700 and 1100x700 (Ember 0.0 to 0.3%).
- Working set against Ember at 520x700: main +6.0, clip -0.3, settings +2.6, merge +3.5, widget
  +3.8 MB; at 1100x700: +6.0, +2.2, +4.9, +5.7, +3.9 MB (the backglass pictures).
- Exe -0.14 MB (115.87 to 115.72 MB), `app.rs` -0.3 MB (216.1 to 215.8 MB): the Pinball pages
  cost about 10 MB of `app.rs` (about 4 MB of exe) and the eleven skins' pickers gave back
  as much. Art 13 KB (back-w), no new fonts. The release rustc of the UI peaked at 28.0 to
  28.2 GB.
- Ember, Tape Deck and Pocket Synth: every page, menu, dropdown and question at 520x700 and
  1100x700 (38 shots each) match the Cozy Cabin build pixel for pixel, but for run to run
  noise seen in two runs of the same build (the search caret blinking in one translate menu,
  the anti-aliased end of Tape Deck's settings scroll thumb). The settings of the nine other
  skins whose pickers changed (top, dropdown open, middle, end, both sizes) match too, but
  for one Car Dashboard shot scrolled a pixel differently.
- Not verified in the app: the models page and Ask (no transcription or AI in the test build;
  previews only), a merged clip's parts and their menu (the test clips carry none; previews
  only), a model download, the answer while thinking, a real file drop (the cover is shown
  with `THRASHER_TEST_COVER`), tooltips, dragging an order row.

**Built in step 5, Pinball main page (2026-10-30):** `ui/skins/x-pinball.slint` (skin 14, theme
"Pinball", index 21 in `Theme.names`, after Cozy Cabin; `EFFECT_FPS` 0 and `RECORDING_FPS` 4
there), art in `ui/skins/x-pinball/` (87 PNGs, 1.0 MB) from `tools/skin-art/x-pinball.html`
(`sh tools/skin-art/cut.sh x-pinball OUT`; the playfields and the display glow are used at 1x only).
- Settings gallery: its tile is the Pinball line in `Gallery.devices`, band #e0316b.
- The backglass holds the Monoton title and the orange dot matrix (the state with its lamp, the
  timer, the microphone); the playfield holds the lane lights (the input level, -40 to 0 dB),
  the pop bumpers (Import, Ask, the widget) and the shooter lane, whose plunger is REC
  ("Record", "Stop and save"). While recording the red bumper is Pause and Resume (latched
  with an amber ring), since Import is not offered then. Clips are arrow inserts (play or
  pause, colours red, yellow, blue, green in turn, lit orange on the highlighted one); the
  apron has the flipper button (play) and the ball on the progress rail (a click jumps)
  under a small display with the clip and its time.
- Portrait windows stretch the backglass and playfield pictures to the window (their rays and
  gradients take it well); wide windows have the fixed 400px column with the bumpers in a
  triangle and the flippers at the foot when the playfield is 300px tall or more. Without the
  local AI there is no Ask bumper (as in the classic look); the widget takes its place.
- Only the status lamp moves, blinking while recording (`RECORDING_FPS` 4, `EFFECT_FPS` 0).
- Fonts: Barlow Condensed 400 (the same family and names as the Answering Machine's 600 and
  700; the Answering Machine main page did not change, pixel for pixel), and four dot matrix
  faces drawn by `python tools/skin-art/x-pinball-fonts.py SRC OUT` from the mockup's own 5x7
  table (127 KB). Monoton is backglass art only: the brand and the titles are PNGs, and the
  font sits in `tools/skin-art/fonts/` with its OFL for the element page, not in the app.
- Its other pages are its own since 2026-11-02 (`Theme.pages` 14, see "Built in step 5, Pinball
  on every other page" above).

**Lessons from Pinball:**
- **A dot matrix is a font.** The mockup draws it on a canvas. "Pinball Dots" puts each glyph
  pixel on a dot grid: one em is 8 rows (7 for the glyph, one between lines), every advance a
  whole number of dots, so a font size of 8 x pitch spaces the dots `pitch` apart. Text is put
  on the grid from its own `preferred-width` (`round(width / pitch) - 1` dots wide), centred or
  right-aligned by whole columns (`DotText`). The unlit grid is one more glyph, an 8x8 block of
  small dots (U+2588), as lines of 20 blocks clipped by the display; the glow is the same text
  in "Pinball Halo" (0.62 pitch dots, 22% alpha) underneath; the timer is "Pinball Dots Big"
  (each glyph pixel 2x2 dots at the same pitch, as the mockup's scale 2). Recording stays at 8
  to 9% CPU with the glow; lowercase maps to the capitals and Norwegian letters are drawn.
- **`col` and `row` cannot be property names** (a GridLayout's); Slint stops with "Cannot
  override property 'col'".
- **A layout with only an `x`** in a plain Rectangle takes the parent's whole width, so the lane
  row spread its lights across the playfield; give it `width: self.preferred-width`. An `Art`
  narrower than its parent and without `x` sits in the middle (the playfield picture did).
- **A radial gradient does not slice.** The project chip cut in three showed hard steps where
  the stretched middle met its ends. It is drawn in Slint: the middle's colours as a vertical
  gradient, a horizontal overlay darkening both ends (colours sampled from the cut), the tip
  as a `Path` with its own gradient, the glow as three faint rounded rings.
- **Big art is memory.** The display case as one PNG (rings, glow and the dark box) was about
  1 MB decoded at 2x; the rings are rounded rectangles now and the glow a 1x nine-slice
  picture with a clear middle. Soft pictures (the playfields) ship at 1x only.
- **Checks:** a stop selects the new clip at the end of the list, so a key check after it
  steps up, not down. A clip with "+ system" was staged by copying a WAV to
  `<name>.system.wav` and removed after.

**Budgets measured on Pinball** (test copy, 2.25x display, 10 s samples, 2026-10-30):
- CPU idle 0.0% at 520x700 and 1100x700 (Ember 0.2%); recording 8.8% and 7.7% (Ember 17.2%
  and 19.8%).
- Working set idle 49.7 vs Ember 41.6 MB (+8.1), wide 57.8 vs 49.4 MB (+8.4); recording 53.0
  vs 45.4 MB, wide 61.0 vs 52.8 MB. The most of any skin so far, inside the 10 MB budget: the
  backglass pictures are stretched from 2x cuts of the whole glass.
- Exe +2.96 MB (62.57 to 65.53 MB): art 1.0 MB, fonts 0.18 MB, the rest generated code.
  Ember, Tape Deck, Phosphor, Pocket and Answering Machine main pages at 520x700, 420x560,
  700x700 and 1100x700 match before and after, pixel for pixel.

**Built in step 5, Phonograph main page (2026-10-30):** `ui/skins/x-phono.slint` (skin 15,
theme "Phonograph", index 22 in `Theme.names`, after Pinball), art in `ui/skins/x-phono/` (172 PNGs, 0.76 MB) from
`tools/skin-art/x-phono.html` (`sh tools/skin-art/cut.sh x-phono OUT`; the four tiles use only
their 1x cut). Its other pages are the classic ones in a mahogany and brass palette
(`Theme.pages` 0).
- Settings gallery: its tile is the Phonograph line in `Gallery.devices` (`ui/gallery.slint`), band #b48c4b.
- The recorder is the machine: the state tag with its jewel lamp, the brass counter of figure
  wheels, the wax cylinder on its mandrel with the reproducer, the flyball governor (the level,
  -40 to 0 dB, pp to ff) and the horn bell, whose wax seal is Record ("Stop and save" while
  recording, with STOP on its plate). Import, Ask and the widget are ivory keys under it
  (portrait) or at the foot of the machine (wide); while recording the Import key is Pause and
  Resume, since Import is not offered then. The project is a brass card holder, the clips are
  cylinder tins whose lids play them, the footer is the reproducer rail with its carriage.
- Short wide windows (under 484 px of machine) take the portrait horn and governor and close
  the gaps; under 400 px the COUNTER word goes.
- Motion only while recording (`RECORDING_FPS` 12, `EFFECT_FPS` 0): the mandrel hub turns and
  the sheen runs along the wax. The governor follows `Main.live`, so it needs no clock.
- Fonts (0.15 MB): Cinzel 600 and 700, and EB Garamond as "Phono Garamond" 600 and italic 500
  with its lining tabular figures as the plain digits, plus "Phono Figures" for the counter
  (OFL, no reserved names, Latin subset): `python tools/skin-art/x-phono-fonts.py SRC OUT`.
  Their 5s have a flat top bar and read clearly. Shared, no visible change elsewhere:
  `TextField.hint-italic` (the search placeholder is italic).

**Lessons from the Phonograph:**
- **Old style figures by default:** EB Garamond's plain digits hang below the line and Slint
  cannot turn on `lnum` or `tnum`, so the build maps the `lnum` then the `tnum` glyph of each
  digit in the cmap and renames the family. Check a text font's default figures before using it.
- **Figure wheels for a proportional font:** "Phono Figures" keeps only the digits and makes
  `:` and `.` empty glyphs of no width, so in "01:23.4" digit k starts at k advances. Each wheel
  clips the whole string with a window exactly one advance wide, centred in the wheel; a window
  as wide as the wheel showed slivers of the neighbouring digits. The separators are their own
  Texts between the wheels.
- **A level gauge as a frame strip:** the governor's arms, balls, collar and pointer are 12
  frames cut from the mockup's SVG and picked by the level. No `Path` is drawn, the look is the
  mockup's exactly, and recording costs no more than a meter (9.8% at 520x700).
- **A gradient across a part of any width with round ends:** the cylinder's middle is a Slint
  `90deg` gradient (square), with the grooves as 3px and 2px tiles and the light as a vertical
  gradient over it; only its 4px ends are PNGs, which works because the gradient ends on the
  same colour at both ends. The tins' label colour changes per row, so their body is Slint too,
  with only the rounded right end as an alpha PNG over a label coloured rectangle.
- **The scroll bar takes clicks:** `Scroller`'s bar has a drag area over its right 11px, which
  ate the clicks on the menu studs at the end of each tin. The page makes the scroller 10px
  wider than the rows (`row-w` comes from the page), so the bar sits in the gutter beside them.
- **Slint 1.18 snags:** a repeater over `text.character-count` generated Rust that did not
  compile ("cast cannot be followed by a method call"); `text == "" ? 0 : text.character-count`
  works (as in `Digits`). A property named `col` cannot be declared on any element (it is a
  GridLayout property). An `Image` inside a plain Rectangle draws at its source size unless it
  gets a width and a height, so a 2x wheel was drawn twice as big.
- **Review fixes:** in a wide list near 820 px the line under a tin's name (the date, the
  TRANSCRIBED stamp and "+ SYSTEM") ran into the waveform; the wave now leaves that line its
  room and the date elides first. In a portrait window under about 445 px a counter past an
  hour ("1:02:03.4") ran over the RECORDING tag; it now starts after the tag.
- **rustc crash:** a release build died once with `STATUS_STACK_BUFFER_OVERRUN` in the huge
  generated `app.rs`; `RUST_MIN_STACK=67108864` built it every time after.
- **Checks:** the working set of the same build and theme varied by up to 5 MB from one
  launch to the next; take several samples and give the range. The test microphone gives silence, so the governor
  stayed at rest in every shot; Ask is not offered there (no local AI), so its key was not seen;
  tooltips do not show headless (as for every theme). One dot of Ember's idle meter flips
  between two states from run to run, in the old build and the new.

**Budgets measured on the Phonograph** (test copy, 2.25x display, 10 s samples):
- CPU idle 0.0 to 0.5% (Ember 0.0 to 0.5%); recording 9.8 to 11.3% at 520x700 (Ember 16.4
  to 19.7%) and 13.4 to 16.6% at 1100x700 (Ember 20.5 to 25.9%), two runs.
- Working set idle 44.4 to 50.4 vs Ember 41.2 to 42.4 MB (+3 to +8), wide 55.8 to 58.2 vs
  49.9 to 50.6 MB (+6 to +8); recording 52.5 to 53.3 vs 44.9 to 45.5 MB, wide 60.8 to 61.5 vs
  52.9 to 54.4 MB. The wide governor strip is the biggest piece (2.1 MB decoded at 2x).
- Exe +2.62 MB (65.61 to 68.23 MB): fonts 0.15 MB, art 0.76 MB, the rest generated code.
  Ember, Tape Deck, Phosphor and Pocket main pages at 520x700, 420x560, 700x700 and 1100x700
  match before and after.

**Built in step 5, Comic Book other pages (2026-10-30):** mockups in
`docs/mockups/device-themes/x-comic-pages/` (approved by Max 2026-10-30), built as Tape Deck's
step 3 was.
- `ui/skins/x-comic-pages.slint`: `ClipPage`, `SettingsPage`, `ModelsPage`, `MergePage`,
  `AskPage`, `Widget`, `Dialog`, `Speaking` and `Drop`, picked in `app.slint` with
  `Theme.pages == 4` (`Theme.pages` is 4 for skin 4; every other value is as before). They
  reuse the main page's parts (`x-comic-parts.slint`: `Box`, `Caption`, `Tag`, `Bubble`, now
  with `tail-left`, `Meter`, `Dots`, `InkBtn`) and its logo, fonts and halftone tiles.
- The clip's name speaks in a bubble on the NOW SHOWING splash panel with the yellow counter
  caption and the Rename and Delete ink keys; the waveform is an inked strip (played part
  red, the playhead an ink line with a red flag, trimmed ends under grey halftone, yellow tabs
  as the trim handles, `TrimTouch` as before); Speed is a five cell strip, Loop a key that
  sinks while on; Export is the red key in the halftone footer. Settings are tagged white
  cards on a hard shadow with an inked scroll rail (wide: Recording and Window, Files and
  Your voice, Transcription and AI in three columns; narrow: Recording, Files,
  Transcription, AI, Your voice, Window, as the portrait mockup stacks them); the models are collector cards (pips for
  quality and speed, the one in use yellow with the IN USE stamp); merging is a crossover
  issue (a cell per clip and a plus burst, wide) with the order as numbered speech bubbles;
  Ask is a thought bubble field and the answer a speech bubble with the ANSWER caption; the
  widget is the mockup's variant A (one halftone panel: status caption, timer bubble, REC or
  STOP burst, Pause beside it while recording, a tiny BZZZT meter, red frame while
  recording); questions are white splash panels (tags UH OH for a delete, WAIT for unsaved
  changes, MEANWHILE for speaking new words); Drop to import is a yellow and ink frame with
  a plus burst.
- The shared widgets draw the comic look when `Theme.comic` is set, next to the deck look:
  `Btn` (ink key, yellow main action, red for `red` or `danger`), `IconBtn`, `Toggle` (inked
  pill, yellow and a red knob when on), `Select` (yellow caption picker, a white panel list,
  the current line yellow with a red edge), `MenuCard`, `MenuRow` (new `caption` for the
  project names in Bangers capitals), `MenuRule` and `MenuTag` (new: the rule and the ink
  group tags AUDIO and TRANSCRIPT), `ExportItem`, `ExportButton`, `ProjectMenu`, `ShortcutBox`
  (ink key caps, PRESS THE KEYS in a yellow caption), `OrderList` (grip, red place circle,
  play dot, the clip as a bubble), `TranscriptCard` (a tagged panel: TRANSCRIPT, TYPING with a
  red frame, SUMMARY, TRANSLATION; the spoken word boxed in yellow, tags in blue), `Tip` (a
  small yellow caption), `Scroller.ink-rail` and the theme gallery strips (`gallery.slint`:
  ink tags, ink frames, the current tile red framed with the PICKED stamp and its name in a
  yellow caption, a yellow and ink ring for the keyboard focus). The `Comic` inks moved to
  `common.slint` (re-exported by the parts file) with `InkShadow`, the hard shadow.
- Rust: the editor's pause tags are blue for the Comic Book (`edit_markup`), as it shows them.
- Art (all from `tools/skin-art/x-comic.html`, `sh tools/skin-art/cut.sh x-comic OUT`, 11 new
  pieces at 1x and 2x, about 0.23 MB): the small masthead `logo-s`, the page titles
  (`title-settings`, `-models`, `-ask`, `-merge`, red letters with an ink edge and a yellow
  shadow), the tilted `inuse` stamp, the widget bursts `wrec` and `wstop` (two frames), the
  plus bursts `plus-s` and `plus-l`, and the `dots-red` tile of the progress bars.
- The flavour captions stay (NOW SHOWING, MEANWHILE, UH OH, WAIT, CROSSOVER, THE MODEL RACK,
  FIND A MODEL, ANSWER, SOURCES). Dialog titles drop the question mark (the copy rule): the
  comic dialog writes "DELETE 3 CLIPS" itself and shows a single clip's name in a yellow box;
  the classic and Tape Deck dialogs are unchanged.

**Lessons from the Comic Book pages:**
- **A hard shadow is a child drawn first.** A Rectangle's own background and border are
  painted under its children, so a shadow child would cover the face: the widget itself is
  transparent in the comic look and draws `InkShadow` (offset copy) then the face. A popup's
  children may draw outside it, so `MenuCard`'s 5px shadow sits outside the popup box.
- **Keep implicit placement when sharing a part.** The menus' hairlines had no `y` and were
  centred in their 9px row; the shared `MenuRule` keeps `(height - 1px) / 2` for the other
  looks, or Ember's rules would have moved 4px. A Text without `x`/`width` in a Rectangle is
  centred too, so the comic's "Not set" is its own Text rather than a changed binding.
- **A property can shadow an enum value:** in a component with `in property <bool> right`,
  `horizontal-alignment: right` fails ("Cannot convert bool to TextHorizontalAlignment").
  Write `TextHorizontalAlignment.right`, and `LayoutAlignment.center` in ternaries.
- **No widths from a sibling layout:** a bubble bounded by its column's width (`most: tl.width`)
  loops through the layout; the room comes from the page width instead.
- **A wrapping column FlexboxLayout wraps** into a second column when its height is short (the
  wide shelf chips); a plain VerticalLayout per chip holds them in one column.
- **Checks:** `slint-viewer --check app.slint` (1.18.1, in `~/.cargo/bin`) compiles the whole UI
  in seconds without a window; run it before the 20 to 40 minute build in the shared target.
  The shared target has no hashed copy of `thrasher.exe` in `deps/`, so copy
  `release/thrasher.exe` right after the build. UI Automation does not see list rows scrolled
  out of view, so drive visible clips (`clip_001`, `clip_109`); the transcription and Ask pages
  need `thrasher-transcribe.exe` and `thrasher-ask.exe` beside the test copy. Tooltips and file
  drops cannot be shown headless; the answer came at once, so Ask's THINKING was not seen.

**Budgets measured on the Comic Book pages** (test copy, 2.25x display, 10 s samples, 2026-10-30):
- CPU idle 0.0 to 0.6% on every page at 520x700 and 1100x700 (Ember 0.0 to 0.8%); a clip
  playing on its page 25.2% and 28.4% (Ember 27.8% and 30.2%). Nothing on these pages moves
  on the clock (the widget's STOP burst boils only while recording, as on the main page).
- Working set within 3 MB of Ember on every page: main 42.9 vs 42.5 MB, one clip 76.2 vs
  76.9 MB, settings 227.0 vs 226.3 MB after a clip was played (wide 237.1 vs 235.1 MB).
- Exe +9.89 MB (77.76 to 87.64 MB; Tape Deck's step 3 was +10.35 MB): art 0.23 MB, the rest
  generated code for the pages and the comic look in the shared widgets. Ember and Tape Deck:
  every page (main, both clip pages, settings top and end, models, Ask, merge, rename, delete,
  widget) and the shared menus, dropdowns and order lists (project menu, clip menu with its
  Export and Move to submenus, the clip's export and translate menus, the editor, a settings
  dropdown, the hotkey listening, merge and a merge row playing) at 520x700 and 1100x700
  match before and after, pixel for pixel.

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

**Built in step 5, Phosphor on every other page (2026-10-30):** mockups in
`docs/mockups/device-themes/phosphor-pages/` (Max approved them 2026-10-30).
- `Theme.pages` is 2 for Phosphor (`Theme.phos`); Tape Deck keeps 1, every other theme 0.
  `app.slint` picks `ui/skins/phosphor-pages.slint` (`ClipPage`, `SettingsPage`,
  `ModelsPage`, `MergePage`, `AskPage`, `Widget`, `Dialog`, `Speaking`, `Drop`) with
  `Theme.pages == 2`, in the same blocks and with the same callbacks as Tape Deck's.
- `ui/skins/phosphor-parts.slint`: the parts the main page and the other pages share (the
  lit block, the pixel glyphs, the caps labels, the "-- WORD ----" rules, the title bar,
  now with an optional `[<]back` key and its tty, and the glass: scanlines and vignette).
  The tube's colours (`Ph`), the dashed rule (`PhDash`), the light shade (`PhShade`) and
  the bracketed key (`PhKey`) moved to `common.slint`, so the shared widgets can draw the
  terminal look. The main page imports them; it looks as before.
- Each page is a tty: `[<]clips` or `[<]back`, the page's name in VT323 (SETUP, MODELS,
  MERGE CLIPS, ASK), sections under dashed rules, settings as lines with a dotted leader
  to their value, keys in brackets (`[r]ename`, `[t]r`, `--all`) or boxed (PLAY, EXPORT,
  every text button), the highlight in inverted video, the latest message as a `> ` line
  in the bar at the bottom.
- The shared widgets draw the terminal look when `Theme.phos` is set: `Btn` (a boxed key
  in VT323, lit for the main action, amber for `red` and `danger`; or a bracketed key when
  `key` is set), `IconBtn` and `Toggle` (`key` and `tail` turn them into `[c]opy` and
  `[m]arks`; a plain `Toggle` is `on` / `off` in a box), `Select` (a boxed value with a
  "v"; its list grows leftwards from the box's right edge, so it stays in the window),
  `MenuCard`, `MenuRow` (inverted hover, a star by the current choice, `prefix` draws
  "~/" before a project without changing its name), `ExportItem`, `ExportButton`
  ("EXPORT ^ flac", a shade bar while exporting, then "[ok] DONE"), `OrderList`
  (":: 01 [>] clip_005 2:03 [x]", the dragged line inverted), `ShortcutBox` (boxed keys
  joined by "+"), `TranscriptCard` ("$ cat clip_005.txt" over a dashed rule, the played
  words bright, the word playing inverted, pause tags amber, "-- INSERT --" and a block
  cursor while typing) and `Tip`. The gallery (`gallery.slint`) has a Phosphor tile: a thin
  frame, its name inverted on the current one.
- Review fixes (2026-10-30): wide settings use `ThemeGallerySide` (both strips side by
  side, as the mockup), so the page fits 1100x700 without scrolling; "not set" in the
  hotkey box sits flush right; a dragged order row leaves a dashed slot where it lands.
- Rust: the editor's pause tags are Phosphor's amber (`Theme.phos`).
- No new art or fonts: the pages reuse the main page's PNGs (dash, vdash, dither, halo,
  vignette) and its two fonts.

**Lessons from Phosphor's other pages:**
- **A child without x and y lends its size to its parent.** Slint adds the preferred size
  of every child that has neither `x` nor `y` set to its parent's own (that is how a
  Rectangle sizes to a layout in it), visible or not. A Phosphor-only word in `Btn` (VT323,
  `width: parent.width`) made Ember's Merge key 16 px wider. Every look-only child of a
  shared widget gets `x: 0; y: 0` (Tape Deck's had a `y` by chance), and Ember and Tape
  Deck are compared pixel for pixel after each change.
- **Keys in brackets on shared widgets:** `key` and `tail` on `Btn`, `IconBtn` and
  `Toggle` keep the widget, so the accessible name, the role and the tooltip stay those
  of the classic button ("Translate", "Show marks" as a checkbox), while it draws as
  `[t]r`. The width follows the text only while the key is shown.
- **A popup's list must fit the window:** a dropdown wider than its box at the right edge
  went past the window; Phosphor's opens from the box's right edge.
- **Checks:** `setval.ps1` (UI Automation ValuePattern) types into the transcript editor
  and the Ask field, so the unsaved-changes dialog and a real answer can be shot headless.
  The cargo lock of the shared target dir made each release build wait up to 30 minutes
  behind the other skin agents: batch fixes between builds.

**Budgets measured on Phosphor's other pages** (test copy, 2.25x display, 10 s samples,
2026-10-30):
- CPU idle 0.0 to 0.9% on every page at 520x700 and 1100x700 (Ember 0.0 to 0.5%). Playing
  a clip on its page: 22.2% at 520x700 and 31.3% at 1100x700 (Ember 26.9% and 22.0%; the
  wide waveform is about 90 block bars that follow the playhead).
- Working set over Ember: +1.3 to +3.6 MB per page (main 45.1 vs 42.1 MB, one clip 78.3 vs
  77.0 MB, settings 229.8 vs 226.2 MB at 520x700).
- Exe +10.54 MB (77.76 to 88.30 MB): generated code only, no new art or fonts. Ember and
  Tape Deck: every page, the dialogs, both menus, the export and translate menus and a
  dropdown at 520x700 and 1100x700 match the build before pixel for pixel (50 shots).
- Not verified headless: dropping files (the Drop cover needs a real drag), speaking new
  words (needs OmniVoice), a merged clip's parts (the scratch clips have none with two
  parts), a model download, and tooltips (posted mouse moves raise no hover).

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
other pages are its own since 2026-10-30 (`Theme.pages` 5, see "Spatial Glass on every
other page" below). It uses `ProjectMenu.up` (added with Phosphor) to open the project menu
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

**Built in step 5, Spatial Glass on every other page (2026-10-30):** mockups in
`docs/mockups/device-themes/x-spatial-pages/` (approved by Max 2026-10-30, widget variant A).
- `ui/skins/x-spatial-parts.slint`: the parts the main page and the other pages share,
  moved out of `x-spatial.slint` with no visible change: `Shadow`, `Pane` (now with `lift`,
  the brighter pane you type in), `Capsule`, `GlassBtn` (now with `tint`), `Status`, `Timer`,
  `Grab`, and `TiltPane`, the wide recorder's tilted glass without its contents (the wide
  clip, models and merge pages use it for their left pane; `RecorderTilt` inherits it).
- `ui/skins/x-spatial-pages.slint`: `ClipPage`, `SettingsPage`, `ModelsPage`, `MergePage`,
  `AskPage`, `Widget`, `Dialog`, `Speaking` and `Drop`, picked in `app.slint` by
  `Theme.pages == 5` (`Theme.pages` is 1 for Tape Deck, 5 for Spatial Glass, else 0). Every
  page is panes in the main page's room: a back pill with the page's name, minimise and
  close in the title bar; the main actions in an ornament under the window (Clean voice and
  Export, Cancel and Merge); messages as a glass chip above it (portrait) or in it (wide).
- `Theme.glass` (skin 5) makes the shared widgets draw the glass look, as `Theme.tape` does
  the deck look: `Btn` (glass pills, the main action white, `red` for Delete and Merge),
  `IconBtn` (round glass, white while on), `Toggle` (dark slot, green when on), `Select` (a
  glass capsule and a glass list), `MenuCard`, `MenuRow`, `ExportItem`, `ExportChoices`
  (brighter glass menus with lit rows), `ExportButton` (white pill, a dark pill that fills
  while exporting), `OrderList` (a grip, the place on a white bead, a round play button, a
  plain x or dots, a faint slot where a dragged row lands), `ShortcutBox` (glass key caps),
  `Tip` and `TranscriptCard` (the page's pane is the card; the word playing on a white bead,
  pauses as cyan pills, a dark well and a cyan caret while editing, a cyan change chip). The
  main page's menus are glass now too. `Glass` (the colours) moved to `common.slint`.
- `TextField.focus-edge` and `focus-ring`: the white edge of a glass field with the cursor
  (defaults are the accent, so no other look changes).
- The gallery (`gallery.slint`) tints its Ember look for glass: a white frame and check bead
  on the current tile, a cyan focus ring, glass hover, no edge fades (the solid card colour
  of the fade would show as a band on see-through glass).
- Art: three new pieces from `tools/skin-art/x-spatial.html` (`ONLY="widget-room orb-w
  ring-w" sh tools/skin-art/cut.sh x-spatial OUT`): the widget's room with its 24px corners
  cut (the widget window is see-through and the renderer has no rounded clipping), its orb
  and its dim halo of 40 ticks, 1x and 2x, 157 KB.
- Fonts: "Spatial Figures" gains % G B M (percents and model sizes in tabular figures),
  `tools/skin-art/spatial-figures.py`; 4.2 KB each.
- Rust: the editor's pause tags are cyan for glass (`edit-markup`).
- Copy: dialog titles have no question mark ("Delete clip_005", "Delete 2 clips"); the
  glass dialog words its delete title itself, so the other looks keep theirs.

**Lessons from Spatial Glass's other pages:**
- **`slint-viewer --check ui/app.slint`** compiles the whole UI in seconds, prints every
  error and warning and opens no window. Run it before a cargo build, which takes 15 minutes
  and more while parallel agents queue on the shared target's lock.
- **The shared target holds whoever built last:** copy `thrasher.exe` out only after checking
  it is this branch's build (it holds a piece of this branch's new art, see `mine.py` in the
  step's scratch kit), and under a unique name.
- **Elements outside a layout are centred,** again: a title bar component without `y: 0`
  sat in the middle of every page. Set `y` in the component itself.
- **Pills without gradients:** gradients ignore corners, so glass pills, switches and the
  play orb are flat fills with a 1px light line along the top, not the mockup's gradients.
- **A rebuilt subset font can differ in glyph names only:** compare outlines decomposed
  (`DecomposingRecordingPen`) and advances before trusting that old text did not move.
- **Checks:** restoring the scratch clips gives them new creation times, which the rows show
  ("Today 17:05"), so a before and an after run must share one restore. A dialog's field
  takes Esc and Delete (posted keys go to it, not the window): close a dialog with its Cancel.
  UI Automation cannot press a control scrolled out of view: wheel the page first (a posted
  move, then the wheel). A short clip exports in 0.1 s, too fast to see the progress: use a
  long one (clip_100, 69 minutes).

**Budgets measured on Spatial Glass's other pages** (test copy, 2.25x display, 10 s samples,
2026-10-30):
- CPU idle 0.0 to 0.5% on every page at 520x700 and 1100x700 (Ember 0.0 to 1.1%). Nothing
  moves on a clock.
- Working set over Ember on the same exe: main +1.9 / +3.4 MB, clip +1.9 / +4.1, settings
  +2.2 / +4.3, models +2.4 / +4.1, ask +2.4 / +3.7, merge +2.0 / +4.5, widget +3.0 / +6.8
  (520 / 1100 wide; the wide room picture stays loaded in the widget).
- Exe +8.37 MB (74.15 to 82.53 MB): art 0.16 MB, fonts 1 KB, the rest the generated code of nine pages and the glass look in the shared widgets (Tape Deck's step 3 was +10.35 MB).
- Ember and Tape Deck: main, clip, settings (top and scrolled), models, ask, merge, both
  dialogs, both menus and the widget at 520x700 and 1100x700, 48 shots, match pixel for
  pixel before and after.

**Built in step 5, Quest Menu main page (2026-10-29):** `ui/skins/px-quest.slint` (skin 8,
theme "Quest Menu", index 15 in `Theme.names`), art in `ui/skins/px-quest/` (23 PNGs, 4 KB)
from `tools/skin-art/px-quest.html` (`sh tools/skin-art/cut.sh px-quest OUT`). Its other
pages are its own since 2026-10-31 (`Theme.pages` 8, see "Built in step 5, Quest Menu on every
other page" below).
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

**Built in step 5, Quest Menu on every other page (2026-10-31):** mockups in
`docs/mockups/device-themes/px-quest-pages/` (approved by Max 2026-10-31: widget variant A,
toggles as the ON OFF pair), built as the batch 1 pages were.
- `Theme.pages` is 8 for the skin (`Theme.quest`); every other value is as before.
  `app.slint` has a `Theme.pages == 8` block next to each Car Dashboard one, with the same
  bindings, and picks `ui/skins/px-quest-pages.slint` (`ClipPage`, `SettingsPage`,
  `ModelsPage`, `MergePage`, `AskPage`, `Widget`, `Dialog`, `Speaking`, `Drop`).
- `ui/skins/px-quest-parts.slint`: the parts the main page and the other pages share, moved
  out of `px-quest.slint` unchanged (`Px`, `Label`, `Win`, `Notches`, `Ring`, `PBtn`,
  `TitleKey`, `Title`, `Sprite`, `Status`, `Timer`, `Meter`, `RecKey`; the blink is `QFx`).
  `Title` takes `gear` (the other pages leave Settings out, as the classic pages do) and a
  back key (`back-text`, the clip page's "Clips"). New: `Key`, the pixel key with an icon.
  The colours and fonts moved to `common.slint` as `Quest` (the parts file re-exports it as
  `Q`), with `QFace` (a key's face), `QFrame` (a floating menu window), `QLit` (the lit row
  with the glove), `QPx` and `QIcons` (pixel icons by the shared widgets' names).
- The pages: the clip is an item card (tape sprite, ITEM, name and meta, Rename and Delete
  keys, the counter in gold Quest Figures), the waveform a pixel strip in its own window
  (played part gold, gold trim posts, white play head; `TrimTouch` as before) with the
  transport (the big play key, back and forward 5, SPEED as a choice row with the cursor,
  Loop as a lamp box), the transcript the dialogue box (gold frame and an EDITING tag while
  typed on), OPTIONS and the footer with Export. Settings is the config screen: cards as menu
  windows, rows split by dark rules, selects a gold value with the down arrow, toggles the ON
  OFF pair, the gallery on top and three columns when wide, a gold scroll rail. Models is the
  item shop (green and blue pip bars, In use lit with a gold check, filter chips or a command
  list with the glove when wide). Merge forms a party (slots, and a formation window of tapes
  joining into one gold framed tape when wide). Ask talks to the sage (owl sprite, numbered
  sources with gold times, a blue project tag when every project is asked). The widget is the
  tiny character card (variant A). Questions are menu windows with a CONFIRM or WAIT tag and
  the glove on the default key; speaking is the same window with a gold bar; dropping files
  shows the chest in a gold double frame.
- The shared widgets draw the pixel look when `Theme.quest` is set, so the main page's menus
  take it too: `Btn` and `IconBtn` (keys with cut corners, pixel icons from `QIcons`),
  `Toggle` (the ON OFF pair; `lamp` for the check box of Marks, Clean voice and Show
  compressed), `Select`, `MenuCard`, `MenuRow`, `MenuRule`, `MenuGroup`, `ExportItem`,
  `ExportButton` (gold bar while exporting), `OrderList` (party slots), `ShortcutBox` (pixel
  key caps), `TranscriptCard`, `Tip` (ink with a gold rim), `TextField.edge-width` (the 3px
  rim), `Scroller.quest-rail` and the gallery tiles (`gallery.slint`). `Theme.menu-h` is 27px
  and the new `Theme.menu-pad` 9px for its menus (5px for every other look).
- Rust: the editor's pause tags are the Quest blue (#56a8ff).
- Art: 25 new pieces at 1x in `ui/skins/px-quest/` (keys, rails, the tape, owl and chest
  sprites, the widget's REC and STOP), `ONLY="back chev ..." sh tools/skin-art/cut.sh px-quest
  OUT` (the list is in `cut.sh`).

**Mockup faults fixed, not copied (Quest Menu pages):** the widget's Pause or Resume key sat
over the PAUSED word (now beside the timer); the merge footer had no room for the message
beside its keys at 400px (portrait floats it above the footer, as the Car Dashboard does);
the settings rows with a note under the name were cut by the next rule (rows with a note are
42px). The merge page shows no gear: the classic pages show none on any other page.

**Lessons from the Quest Menu pages:**
- **A window used as a background must not lend its size.** `Win { width: parent.width;
  height: parent.height; }` behind the transcript card had no `x` or `y`, so its (empty)
  layout's maximum height became its parent's: the dialogue box shrank to its minimum and the
  other windows took the room. Give such a background `x: 0; y: 0`.
- **Children of a component with `@children` in a layout go into that layout:** a tag drawn
  on a window's frame, a `TouchArea` or a ring must be a sibling of the `Win`, not a child.
- **A `Text` with `y` and no `height`** sits at the top of its box: give centred labels
  `height: parent.height - 1px` when they are nudged down a pixel.
- **`slint-viewer --screenshot` with a preview wrapper** (state set in its `init`, `--load-data`
  for the page and size) shows each page in seconds; it caught the layout faults above before
  the next 2 hour wait on the shared target's lock.
- **Checks:** with many agents running, posted presses land late more often; a fresh start
  per page and state (`seg2.sh` in the step's scratch kit) and re-shooting a page until before
  and after match (`parity.py`, up to four tries) separate flaky presses from real changes.
  A merge moves its source clips out of the scratch folder, and restored clips get new
  creation times, so they sort to the end of the list and drop out of view: drive clips that
  are on screen, and restore the sources after a test merge.
- **The test binary outgrew the linker's program database:** `cargo test -p thrasher-app`
  failed to link (`LNK1140: limit exceeded for program database`) with this step's code.
  the workspace `Cargo.toml` now gives only `thrasher-app` line tables in the dev and test
  profiles (`[profile.dev.package.thrasher-app] debug = "line-tables-only"`), so plain
  `cargo test` links again, in CI too, and dependencies keep their full debug build (only
  the app crate rebuilds once).

**Budgets measured on the Quest Menu pages** (test copy, 2.25x display, 10 s samples,
2026-10-31; Ember on the same exe in brackets):
- CPU idle 0.0 to 0.6% on every page at 520x700 and 1100x700 (Ember 0.0 to 0.6%). Nothing
  moves on a clock (the widget's lamp blinks only while recording, as on the main page).
- Working set at 520x700: main 37.8 (38.6), one clip 155.1 (166.2, after a long clip was
  loaded), settings 54.4 (55.1), models 55.8 (56.3), Ask 67.7 (68.3), merge 69.7 (70.5),
  widget 63.7 (64.9) MB; at 1100x700: 45.7 (46.5), 166.6 (169.2), 62.9 (64.6), 64.1 (65.6),
  75.8 (77.4), 77.6 (79.1), 63.8 (65.5) MB. Never above Ember.
- Exe +19.35 MB (136.41 to 155.76 MB): art 3 KB, the rest generated code, more than batch 1
  (+8 to +13 MB): the pixel look is drawn from many small rectangles (frames, cut corners,
  key faces) inlined into every use of the shared widgets, and the clip page carries two
  transcript cards (portrait and wide). For the size pass.
- Ember, Tape Deck and Phosphor: main, project menu, clip menu and its Export submenu,
  rename, delete and new project questions, one clip (plain, Export menu, editing, renaming,
  no transcript), settings (top, middle, end), models, Ask, merge and the widget at 520x700
  and 1100x700, 114 shots, match before and after pixel for pixel.

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

**Built in step 5, Brick Phone on every other page (2026-10-31):** mockups in
`docs/mockups/device-themes/dv-brick-pages/` (approved by Max 2026-10-31: widget variant A,
portrait pages slide the keypad away so the LCD grows, keypad digits pick nothing).
- `Theme.pages` is 9 for skin 9 (`Theme.brick`); every other value is as before. `app.slint`
  has a `Theme.pages == 9` block next to each Car Dashboard one, with the same bindings; the
  question and the covers also get `wide` and `main: root.page == 0` (they draw over the
  handset, so they need to know where its LCD and keys are).
- `ui/skins/dv-brick-parts.slint`: the parts the main page and the other pages share, moved out
  of `dv-brick.slint` unchanged (keys and their faces, shell, top, bezel, small caps, signal
  bars, timer, scrollbar), plus `Hand` (where the bezel, the LCD, the soft keys and the rocker
  sit for a window size, the main page with its keypad or not), `LcdGlass` (the backlight),
  `LcdCorners` and `Battery`. `KeyFace` gains kinds 5 and 15 (the green key while off) and 16
  to 18 (the widget's keys). The colours (`Brick`) moved to `common.slint`.
- `ui/skins/dv-brick-pages.slint`: `ClipPage`, `SettingsPage`, `ModelsPage`, `MergePage`,
  `AskPage`, `Widget`, `Dialog`, `Speaking` and `Drop`. Every page is the handset (`Phone`):
  the LCD's title row (signal bars, the page's name, a note or tag, the battery), the page,
  the soft key labels; the soft keys, the rocker, the green and red keys. The keys keep one
  job each: green is the page's go key (Export with its list on the LCD, Merge N clips, Ask;
  off on settings and models), red is Back, the soft keys are Rename and Delete (clip),
  Cancel (merge) or All projects (Ask, only with two projects); the rocker seeks 5 s and
  plays on the clip page and scrolls settings, models and Ask ("Scroll up", "Scroll down").
  Wide pages keep the keypad on the left wing; its digits only show.
- Questions are LCD boxes with a double ink frame and an inverted title bar over a dithered
  screen; their answers are drawn on the soft keys and the LCD's foot (Discard changes on the
  rocker's middle). The speaking cover is the same box over a darker dither, the drop cover
  a dashed frame 12 px in from the LCD edge with the big pixel arrow.
- Not reachable headless: a merged clip's parts (the scratch clips have none with two parts),
  Ask's answer and THINKING (no AI run), a real file drop, speaking new words (the cover is
  shown with `THRASHER_TEST_COVER`), tooltips, and dragging an order row.

**Budgets measured on the Brick Phone pages** (test copy, 2.25x display, 10 s samples,
2026-10-31, Ember in brackets):
- CPU idle 0.0 to 0.3% on every page at 520x700 (0.0 to 0.5%), 0.0 to 0.9% at 1100x700 (0.0 to
  0.8%; the 0.9 is Ask's blinking caret). Nothing moves on a clock.
- Working set at 520x700: main 40.9 (41.1), clip 165.3 (168.4), settings 63.0 (57.8), models
  64.7 (71.8), ask 77.1 (71.9), merge 77.6 (72.4), widget 71.7 (66.9) MB; at 1100x700: main 49.7
  (48.7), clip 180.1 (170.5), settings 72.2 (66.3), models 74.2 (66.3), ask 86.4 (81.1), merge
  87.3 (80.8), widget 73.1 (67.2) MB. At most +9.6 MB.
- Exe +22.26 MB (136.41 to 158.66 MB): the pages' generated code and the LCD look in the shared
  widgets (the menus inside every skin's clip rows carry it); art 43 KB, no new fonts. Twice
  the batch 1 skins' cost; a target for the size pass.
- Ember, Tape Deck and Phosphor: every page, menu and question at 520x700 and 1100x700 (114
  shots) match the old build pixel for pixel, but for blinking carets and a 4 x 3 px tip of the
  Tape Deck settings scroll thumb at the page's end (520x700), not explained.
- The shared widgets draw the LCD look when `Theme.brick` is set: `MenuCard` (double frame,
  `title` bar), `MenuRow` and `ExportItem` (new `num`: the inverted place number), `Select`
  (boxed value, a numbered pick list under the setting's name), `Btn`, `IconBtn` (square LCD
  keys with pixel glyphs from `BrickIcons`), `Toggle` (the OFF ON pair), `TextField` (2 px
  frame, a second frame while it has the cursor), `ShortcutBox`, `OrderList`, `Scroller` (the
  LCD's ruled bar), `TranscriptCard` (tagged box, VT323 words, the spoken one inverted, pauses
  boxed, `brick-size`), `Tip` and the gallery strips (72 px framed tiles, the current one
  inverted in a double frame). `ProjectMenu` and `ClipMenu` number their lines and carry
  titles (Projects, the clip's name, Export, Move to), so the main page's menus take the look
  too. `Theme.menu-h` is 22 px and `Theme.menu-top` 24 px for the skin (0 for every other).
- The skin's palette is now ink on the amber LCD (it was charcoal for the classic pages), so
  whatever a shared widget draws without a Brick look of its own lands on the LCD in ink;
  the editor's pause tags take the palette's accent (ink) with no Rust change.
- Art: 39 pieces from `tools/skin-art/dv-brick.html` (`kp-off-*` and `kw-off`, the keys off;
  `kg-*`, the widget's keys; icons send, back, check, merge, ask, expand, left, right; pixel
  glyphs from the mockups' maps; `t-dither`), 43 KB; `ONLY="kp-off-l ..." sh
  tools/skin-art/cut.sh dv-brick OUT` cuts them (the list is at the end of its dv-brick line).
- Mockup faults fixed, not copied: the 2 key reads ABC (as the built main page), the invalid
  file name field keeps its 24 px height with the error as an inverted tag under it, the
  transcript words stop short of the scroll bar, and the wide dashed column rule ends above
  the message line.

**Lessons from the Brick Phone pages:**
- **A page drawn over the handset needs its geometry from one place.** The questions and
  covers are drawn by AppWindow over whatever page is shown, so `Hand` works out the bezel,
  the LCD, the soft keys and the rocker from the window size, the size class and whether the
  main page's keypad is there; the pages and the overlays both use it.
- **Redraw a piece of the LCD by clipping a second backlight:** the question's soft labels
  cover the page's own with a `LcdGlass` of the LCD's full size inside a clipped strip, so
  the radial gradient matches to the pixel.
- **Change the palette, not every widget:** setting the skin's palette to the LCD's ink and
  amber gave every shared widget a usable look at once; the Brick look in each widget then
  only adds the frames, fonts and inversions.
- **`slint-viewer --check` on a skin file alone misses errors** (a component never used from
  an exported one, `width` with `max-width`); check `app.slint`.
- **Ids inside an `if` cannot be read from outside it**, also not for a height: the shelf row
  and column became two components.
- **`accessible-role: text` in a component with a `text` property** is read as the property;
  write `AccessibleRole.text`.
- **Widgets with `@children` are inlined wherever they are used** (`MenuCard`, `IconBtn`,
  `Scroller`), so a look added to them costs at every use. Their Brick parts are their own
  components (`BrickMenuFrame`, `BrickGlyph`, `BrickBar`); that took 1.65 MB off the first build.
- **The debug test exe passes the linker's limit:** `cargo test -p thrasher-app` now stops with
  `LNK1140: limit exceeded for program database` (the generated UI code is that large).
  `cargo test --release -p thrasher-app` passed (no debug info). Fixed in review: the root
  `Cargo.toml` sets `debug = "line-tables-only"` for `thrasher-app` in the dev profile (other
  crates keep full debug info), so `cargo test` and the CI's `cargo test --all` link again.
- **Review fixes (2026-10-31):** the pick list (`Select`) and the language list drew rounded
  highlight lines, two lines were inverted at once while hovering (now one, as Desktop 95),
  the pick list's title bar cut "Stop recording after" (the list is 180 px wide now), and the
  language list's Install link and check were still in the classic look.
- **Checks:** the UI Automation names are matched without case, and a `Text` component with
  `accessible-role: none` set on the instance is still listed, so the LCD's "Cancel" label
  caught the press meant for the Cancel key; `Words` now goes silent itself. The drive is
  flaky in places (a menu left open, a window a few px narrower), so the old build was shot
  twice and a page counts as unchanged when it matches either run.

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

### Generated code size

Lesson of 2026-11-01. Slint turns `ui/` into one Rust module (`app.rs` in the build
folder, now in the `thrasher-ui` crate). By the Pocket Synth's pages it was 467 MB, the
exe 246 MB, and one release rustc needed over 30 GB of RAM (a debug one reached 45 GB and
froze the PC). Nearly all of it was copies, not UI. Three things made the copies:

1. **A cheap global binding is pasted into every read.** Slint's optimizer inlines any
   binding that costs less than about 1390 units (one property read is 1000). After the
   reads of `skin` were merged into one local, the 11-way chain
   `pages: skin == 1 ? 1 : skin == 2 ? 2 : ...` fell under that limit and was copied
   about 36,700 times, also through `os95`, `brick` and the other flags built on it: 29%
   of the file. It is now `pages: skin <= 11 ? skin : 0`.
2. **A component whose root binds `opacity`, `visible`, `drop-shadow-*`, an animated
   public property or a percent size is inlined at every use** (Slint wraps the element
   in an Opacity or Clip item, and a component root cannot get a parent). Every `if`,
   `for` and PopupWindow inside it then becomes a new type per use. Select (27 uses) had
   78 copies of its popup and rows (about 67 MB), Toggle 100 copies, Btn and IconBtn
   hundreds of inlined bodies. Each `if` or `for` costs 13 to 24 KB of code even for a
   single Text; a shared sub-component use costs about 1.4 KB.
3. **A component used with children is always inlined**, so the Scroller (always used
   with children) copied its 11 per-skin rails and fade strips into all 100 uses.

What was done (app.rs 467 MB to 188 MB, exe 246 MB to 106 MB):
the `pages` line; Select, Toggle, Btn, IconBtn, TextField, MenuRow and the Car
Dashboard's Key draw everything in a `body := Rectangle` child that fills the root (the
root keeps the API, size, accessibility and input); the Scroller's decorations are one
shared `ScrollDecor` component; and the UI is its own crate, `thrasher-ui`, built with
`opt-level = "s"` (docs/building.md). Every theme's pages were shot before and after and
compared pixel for pixel. The rustc peak for the UI is still about 26 to 29 GB (it was
over 30 GB): it falls only with less generated code.

Rules for new skins and widgets:

- **No `opacity`, `visible`, `drop-shadow-*`, `animate` of a public property or percent
  size on a component's root.** Put them on a `body := Rectangle` child that fills the
  root. Give body **no x, y, width or height**: it fills the root anyway, and only a
  child without x and y passes its layout constraints (preferred width and so on) up to
  the root; with `x: 0; y: 0` the buttons lost their preferred widths and grew.
- If a use site sets a Rectangle look (background, border, radius) on such a widget,
  forward it to body; otherwise it paints the empty root, under body.
- A widget that takes `@children` is inlined wherever it is used with children. Keep it a
  thin shell; put per-skin art (rails, frames, `if Theme.x` pieces) in one separate
  component with plain `in` properties, placed with explicit x and y, as `ScrollDecor`.
- No long `==` chains in cheap global bindings (Theme). One comparison, an array lookup
  (it costs over the limit, so it stays a cached property) or a plain property.
- Per-skin variants of a shared widget: one skin-only component inside the widget's
  single shared type (`if Theme.x: XArt {}`), never new `if` branches in a widget that is
  inlined.
- For a tiny absolutely positioned piece in a widget or page used a few times,
  `visible:` (about 1 to 2 KB) is cheaper than `if` (13 to 24 KB), but never inside a
  layout: invisible items still take layout space there. In a shared widget used many
  times it is the other way round (see the second pass below).
- No component that inherits a shared widget (`component RowSelect inherits Select`): it
  copies the widget's whole tree; set its size at each use (2026-11-02).
- Check `app.rs` after a skin's pages land (the newest
  `target/release/build/thrasher-ui-*/out/app.rs`): a skin's pages should cost about 3 to
  5 MB of exe, not 20 MB.

**Second pass (2026-11-02, after the Pinball pages):** `app.rs` 215.8 MB to 170.8 MB, exe
115.7 MB to 107.5 MB, the release rustc of the UI 29.4 GB to 25.4 GB at its peak. Where
the code went, measured with `slint-compiler` (see "Lessons from the Pinball pages"):
- **Item arrays were the biggest single part (56 MB).** Every `if` or `for` body and the
  window list every item they hold, through all their sub-components, in two constant
  tables (`item_array`, `item_tree`, about 500 bytes per item). A settings page alone held
  1,000 to 1,900 items. Most of them were the other skins' pieces inside the shared widgets:
  `OsBevel { visible: Theme.os95; }` in Btn is in every Btn on every page, whatever the skin,
  and Btn, IconBtn, Toggle, Select and ExportButton put about 90,000 items into those tables.
  As `if Theme.os95: OsBevel { }` the piece is one body, once, in the widget's shared type,
  and in no table: 27 MB less, fewer items in memory too.
- **The if and for bodies were 151 MB of 216.** A body costs about 17 KB before its
  bindings. 3,089 bodies became 2,611; many were copies: a component used with children,
  inherited, or given `opacity` or `visible` at a use is copied, with its bodies, every time.
  OrderList (96 bodies) had two extra copies through an `opacity` on its use in two skins'
  parts lists; each settings row copied its note's `if` into all of its 20 or so uses.
- Checks: every theme's main page, delete question, clip page and settings (top, a dropdown
  open, scrolled) at 520x700 and 1100x700 (276 shots) match the Pinball build pixel for
  pixel, but for run to run noise seen between two runs of that build (the moving
  backgrounds of the classic themes, a gallery arrow edge in Car Dashboard's scrolled
  settings, two pixels in Fighter Select's, Phonograph's scrolled settings). Idle CPU is
  unchanged (0.0 to 0.6% both), the working set 0.2 to 1.8 MB lower on every page measured.

Rules (added to the ones above):
- **A skin's piece in a shared widget is an `if`, not `visible`,** when it sits in a plain
  parent (not a layout) and sets x or y or adds no layout constraints (a repeated child adds
  none to its parent, a visible one without x and y does; so check both before switching).
  `visible` stays right for a piece of a widget used a handful of times.
- **No `opacity`, `visible` or `drop-shadow-*` on a use of a big component:** put it on a
  box around the use (`Rectangle { opacity: x; OrderList { } }`; a box without x, y or size
  takes the inner element's layout constraints, so nothing moves).
- **Nothing inherits a shared widget,** also not to set a few looks (`component Field
  inherits TextField { height: 24px; }`): use the widget with those bindings at each use.
- **A container used with children holds no `if` or `for`:** move the part that has them
  into a component of its own (the settings rows' label and note: `SRowText`; Brick Phone's
  keys: `PhoneKeys`; MenuCard's frames: `MenuCardArt`). Several pieces in a plain parent move
  together into a component placed with `x: 0; y: 0; width: parent.width; height:
  parent.height`.
- In a layout with no spacing, a rule that only some rows show can be zero high
  (`height: root.first ? 0 : 3px`) instead of an `if`.
- After a skin's pages, run `slint-compiler` on `app.slint` and compare the bodies
  (`struct InnerComponent_`) and the item counts (`ITEM_TREE : [...; N]`) with the last
  build: a page body's item count shows what its widgets carry.

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
   - Main pages of all 14: done 2026-10-30 (#104 to #119).
   - Other pages, batch 1 (Phosphor, Desktop 95, Comic Book, Spatial Glass, Car Dashboard):
     mockups approved by Max 2026-10-30, in `mockups/device-themes/<skin>-pages/`. Max's picks:
     widget variant A for Desktop 95 (mini Sound Recorder) and Spatial Glass; flavour labels
     that are not app text stay (Comic Book captions such as UH OH and MEANWHILE, Car Dashboard
     modes such as PLAY MODE, the merged total on the Dash gauge, the Dash Disk heading).
     Phosphor's are built (`Theme.pages` 2, see "Built in step 5, Phosphor on every other
     page").
     Desktop 95's other pages: built 2026-10-30 (see "Built in step 5, Desktop 95 on every
     other page").
     Comic Book built 2026-10-30 (`Theme.pages` 4, widget variant A).
     Spatial Glass built 2026-10-30 (see "Spatial Glass on every other page").
     Car Dashboard built 2026-10-30 (`Theme.pages` 6, see "Built in step 5, Car Dashboard on
     every other page").
     All five merged 2026-10-31 (#120 to #125, with the dialog question-mark fix); exe 136 MB.
   - Other pages of the last 9 (Pocket, Quest Menu, Brick Phone, Answering Machine, Pocket
     Synth, Fighter Select, Cozy Cabin, Pinball, Phonograph): mockups approved by Max
     2026-10-31, in `mockups/device-themes/<skin>-pages/`. Max's picks: widget variant A for
     all nine; Pocket's bezel keys act (A does the page's main thing, B goes back, the pills
     Rename and Delete); Brick Phone portrait slides the keypad away on the other pages
     (variant B); Pocket Synth's dialog Delete stays a dark key. Defaults taken without a
     question: no new keyboard behaviour (Brick Phone keypad digits do not pick items, Pocket's
     D pad only skips 5 s where the app already does), Quest Menu toggles are the ON OFF pair
     (A), flavour labels stay as in batch 1. Small mockup faults the builders fix: Quest widget
     A PAUSED label (class clash), Brick invalid-name panel squashed and the 2 key reading ASK
     (built page says ABC), Pinball wide Export position and portrait main-page overlaps.
     Pocket built 2026-10-31 (`Theme.pages` 7, see "Built in step 5, Pocket on every other
     page").
     Quest Menu built 2026-10-31 (`Theme.pages` 8, see "Built in step 5, Quest Menu on every
     other page").
     Brick Phone built 2026-10-31 (`Theme.pages` 9, see "Built in step 5, Brick Phone on
     every other page").
     Answering Machine built 2026-10-31 (`Theme.pages` 10, see "Built in step 5, Answering
     Machine on every other page").
     Pocket Synth built 2026-10-31 (`Theme.pages` 11, see "Built in step 5, Pocket Synth on
     every other page").
     Fighter Select built 2026-11-01 (`Theme.pages` 12, see "Built in step 5, Fighter Select
     on every other page").
     Pinball built 2026-11-02 (`Theme.pages` 14, see "Built in step 5, Pinball on every other
     page").
   - Size (Max, 2026-10-31): the nine would bring the exe to about 235 MB. Build all nine,
     then one size pass (palette PNGs, shared frames deduped) to pull it back toward 200 MB.
   - Build tip (Max, 2026-10-30): parallel worktrees share one `CARGO_TARGET_DIR`
     (`.claude/targets/shared`), never a target per copy.
6. **Docs:** update `docs/plan.md` section 16 and the building notes.

### Checks for each skin PR

- Headless screenshots in portrait (520x700) and wide (1100x700), compared side by side with
  the mockup.
- Every main-page control works: record, import, ask, widget, settings, project picker,
  search, play, row menu and the scrollbar.
- Recording state shown (timer running, meter moving).
- CPU, memory and exe size within budget.
- `cargo fmt`, `clippy -D warnings`, tests.

### Copy rule for dialogs

Dialog titles drop their question mark in every theme ("Delete clip_005", not
"Delete clip_005?"), per the short-copy rule (Max, 2026-10-30).

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
