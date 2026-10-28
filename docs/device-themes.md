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
- **Exe size:** grows by at most 15 MB in all.

## 4. Theme picker

With 23 themes, a dropdown is too long. Settings gets a theme gallery: small thumbnails in
two groups, Classic and Devices. Clicking one switches at once. The thumbnails come from
the mockup screenshots.

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
