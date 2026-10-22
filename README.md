# Thrasher

A small, fast, polished audio recorder for Windows, written in Rust.

The thrasher is a bird that copies almost any sound it hears and sings it back. This app does the same: it listens, keeps what you say as clean clips, and plays it back.

**Status:** planning. No code yet. See [docs/plan.md](docs/plan.md).

## Why

Thrasher replaces a quick Python and Tkinter recorder that saves numbered WAV clips in `~/Music/VoiceClips`. That one works, but it is basic. Thrasher keeps the same simple flow (press a key, talk, stop, clip saved) and adds the things you reach for when you record often:

- a live level meter and waveform
- a device picker and pause/resume
- global hotkeys and a tray icon, so you can record without switching windows
- markers, trimming, normalizing and export
- an optional one-click transcript with a local Whisper model

## Goals

- Starts instantly and uses almost no CPU or memory while idle.
- Never loses a recording. Audio is written to disk as it is captured.
- Keyboard first, but pleasant to use with a mouse.
- Everything runs locally. No account and no network needed.

## Non-goals

- Multi-track mixing or a full audio editor.
- Cloud sync.
- Streaming or live broadcast.

## Repository layout (planned)

```
crates/
  thrasher-core/   audio capture, playback, file formats, clip library (no UI)
  thrasher-app/    window, tray, hotkeys
docs/
  plan.md          spec and implementation plan
```

## License

Not decided yet. The repository is private.
