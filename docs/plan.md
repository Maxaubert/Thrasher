# Thrasher: spec and implementation plan

Status: draft for approval, 2026-10-22. Owner: Max Aubert.

## 1. Purpose

A small desktop audio recorder for Windows. The primary use is capturing spoken clips quickly (notes, voice-over, practice runs) and keeping them organized. It replaces `Tools/voice-recorder/recorder.pyw` (Python, Tkinter, sounddevice).

## 2. What the Python recorder does today (parity baseline)

Thrasher must do at least all of this:

- Record from the default microphone as mono, 16-bit PCM WAV.
- Start and stop with one control (toggle).
- Save each clip as `clip_NNN.wav` in `~/Music/VoiceClips`, numbering continues from the highest existing number.
- List the clips, newest last, with their length.
- Play the selected clip.
- Rename a clip.
- Delete the selected clip, or the last clip.
- Move the selection up and down.

## 3. Features beyond parity

Grouped by priority. Phase numbers refer to section 6.

| Feature | Why it matters | Phase |
|---|---|---|
| Input device picker, remembered between runs | Several microphones are common | 1 |
| Pause and resume while recording | Avoids many short clips | 1 |
| Live level meter with peak hold and clip warning | See immediately if the mic is dead or too loud | 2 |
| Live waveform while recording; waveform in the clip list | Faster to find the right clip | 2 |
| Seek, loop and playback speed for clips | Reviewing speech | 2 |
| Rename in place, delete with undo, drag to reorder | Basic library hygiene | 2 |
| Dark and light themes | Polish | 2 |
| Global start/stop hotkey | Record while another app has focus | 3 |
| Tray icon with recording state, and a small always-on-top mini mode | Always available, never in the way | 3 |
| Markers added with a hotkey during recording, saved with the clip | Find important moments later | 4 |
| Trim, fade, normalize | Clean clips without another tool | 4 |
| Auto-split on silence | Long takes become separate clips | 4 |
| Export to FLAC and MP3 as well as WAV | Smaller files, sharing | 4 |
| Record system audio (WASAPI loopback), alone or mixed with the mic | Capture a call or video | 5 |
| One-click transcript using a local Whisper model (for example the Norwegian NB-Whisper) | Searchable clips | 5 |
| Full-text search across transcripts and clip names | Find clips by what was said | 5 |

## 4. Architecture

Two crates in one Cargo workspace:

- `thrasher-core`: everything that is not UI. Fully testable without a window or a real microphone.
  - `capture`: audio input via `cpal`. The audio callback only copies samples into a lock-free ring buffer (`ringbuf`). It never allocates, locks or does I/O.
  - `writer`: a worker thread drains the ring buffer and writes WAV (`hound`) incrementally, so a crash loses at most a fraction of a second. WAV headers are finalized on stop. If the app dies, an unfinalized file can be repaired on next start.
  - `playback`: audio output via `cpal`, with seek and speed control.
  - `meter`: peak and RMS levels, and downsampled waveform data, computed on the worker thread and published to the UI.
  - `library`: the clip folder, naming, metadata, undo of deletes (moves to a hidden `.trash` folder).
  - `metadata`: a small JSON sidecar per clip (`clip_007.json`): markers, transcript, waveform cache, tags. The WAV file stays plain and portable.
  - `export`: FLAC and MP3 encoders behind a trait, so formats can be added without touching the rest.
  - `transcribe` (optional, behind a Cargo feature): local Whisper through `whisper-rs`.
- `thrasher-app`: window, tray, hotkeys, settings. Talks to core only through a small command and event interface (channels), so the UI toolkit can be swapped.

Candidate crates (to be confirmed when each phase starts): `cpal`, `hound`, `ringbuf`, `rubato` (resampling), `global-hotkey`, `tray-icon`, `directories` (paths), `serde` and `serde_json`, `thiserror`, `tracing`, `flacenc`, a pure-Rust or bundled MP3 encoder, `whisper-rs`.

UI toolkit: **undecided**. Options are Slint (matches the Flare project, native and light), egui (fastest to build) and Tauri (easiest to make pretty, heavier). Core has no dependency on any of them, so the choice can wait until the end of phase 1, when a thin throwaway CLI already proves recording and playback.

## 5. Quality bar

- Audio thread rules: no allocation, no locks, no logging in the callback.
- `cargo fmt` and `cargo clippy -- -D warnings` clean.
- Unit tests for the library, naming, metadata and meter math. Integration tests use a fake in-memory audio device so they run in CI without hardware.
- GitHub Actions: format, clippy and tests on Windows.
- Idle cost: under 1% CPU and under 40 MB memory with the window closed to tray.
- Latency from hotkey to first recorded sample under 100 ms.
- Settings and clips never leave the machine.

## 6. Milestones

Each phase ends with something runnable.

0. **Scaffold.** Cargo workspace, CI, lint config, `.gitignore`, license decision, empty crates. Done when CI is green.
1. **Core recording.** Capture, incremental WAV writing, clip numbering and the library, playback, device picker, pause and resume, behind a minimal CLI. Done when it matches the Python recorder's behaviour and survives a killed process.
2. **Polished window.** Pick the UI toolkit. Level meter, live waveform, clip list with waveforms, rename and delete with undo, themes, settings.
3. **Background use.** Global hotkey, tray icon, mini mode, start with Windows (optional).
4. **Editing and export.** Markers, trim, fade, normalize, auto-split on silence, FLAC and MP3 export.
5. **Extras.** System-audio loopback, local transcription, search.

## 7. Risks and open questions

Tech stack research (2026-10-23), with recommended defaults for each open choice: `C:\Users\Admin\Documents\Claude\research\thrasher\2026-10-23-tech-stack.md`. Short version: Slint (egui fallback, prototype first), whisper.cpp with NbAiLab's own GGML files (ct2rs as plan B), the `wasapi` crate instead of cpal for capture, one WAV per source with a custom RF64-capable writer, flacenc, LAME behind a feature, sonora for noise suppression and auto gain.

Voice cloning research (2026-10-23), for typing new words that are spoken in Max's voice: `C:\Users\Admin\Documents\Claude\research\thrasher\2026-10-23-voice-cloning.md`. Short version: no open model edits Norwegian speech in place, so generate the sentence in Max's voice and splice in only the new word. Candidates with Norwegian: Chatterbox Multilingual (MIT, C++ port exists), VoxCPM2 (Apache-2.0), OmniVoice (most Norwegian data, but weights CC-BY-NC). Not started; needs a listening test first.

- **UI toolkit** choice (see section 4). Decide after phase 1.
- **Loopback capture** on Windows needs WASAPI-specific handling and may not fit `cpal` cleanly. Prototype early in phase 5 and keep a fallback.
- **MP3 encoding** licensing and crate choice. FLAC and WAV first.
- **Whisper** model size versus startup time. Make it optional and download-on-demand, never bundled.
- **Global hotkeys** can conflict with other apps. Make them configurable.
- **Resampling** if the device only supports a rate other than 44.1 or 48 kHz. Use `rubato`.
- **Format default.** Mono 16-bit at 44.1 kHz matches the current recorder. Decide whether to offer stereo and 24-bit.

## 8. Decisions already made

- Name: **Thrasher** (a bird that mimics every sound it hears).
- Repository: `Maxaubert/Thrasher`, private.
- Language: Rust.
- Platform: Windows first. The core stays portable.

## 9. Additions agreed 2026-10-22

Tracked as issues under the epic [#1](https://github.com/Maxaubert/Thrasher/issues/1).

- **Output settings:** default output folder, filename template with placeholders (for example `{date}_{time}_{n}`) and configurable date formats, and a default file type like OBS (WAV, FLAC, MP3). A regex can validate a template but cannot build names, so the template is the main mechanism.
- **Microphone choice** and **system audio**, including **both at the same time** (for example a meeting): two separate tracks plus an optional mix, with gain and a level meter per source.
- **Transcription:** optional local models downloaded on demand from the settings page (for example Whisper, including a Norwegian model if available), a language setting, and **word-level timestamps** stored next to the clip.
- **Silence tags in the transcript**, for example `[silence:7s]`. Editing the tag to `[silence:2s]` shortens that pause in the exported audio. Non-destructive and undoable.
- **Synced transcript player:** words are highlighted as they are spoken, clicking a word jumps to it, and search works across all transcripts.
- **Subtitle export** (SRT, VTT, word-level JSON) and **edit by transcript** (delete words to cut audio).
- **Voice visualizer:** adaptive frequency bars scaled to the speaker's own voice range after a short calibration, so a low voice still fills the display and bass rumble is ignored. Louder speech moves the bars more.
- **Noise suppression and auto gain** (local), and **auto-split on silence**.
- **Global hotkeys, tray icon and mini mode.**
- **UI:** minimalist, modern, dark with strong contrast and one accent color. Wireframes and mockups come before the window is built (issue [#3](https://github.com/Maxaubert/Thrasher/issues/3)), including a settings page, a model download page and the clip library with a timeline.
- **Skipped for now:** markers and tags (issue [#13](https://github.com/Maxaubert/Thrasher/issues/13), closed as not planned; can be reopened).
- **Still undecided:** the UI toolkit.

## 10. Next steps

1. Approve this plan or change it.
2. Wireframes and mockups for the dark UI.
3. Phase 0 on a branch with a pull request.
4. Phase 1.

## 11. Decisions 2026-10-23

Max accepted the research defaults (see section 7 for the report link):

- **UI:** Slint. The first window is deliberately bare and will be rebuilt after the mockups (#3). A short Slint vs egui measurement can still overturn this.
- **Transcription:** whisper.cpp with NbAiLab's GGML models, behind a `Transcriber` trait. Word timing from DTW is good enough (Max: current faster-whisper timings "look alright"). Forced alignment is a possible later upgrade.
- **Capture:** the `wasapi` crate (not cpal), mic as master clock, system audio by process loopback that excludes Thrasher, endpoint loopback as fallback.
- **Recording format:** 32-bit float at the device's native rate, one WAV per source (`mic`, `system`), custom incremental writer that switches to RF64 past 4 GiB. Mix rendered on demand.
- **Export:** flacenc for FLAC, LAME (`mp3lame-encoder`) behind an `mp3` feature, Opus later.
- **DSP:** sonora (noise suppression, auto gain) applied at export or as an extra track, raw mic always kept.
- **Widget:** Max wants a small draggable widget (mic button, close to hide) and an always-on-top pin. Exact look decided in the mockups.
- **Build order:** build as much as possible now; keep UI minimal.

## 12. Mockups (2026-10-24, issue #3)

20 main-screen directions on a Claude Design canvas (private link): https://claude.ai/artifact/SU1FDLC9SZgMi9sXMe6wbB

**2026-10-26:** Max asked to clear the canvas. It now has one page, "Screens", with 17 boards made from headless screenshots of the current app in Ember: main, search, clip menu, projects, delete dialog and wide main; clip page, translate menu, summary, editor and wide clip page; Settings, Models, Ask and Merge; the widget ready and recording. All earlier boards (directions, rounds, window sizes, themes) were removed. The screenshot script is `capture.sh` in the session scratchpad, not in the repo.

- **Max picked 02 Ember:** orange (#ff6a1a) on warm black, Inter Tight with JetBrains Mono for times, 520×600 window.
- Each clip row has a play button (plays from the main view) and a `>` that opens a **manage screen** for that clip: player with seek, speed and loop, transcript view, and transcript editing (cut words, shorten silences, type new words in your voice).
- **Models are not a tab.** They live on the **settings page** (opened from a gear in the title bar) with recording, output, voice and shortcut settings.
- Ember screens on the canvas: main while a clip plays, manage clip, edit transcript, settings.
- **2026-10-24, round 2:** editing a transcript has no Preview and no Undo. There is only Cancel and **Save as new clip**; you test by playing the new clip. No help or hint text in the UI (Max removed the legend). The 19 other directions were deleted. The canvas now holds 4 variations of each screen (main, manage, edit, settings) next to the base version.
- **2026-10-24, final picks:** main screen 02 Ember, clip page "Manage: base" with circular 5 s skip buttons, edit page "Edit: base", settings "V2 flat list". Built in PR #15, with the widget in the same style.
- **2026-10-24, round 3 (Max's feedback on the built app):** the window frame must be part of the UI (own title bar, drag, minimise, close); the settings page needs a full rework; the Windows-default dropdowns, text fields and similar controls look bad and get Ember versions; the model list grows from Norwegian plus one global model to the whole whisper.cpp range (Whisper tiny to large-v3 and turbo, English-only and compressed versions, all NB-Whisper sizes, Swedish KB-Whisper). The record shortcut has no preset (fixed in PR #18). Four settings variations (row "Settings, round 3": V1 flat list with models inline, V2 sidebar, V3 cards with a separate model library page, V4 search and model table) wait for Max's pick.
- **2026-10-24, round 3 pick: V3 (cards, models on their own page).** Built in the `feat/settings-round-3` PR: the window is frameless with Thrasher's own title bar on every page (drag, minimise, close; edges still resize); Ember dropdown (`Select`), text field (`TextField`) and press-the-keys shortcut box (`ShortcutBox`) replace the Windows-style widgets; settings are grouped in cards; a Models page lists all 52 ggml models from NB-Whisper (normal and verbatim), Whisper (all languages and English only) and KB-Whisper, each also compressed, with search, language chips, a compressed toggle and quality and speed bars. Transcription languages grew from 3 to 20 plus detect.
- **2026-10-26, themes:** the canvas page "Themes" shows the main screen in 7 palettes: Ember, Volt (yellow), Glacier (blue), Mint, Violet, Signal (red), Chalk (white). Max wanted all of them in Settings (Shortcuts and window, row Theme), first with Signal as the default, then **Ember stays the default** (Max, same day). Dropdowns open upwards when there is no room below, and Up and Down step through the options at once. Picking clips works like File Explorer: Shift picks the range from the last plain or Ctrl click, Ctrl adds or removes one, Ctrl+Shift adds a range, also on the waveform. The palettes live in the `Theme` global in `common.slint`; the choice is saved as `theme` in settings.json.
- **2026-10-26, title bar and widget:** from a browser mockup (six button styles, six widgets) Max picked **F round outlines** for settings, pin and widget (pin fills while on) and the **W2 card widget** (240 x 124: status with open and hide, a big time with pause and record, the live level). Then, still 2026-10-26, a second browser mockup of six layouts: Max picked the **side rail** on the main page (Clips and Import at the top; Keep on top, Small widget and Settings at the bottom; icons after Lucide). The title bar keeps only minimise and close. Later the same day Max dropped the rail again: Import, Ask, Keep on top and Small widget now sit after the search field, and Settings sits in the title bar next to minimise. Double-click on a clip opens its page (it used to play); picking several no longer shows "N picked", Merge and Clear in the header (merging stays in the clip menu). Keep on top is gone too (Max, 2026-10-26): only the widget stays on top.
   - **Recording out of sight:** while recording, Thrasher turns into the widget when it
     is minimised, or when another app's window is maximised on its screen or covers 80
     percent of it.
   - **Not when it loses focus:** a recorder beside Notepad must stay a window.
   - **Getting the window back:** the widget stays after the recording stops. Alt-Tab or
     the taskbar brings back the full window, but only when the widget came by itself
     and the mouse is not over it.

## 13. Round 6: clip menu, import, merge, editor (agreed 2026-10-25)

Max's answers (2026-10-25): Delete sends clips to the Windows Recycle Bin with no
question; Import takes common formats (WAV, MP3, FLAC, M4A/AAC, OGG/Opus); a merged clip
remembers its parts, so a part can be pulled back out or the order changed later.

Spec:
1. **Undo delete is gone**: no "Undo delete" label, no Ctrl+Z on the list. Delete moves
   the clip's files to the Recycle Bin. The hidden `.trash` folder is no longer used.
2. **Clip row menu**: the arrow becomes a vertical three-dot button. Its menu has
   Manage (opens the clip page), Rename, Export (the export formats), Delete.
3. **Clip page**: the Delete button becomes a trash icon in the header, next to the
   rename pencil. The bottom bar keeps only Export, on the right.
4. **Export everything**: a "Whole project" entry in the export menu. You pick a folder,
   and Thrasher writes a folder named after the clip with the audio (default format),
   SRT, VTT, word timings and a timestamped transcript (`.txt`, one `[mm:ss] line` per
   subtitle line).
5. **Marks in the preview**: pause chips in orange so they stand out.
6. **Editor**: pause tags drawn in orange while you type (a coloured text layer behind
   the editable text). Ctrl+Z and Ctrl+Y undo and redo like any text box, no buttons.
   With "Show marks" off, Ctrl+C in the editor copies the words without the pause tags.
7. **Import** (main page, button next to the clip count): pick one or more audio files.
   Each becomes a new clip (decoded with Symphonia, stored as WAV). You can transcribe it
   like any recording.
8. **Merge**: Ctrl+click and Shift+click highlight several clips. Right-click (or the
   three-dot menu) gives "Merge N clips". A merge page lists them; drag rows to set the
   order, then Merge. The new clip's audio is the parts back to back (resampled to one
   rate), transcripts are joined with shifted times. The originals leave the list. The
   clip remembers its parts; on its page a Parts card lets you drag to reorder them or
   pull one part back out as its own clip.

Plan (one PR per group, each tested headlessly):
- A. Delete to Recycle Bin, remove undo, three-dot menu, trash icon, orange pause chips.
- B. Export project.
- C. Editor: coloured tags, copy without tags, check undo/redo.
- D. Import with Symphonia.
- E. Multi-select, merge page with drag reorder, parts card (reorder, extract).

Built 2026-10-25 in PRs #27 (A), #28 (B), #29 (C), #30 (D), #31 (E). Differences from the
spec: Opus files cannot be imported (Symphonia has no Opus decoder); FLAC is decoded with
`claxon` because Symphonia rejects valid FLAC files from our own encoder. "Whole project"
and a timestamped script ("Script", `.txt`) were added to the export menus.
**2026-10-26:** the rail button is just "Import" and takes video too (Max): only the sound is
kept. Symphonia reads MP4, MOV and MKV audio; anything it cannot read (Opus, WebM, AVI, WMV)
goes through ffmpeg when it is installed on the PC, so Opus now imports where ffmpeg exists.

## 14. Window sizes (mockups 2026-10-25)

Canvas rows "Window sizes: main page / manage clip / settings" (canvas linked in section 12)
show each page at narrow (420), default (520) and wide (1040). Rule from Max: a size
never adds or removes options; it only rearranges them. Narrow: one-line recorder with a
round Record button, no mini waveforms in the list, a lower player, wide controls under
their label. Wide: recorder beside the clip list; player, parts and options beside a
full-height transcript; settings cards in two columns. Waiting for Max's feedback.

## 15. Round 7: projects, auto transcribe, search, trim, translate, local AI (proposed 2026-10-26)

Max picked these after a brainstorm. Each item is its own PR, built in this order.

1. **Projects.** A project is a folder of clips, for example School, To do or a work site.
   - **Storage:** each project is a subfolder of the clip folder. The clip folder itself
     is the first project, **Inbox**, so no existing file moves.
   - **Picking a project:** the word "Clips" over the list becomes a project dropdown
     (Max's pick). It lists the projects and ends with "New project". The current project
     is remembered in settings.
   - **Where new clips go:** every recording, import and drop goes into the current
     project.
   - **Moving clips:** the clip menu gets "Move to" with the list of projects.
   - **Project menu:** rename and delete (delete only when the project is empty), and
     export the whole project as a folder.
   - **Naming clash:** today's export "Full project" (one clip with all its files) is
     renamed "All files", so "project" only means the new kind.
   - **Widget and hotkey:** they record into the current project.
2. **Drag and drop.** Drop audio or video files anywhere on the window to import them into
   the current project, the same way as the Import button. While files hover, the window
   shows a dashed outline and "Drop to import". Files Thrasher can't read are skipped with
   a status note. Built on winit's file drop events (`unstable-winit-030` is already on).
3. **Auto transcribe** (setting, on by default since 2026-10-26, card TRANSCRIPTION, row "Auto
   transcribe"). When it's on, every new clip is transcribed as soon as it's saved:
   recordings, imports and dropped files. Several new clips go to one worker run (the
   worker already takes more clip names). Clips that arrive while a job runs wait in a
   queue.
4. **Shorten pauses** (setting, card TRANSCRIPTION, row "Max pause": Off, 0.3 s, 0.5 s,
   1 s, 2 s; default Off).
   - After each transcription, every pause longer than the limit is set to the limit.
   - It's a normal non-destructive edit, the same as typing a shorter `[silence]` tag.
     Playback and export use it, and the editor can change it back.
   - Chosen instead of starting the recording on voice, which can cut off the first word.
5. **Copy transcript.** A copy icon in the transcript card header. A click copies the text
   without pause tags, and the icon turns into a check mark for 1.5 s. Tooltip only.
6. **Search.** A search field next to the project dropdown (Ctrl+F).
   - **Filtering:** typing filters the list to clips whose name or transcript contains the
     text.
   - **Hits:** each hit shows the matching words under the clip name, with the match in
     the accent colour.
   - **Playing a hit:** a click opens the clip page and plays from that word. Esc clears.
   - **Scope:** a toggle searches all projects instead of the current one.
7. **Trim by waveform.** On the clip page, the player waveform gets a start handle and an
   end handle.
   - Dragging them hides the audio outside, non-destructively: a new sidecar field `trim`,
     used by playback, export and the edited length.
   - Words outside the trim are shown as cut. Dragging a handle back restores the audio.
8. **Translate.** "Translate to English" in the transcript card menu.
   - **Target:** Whisper can only translate into English, so that is the only target.
   - **Storage:** the result is saved next to the transcript (sidecar `translation`,
     segments with times).
   - **Display:** the card gets an Original / English switch, and the subtitles export uses
     whichever is shown.
   - **Model warning:** turbo models translate badly, and the menu says so when one is
     active.
   - **As built (2026-10-26):** NB-Whisper and KB-Whisper only transcribe; asked to
     translate, they answer in Norwegian or Swedish. So translation uses the active model
     only when it is a multilingual OpenAI Whisper model, and otherwise the best one
     downloaded (`models::TRANSLATORS`: Large v3 first, no turbo, no English-only). With
     none downloaded, Translate says so and opens the Models page. The translation is
     stored as English words with times (sidecar `translation`), so every subtitle
     format works from it.
   - **2026-10-26, language menu (Max):**
     - **The menu:** the translate icon opens a searchable list of 16 languages
       (`ai::LANGUAGES`), without the clip's own, with Original first.
     - **Missing model:** a language whose model is missing is greyed out with an
       Install link (the Whisper models, or the AI card in Settings).
     - **How each is made:** English uses Whisper when a model can, so it keeps word
       times. Every other language, and English without such a model, is translated
       by the local AI in parts of 1,200 words and stored as text (sidecar
       `translations`).
     - **Which AI model:** the AI translation always uses Ministral 3B. In a test,
       Borealis answered translation requests with the text unchanged, while Ministral
       translated well, into Nordic languages, Polish and Ukrainian too.
9. **Local AI** (research: `C:\Users\Admin\Documents\Claude\research\thrasher\2026-10-26-local-llm.md`).
   - **Summary:** "Summary" in the transcript card menu writes a short summary and a to-do
     list, saved in the sidecar.
   - **Ask:** an "Ask" item in the side rail opens a question box over the current project,
     or all projects. The answer lists the clips and moments it used, and a click plays
     from there.
   - **Models:** downloaded on demand, like the Whisper models, on the Models page.
     - Default: **NbAiLab borealis-open-4b**, Q4 from NB's own GGUF, 2.5 GB, tuned for
       Norwegian.
     - Smaller fallback: **Ministral-3-3B-Instruct-2512**, 2.2 GB.
     - Before settling on the default, A/B test both on a few of Max's own transcripts.
   - **Runtime:** a new worker `thrasher-ask.exe` (its own crate and cargo command), using
     `llama-cpp-2` with optional CUDA.
     - It talks over stdin and stdout like `thrasher-transcribe`.
     - It never shares an exe with whisper-rs, because the two ggml copies clash on
       Windows.
   - **How answers are found:**
     - First, pick the clips by project, date and name ("the last lecture" means the newest
       clip).
     - Then search the transcripts by keyword (tantivy BM25, Norwegian stemmer).
     - The model reads only the best passages, with their times, and answers in the
       language of the question.
     - Long transcripts are summarised in chunks.
   - **9a built 2026-10-26 (Summary):** a Summary button (sparkles) on the transcript
     card, a Summary view next to Original and English, written live as it streams.
     Settings has an AI card (model and download). Prompts are in Norwegian for Norwegian
     transcripts; more than 2,500 words are summarised in parts, then as a whole.
   - **A/B on clip_001** (17 min Norwegian lecture, RTX 5090): both took about 4 s.
     - **Borealis:** concise and in the asked format, but the to-do list is generic and
       repeats itself.
     - **Ministral:** more specific (names, tools, firm), but too long, and it puts
       non-tasks in the to-do list.
     - Borealis stays the default (Max's pick, 2026-10-26).
   - **9b built 2026-10-26 (Ask):**
     - **How it answers:** an Ask item in the side rail opens the Ask page (question,
       All projects, answer written live, numbered sources that play from their moment).
       Transcripts are cut into passages of 120 words, overlapping 30. Plain BM25 picks
       the best 8: stop words are dropped, words are cut to 6 letters, and there is no
       extra dependency in place of tantivy. A question with "siste", "forrige", "last"
       and similar reads only the newest clip.
     - **Tested on the lecture:** Borealis answered in Norwegian in about 2 s, but it
       left out the [n] citations and answered only half of a two-part question.

## 16. Themes with character (agreed 2026-10-27)

The old colour-only themes (Volt, Glacier, Mint, Violet, Signal, Chalk) felt boring. Max
wants themes that stand out from each other, some with subtle graphics or motion behind
the recorder. Mockups: [docs/mockups/themes.html](mockups/themes.html) (open in a browser,
press Recording to see how each one reacts).

**Locked in by Max (2026-10-27).** Ember stays the default and unchanged.

| Theme | Look | Motion |
|---|---|---|
| Snow Night | Ice blue on night slate, snow cap on the recorder card | Slow falling snow |
| Snow Daylight | Light theme, white cards, blue accent | Grey blue snow |
| Glass Graphite | Smoked dark glass, one teal glow | Glow drifts slowly |
| Sumi Sakura | Ink black, faint 録音 and ensō on the recorder, square pink seal button | A few falling petals |
| Sumi Kintsugi | Black lacquer, gold cracks across the recorder, gold accent | Slow shine along the cracks |
| Ocean Lagoon | Turquoise water, see-through cards | Sun rays sway, small bubbles rise |
| Cyberpunk | Still neon outlines on the recorder and toolbar | A red, green and blue snake runs round the recorder |

The old colour themes (Volt, Glacier, Mint, Violet, Signal, Chalk) are removed (Max,
2026-10-27). A saved old theme name falls back to Ember.

**Built 2026-10-27:**
- Each theme has one effect: on the window (`WindowFx`) and on the recorder card
  (`CardFx`), in `common.slint`. They move with `Fx.t`, a clock the app advances; `Fx.slow`
  is the same clock in quarter seconds for the big glows.
- The clock stops for Ember, in the widget, and while the window is hidden or minimised.
  It runs twice as fast while recording.
- Aurora is removed and Sakura's highlighted clip is pink (Max, 2026-10-27).
- Nebula is removed (Max, 2026-10-28). A saved "Nebula" falls back to Ember.
- Max found the first step rates (4 to 30 a second) choppy, so effects now step 60 times a
  second. Slow drifts (glows, rays, stars) follow `Fx.slow`, which steps 20 times a
  second, and the glows and Lagoon's water and rays are small images (`ui/fx`) scaled up:
  the software renderer draws those far faster than gradients. CPU of one core at
  520x700 on the test copy (32 threads, so under 1% of the whole CPU):

  | Ember | Snow | Daylight | Graphite | Sakura | Kintsugi | Lagoon | Cyberpunk |
  |---|---|---|---|---|---|---|---|
  | 0% | 21% | 18% | 8% | 16% | 14% | 30% | 12% |

- 録音 is drawn from Noto Serif JP Black glyph outlines (OFL), since the app embeds no
  Japanese font.
- Sumi must not use the orange red accent (Max, 2026-10-27). Slint's software renderer has
  no backdrop blur, so the translucent cards sit over soft glows instead.

### Device themes (2026-10-28 to 2026-11-02)

Spec and per-skin notes: [device-themes.md](device-themes.md). 12 device skins (15 until
Max removed Phosphor, Spatial Glass and Fighter Select on 2026-11-06) on top of
the classic themes, each with its own main page and the full device look on every other
page (clip, settings, models, merge, Ask, dialogs, menus, widget), chosen in a theme
gallery in Settings.

- Main pages built 2026-10-28 to 2026-10-30. Other pages: Tape Deck first, then Phosphor,
  Desktop 95, Comic Book, Spatial Glass, Car Dashboard (#120 to #125), Pocket, Quest Menu,
  Brick Phone, Answering Machine, Pocket Synth (#126 to #130), Fighter Select, Cozy Cabin,
  Pinball and Phonograph (#132 to #134, #136). Widget variant A for every skin.
- Generated code: by the eleventh skin Slint's output was 467 MB of Rust, the exe 246 MB,
  and one rustc needed over 30 GB. Two size passes (#131, #135) brought it to about 180 MB
  and 111 MB with all 15 skins, every theme pixel identical. The rules new UI code must
  follow are in device-themes.md, "Generated code size".
- Builds: one cargo command at a time, tests in release (docs/building.md).
