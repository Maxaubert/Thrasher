# Thrasher

A polished, dark, minimalist audio recorder for Windows, written in Rust. Replaces a small Python recorder. Private repo `Maxaubert/Thrasher`.

**If `HANDOFF.md` exists, read it first, then delete it** (`git rm HANDOFF.md` in your next docs commit). It is a one-time briefing from the planning session.

## Where things are

- `docs/plan.md`: spec, architecture, phases, agreed features. The source of truth for scope.
- GitHub issues: epic #1, child issues #2 to #12. #13 (markers and tags) was skipped on purpose.
- `README.md`: the short pitch.

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
- UI toolkit is undecided: Slint, egui or Tauri. Decide after phase 1.
- Never commit audio clips, models or personal paths.

## Quality bar

`cargo fmt`, `cargo clippy -- -D warnings`, tests (use a fake audio device so CI needs no hardware), GitHub Actions on Windows.
