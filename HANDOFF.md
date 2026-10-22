# HANDOFF: read this once, then delete it

Briefing from the session that planned Thrasher on 2026-10-22. Read it, use what is useful, then `git rm HANDOFF.md`. Nothing here is a requirement that is not also in `docs/plan.md` or the issues, except the environment facts and gotchas below.

## What happened

Max asked for a Rust recorder, "like the Python one but more polished". We chose the name Thrasher (a bird that copies any sound), created the private repo, wrote `docs/plan.md`, and created the epic (#1) with 11 child issues (#2 to #12). Max added features along the way. Markers and tags were offered and skipped (#13 closed).

## Decisions already made with Max

- Name Thrasher. Owner Maxaubert. Private. Rust. Windows first.
- Voice visualizer: **adaptive frequency bars** scaled to the speaker's own voice range after a short calibration. Low voices must still fill the display, bass rumble is ignored, louder speech moves the bars more.
- Filename structure is a **template with placeholders** (`{date}_{time}_{n}`), not a regex. A regex cannot build names. Max wrote "regex" but the goal is configurable naming and date formats.
- Default file type setting like OBS: WAV, FLAC, MP3.
- Transcripts get **word timestamps** and **silence tags** like `[silence:7s]`. Editing a tag to `[silence:2s]` shortens that pause in the exported audio.
- Synced transcript player: highlight each word as spoken, click a word to seek.
- Record **mic and system audio at the same time** (meetings) as two tracks plus an optional mix.
- Dark, minimalist, strong contrast, one accent color. Wireframes first, including a settings page and a model download page.
- Not decided: UI toolkit; container for two tracks; MP3 encoder.

## Environment facts (this machine)

- Windows 11. Rust 1.99 (stable, msvc) is installed. `gh` is logged in as Maxaubert. Repo cloned at `C:\Users\Admin\Documents\Claude\Github\Thrasher`.
- GPU: RTX 5090.
- The old Python recorder is `C:\Users\Admin\Documents\Claude\Tools\voice-recorder\recorder.pyw` (Tkinter and sounddevice, launched by `Voice Recorder.cmd`). Parity baseline: mono 16-bit WAV at **44100 Hz**, clips in `~/Music/VoiceClips` as `clip_NNN.wav`. Keys: Space toggles recording, Enter plays, Delete deletes the selected clip, Backspace deletes the last clip, Up and Down move the selection, F2 renames, Escape stops playback. Colors used there: background `#1e1e1e`, text `#e8e8e8`, accent `#3b82f6`, recording `#ef4444`.

## Transcription gotcha (important for issue #8)

The plan says `whisper-rs` (whisper.cpp). **The Whisper models already on this machine are CTranslate2 format, not the GGML format whisper.cpp needs.**

- Cached in `~/.cache/huggingface/hub`: `NbAiLab/nb-whisper-large`, `NbAiLab/nb-whisper-large-verbatim` and `Systran/faster-whisper-large-v3` (all `model.bin` CTranslate2).
- They run today through Python 3.12 with `faster-whisper` 1.2.1 on the GPU. CUDA libraries came from `pip install nvidia-cublas-cu12 nvidia-cudnn-cu12`, and the DLL folders must be added with `os.add_dll_directory` plus PATH.
- Measured: a 17.4 minute Norwegian recording transcribed in about 72 seconds with `nb-whisper-large`, float16, beam 5, VAD filter on.
- Decide in issue #8 between: (a) `whisper-rs` with GGML models (check whether NB-Whisper has GGML builds, or convert), (b) a CTranslate2 route from Rust (for example a `ctranslate2` binding), or (c) a Python sidecar. Do not assume option (a) is a drop-in.

## Sidecar format Max already uses

His existing tooling stores transcripts as JSON with per-word timestamps in `~/Music/VoiceClips/transkript/clip_NNN.json`:
`{"file": "clip_001.wav", "duration": 19.036, "words": [{"id": "w0", "text": "La", "start": 0.6, "end": 1.12}, ...]}`
Making Thrasher's sidecar a superset of that keeps his video workflow (word-synced motion graphics) working. Add silence entries and a language field on top.

## Suggested first steps

1. Read `docs/plan.md` and the epic #1.
2. Start with the mockups (#3) as HTML pages, and open them for Max in **Zen** (`"C:\Program Files\Zen Browser\zen.exe" "file:///C:/path/page.html"`), not the default app. Show: recorder view with the adaptive voice bars, clip list with timeline and synced transcript, settings page, model download page.
3. Then phase 0 (#2) on a branch with a PR, then phase 1 (#4).
