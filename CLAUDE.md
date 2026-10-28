# Thrasher

A polished, dark, minimalist audio recorder for Windows, written in Rust. Replaces a small Python recorder. Private repo `Maxaubert/Thrasher`.

## Where things are

- `docs/plan.md`: spec, architecture, phases, agreed features. The source of truth for scope.
- GitHub issues: epic #1, child issues #2 to #12. #13 (markers and tags) was skipped on purpose.
- `README.md`: the short pitch.
- `docs/building.md`: how to build. The app and the transcription worker must be built with **two separate cargo commands** (feature unification would put CUDA in the app).
- `docs/device-themes.md`: spec and plan for the 15 device themes (mockups in `docs/mockups/device-themes/`).
- `docs/open-questions.md`: questions waiting for Max. Check it at the start of a session.
- `docs/transcription-benchmark.md`: whisper.cpp vs faster-whisper speed and word timing.
- Tech stack research (2026-10-23): `C:\Users\Admin\Documents\Claude\research\thrasher\2026-10-23-tech-stack.md`, linked from `docs/plan.md` section 7.

## Rules for this repo

- Global rules from `~/.claude/CLAUDE.md` apply. In short: code changes go on a branch and a PR, never straight to main; docs (`*.md`, `docs/`) may go to main; merge a PR only after asking Max with AskUserQuestion; no em-dashes.
- Max uses text-to-speech. Write chat replies in English and translate any Norwegian terms. Keep replies short and structured.
- Plan before building. Max wants to be part of design decisions, so ask when something is a real choice (UI toolkit, container formats, naming) and do not decide it silently.
- Mockups come before UI code (issue #3).

## Technical decisions

- Rust workspace: `thrasher-core` (no UI, fully testable) and `thrasher-app`.
- Audio callback: only copy samples into a lock-free ring buffer. No allocation, locks, logging or file I/O there.
- Write WAV incrementally on a worker thread. A crash must lose under a second of audio.
- Clip sidecar JSON next to each clip. Keep the WAV plain and portable.
- Everything local. The only network use is downloading transcription models on demand.
- UI toolkit: Slint with the software renderer (idle about 32 to 40 MB; FemtoVG used 80 MB). The window is frameless with its own title bar and Ember controls (no std-widgets ComboBox). It follows the "Ember" mockups (orange on warm black, canvas linked in `docs/plan.md` section 12). Fonts Inter Tight and JetBrains Mono (plus Caveat and Oswald for Tape Deck) are embedded from `crates/thrasher-app/ui/fonts` (OFL). Device skins: `docs/device-themes.md`, art tools in `tools/skin-art/`.
- Transcription: whisper.cpp (whisper-rs) in a separate worker process, `thrasher-transcribe.exe`.
- Test against a scratch folder with the env var `THRASHER_DIR`, never Max's real `VoiceClips`.
- Never commit audio clips, models or personal paths.

## Quality bar

`cargo fmt`, `cargo clippy -- -D warnings`, tests (use a fake audio device so CI needs no hardware), GitHub Actions on Windows.
