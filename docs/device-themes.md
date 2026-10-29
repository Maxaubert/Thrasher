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
4. **Theme gallery** in settings.
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
