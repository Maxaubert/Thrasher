<div align="center">
  <img src="assets/icon/thrasher-colored.svg" alt="Thrasher logo" width="128">

  # Thrasher

  A dark, minimalist audio recorder for Windows, with local transcription.

  [![Release v1.0.0](https://img.shields.io/badge/release-v1.0.0-ff6a00?style=flat-square)](https://github.com/Maxaubert/Thrasher/releases/latest)
  [![Windows 10 | 11](https://img.shields.io/badge/Windows-10%20%7C%2011-ff6a00?style=flat-square)](https://github.com/Maxaubert/Thrasher/releases/latest)
  [![Built with Rust](https://img.shields.io/badge/built%20with-Rust-ff6a00?style=flat-square)](docs/building.md)
  [![Licence: not decided](https://img.shields.io/badge/licence-not%20decided-ff6a00?style=flat-square)](#licence)

  [Download](https://github.com/Maxaubert/Thrasher/releases/latest) · [Releases](https://github.com/Maxaubert/Thrasher/releases) · [Developer docs](docs/plan.md)
</div>

The thrasher is a bird that copies almost any sound it hears and sings it back. Thrasher does the
same: press record, talk, stop, and your words are kept as clean clips you can play, trim, export
and read back as text. Everything runs on your own PC, with no account and no cloud.

## Features

- Records the microphone, system audio, or both, written to disk as it records, so a crash loses
  under a second.
- A clip library with projects, search, trim, merge and export to FLAC, WAV or MP3.
- Local transcription with whisper.cpp on the GPU, Norwegian NB-Whisper by default, with word
  timing and an optional translation to English.
- A local AI that summarises clips and names new recordings from what was said.
- Typed-in words in the transcript are spoken in your own voice (OmniVoice).
- Import audio and video by dropping files on the window.
- A global hotkey, a tray icon and a compact widget, so you can record from any app.
- 20 themes: 8 classic colour themes and 12 device skins, from a tape deck to a pinball table.
- Light when idle: the window uses about 40 MB, and the heavy parts only run while they work.

## Install

[Download the installer](https://github.com/Maxaubert/Thrasher/releases/latest)
(`Thrasher-Setup-x64-<version>.exe`) and run it.

- Needs 64-bit Windows 10 or 11. No administrator rights: it installs for your user only, into
  `%LOCALAPPDATA%\Programs\Thrasher`, with a Start menu shortcut and an uninstaller.
- Transcription and the AI need an NVIDIA card, RTX 20 series or newer, with a current driver.
  Recording, playback and export work on any PC.
- Transcription and AI models download from Settings into `%LOCALAPPDATA%\Thrasher\models`.
- Speaking typed-in words needs [uv](https://docs.astral.sh/uv/) (`winget install astral-sh.uv`);
  Thrasher sets up its Python environment (several GB) on first use.
- Uninstalling keeps your recordings, settings and models.

The installer is not signed, so SmartScreen warns on first run: choose **More info**, then **Run
anyway**. The download is large (about 780 MB), mostly NVIDIA's cuBLAS library for the GPU
features.

## Usage

- **Record:** press the record button, or set a global hotkey in Settings to start and stop from
  any app. Clips are saved in `Music\VoiceClips` (changeable in Settings).
- **Play and edit:** click a clip to play it, open it to trim, read or edit its transcript, and
  export it.
- **Transcribe:** new clips are transcribed automatically (turn it off in Settings), or press
  Transcribe on any clip.
- **Themes:** Settings > Appearance. Drag a theme row sideways or use its scroll bar to browse.
- **Close:** the close button quits; turn on "Close button hides to tray" in Settings > Window to
  keep Thrasher running in the tray.

## Limits

- Windows only.
- Transcription and the AI need an NVIDIA GPU; there is no CPU fallback yet.
- Typed-in speech uses OmniVoice, whose weights are non-commercial (see [Licence](#licence)).

## Development

A Rust workspace: `thrasher-core` (audio, clips, transcription, no UI), `thrasher-app` (the Slint
window, tray and hotkeys), `thrasher-ui` (the generated UI code) and `thrasher-ask` (the local AI
worker). The app and the transcription worker are built with **two separate cargo commands**;
see [docs/building.md](docs/building.md), which also covers the installer and releases.
Run the tests with `cargo test --release -p thrasher-app` (a debug build of the UI needs over
30 GB of RAM). The spec and plan are in [docs/plan.md](docs/plan.md), the themes in
[docs/device-themes.md](docs/device-themes.md).

## Licence

Not decided yet. The repository is private.

Typed-in words are spoken by **OmniVoice** by k2-fsa, which Thrasher downloads on first use: code
Apache-2.0, model weights [CC-BY-NC 4.0](https://huggingface.co/k2-fsa/OmniVoice). Speech made
with it may **not be used commercially**, so Thrasher as a whole cannot be sold while it uses it.
