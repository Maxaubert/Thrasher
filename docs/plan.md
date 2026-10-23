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
