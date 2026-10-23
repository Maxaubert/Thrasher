# Adding words in your own voice: plan

Agreed with Max on 2026-10-23: "plan then build it". Research:
`C:\Users\Admin\Documents\Claude\research\thrasher\2026-10-23-voice-cloning.md`.

## Goal

In the transcript editor, type a word that was never said, and hear it in your own
voice, fitted into the sentence. Fully local. The recording itself is never changed.

## Decisions

- **Two models get a listening test**: Chatterbox Multilingual V3 (MIT) and OmniVoice
  (weights non-commercial). VoxCPM2 is skipped: biggest, and no clear quality edge.
  Max picks by ear. The app supports the winner; the other can be added later.
- **No training.** Both models clone a voice from a few seconds of reference audio
  ("zero-shot"). Thrasher picks that reference automatically from the same clip, so the
  microphone and the room match.
- **Too little speech, no generation.** The reference needs at least 5 s of clean
  speech (10 s is better). If the clip has less, Thrasher says so and does nothing.
- **Runtime: a Python worker run by `uv`.** chatterbox.cpp would avoid Python, but it
  has no Windows build guide and no V3 support yet. The worker is a small script in the
  repo; the first use creates its environment (PyTorch with CUDA 12.8, about 3 to 4 GB)
  in `%LOCALAPPDATA%\Thrasher\voice`. The installer does not grow. chatterbox.cpp stays
  the plan for a later, Python-free version.
- **Transparency.** Generated words are marked synthetic in the sidecar and shown
  differently in the transcript. Chatterbox's built-in watermark stays on.

## How a new word is made

1. Max types new words in Edit and applies. Words that match no audio become an
   *insert* (stored in the sidecar) instead of being ignored.
2. Thrasher picks the reference: the longest stretch of continuous speech in the clip,
   up to 20 s (a 20 s sample kept the accent better than 10 s in a listening test), between two pauses.
3. The voice worker generates a short phrase: the word before, the new words, and the
   word after, so the new words get natural sentence intonation.
4. Thrasher transcribes that phrase with the existing whisper worker to find where the
   new words are, and cuts them out at quiet points (same snapping as word cuts).
5. The result is matched in loudness to the words around it and saved as
   `<clip>.voice/<id>.wav`. Playback and export splice it in with the usual crossfade.

## Steps

1. Listening test: 6 Norwegian sentences per model in Max's voice, plus two spliced
   insert examples. Files in `docs/` are not committed (audio); they go to a scratch
   folder and the path is reported.
2. Core: `voice.rs` (reference picking, inserts in the sidecar, splice planning),
   insert pieces in the edit plan, tests.
3. Worker: `voice/worker.py` plus the Rust side that sets up the environment and runs it.
4. UI: "Generating voice…" progress, inserted words shown in a different colour,
   errors such as "This clip has only 3.2 s of speech; at least 5 s is needed".
5. Verify on a scratch copy, never on Max's real clips.
