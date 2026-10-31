# Building Thrasher

## Plain build (no transcription)

```
cargo run --release -p thrasher-app
```

Needs only Rust (stable, MSVC). This is what CI builds and tests.

Debug builds of `thrasher-app` carry line tables only (`[profile.dev.package.thrasher-app]`
in the root `Cargo.toml`): with full debug info the test exe passed the linker's program
database limit (`LNK1140`) once the generated UI code grew with the Quest Menu and Brick Phone pages
(2026-10-31).

## With transcription (whisper.cpp)

Transcription is behind Cargo features so the plain build stays light:

- `whisper`: whisper.cpp on the CPU.
- `whisper-cuda`: whisper.cpp on an NVIDIA GPU. Needs the CUDA toolkit.

Build tools whisper.cpp needs on Windows (verified 2026-10-23 on Max's PC):

| Tool | Where it comes from here |
|---|---|
| CMake | Ships with Visual Studio: `C:\Program Files\Microsoft Visual Studio\18\Community\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin` |
| libclang (for bindgen) | `pip install --user libclang`, then point `LIBCLANG_PATH` at its `clang\native` folder. whisper-rs-sys's pre-generated bindings are made on Linux and fail to compile on Windows, so the bindings must be generated. |
| CUDA toolkit | 13.4 is installed; `CUDA_PATH` points at it |

From Git Bash:

```
export PATH="/c/Program Files/Microsoft Visual Studio/18/Community/Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin:$PATH"
export LIBCLANG_PATH="C:/Users/Admin/AppData/Roaming/Python/Python314/site-packages/clang/native"
export CMAKE_CUDA_ARCHITECTURES=120   # RTX 5090 (Blackwell); keeps the CUDA build short
cargo build --release -p thrasher-app                                          # the app
cargo build --release -p thrasher-app --features whisper-cuda --bin thrasher-transcribe  # the worker
cargo build --release -p thrasher-ask --features cuda                           # the local AI worker
```

Build them with **two commands**, in this order. Cargo merges features across the
binaries of one build, so building both with `--features whisper-cuda` puts CUDA into
`thrasher.exe` too (18 MB becomes 71 MB, and idle memory goes from about 32 MB to 200 MB).

Transcription runs in `thrasher-transcribe.exe`, which the app starts for each job and
finds next to itself. The app shows the Transcribe button only when the worker is there.
Running it as its own process means the model and the CUDA runtime (about 600 MB of RAM
and 3 GB of video memory) are released when the job ends, and a whisper.cpp crash cannot
stop a recording. The first CUDA build takes several minutes.

The local AI (summaries, later Ask) runs in `thrasher-ask.exe`, built from its own crate
with llama.cpp (`llama-cpp-2`, pinned). It must never share an exe with whisper-rs: both
carry their own ggml, which clashes on Windows. Without `--features llm` (or `cuda`) the
crate builds nothing, so the normal build and CI never compile llama.cpp. The app shows
the AI card and the Summary button only when the worker sits next to it. The first CUDA
build takes about four minutes.

Models are downloaded from the app (Settings, under Transcription) into
`%LOCALAPPDATA%\Thrasher\models`. They are never committed.

## Speaking new words (voice cloning)

Typing a new word in the transcript editor speaks it in the user's voice with
OmniVoice (k2-fsa; best Norwegian accent in Max's listening test; weights CC-BY-NC, so
not for commercial use). This needs no build step: the app runs
`crates/thrasher-app/voice/worker.py` (compiled into the app) in a Python environment
it creates with `uv` on first use, in `%LOCALAPPDATA%\Thrasher\voice\env-omnivoice`
(several GB with PyTorch; the model goes to `voice\models`). Versions are pinned in
`crates/thrasher-app/src/voice.rs`.
`uv` must be installed (`winget install astral-sh.uv`). The transcription worker must
be there too: it finds the new word in the generated speech. Plan and design:
`docs/voice-plan.md`.

## Command-line examples

```
# Record 5 s of mic + system audio into a folder
cargo run -p thrasher-core --example record -- <folder> 5

# Transcribe a clip and write words and pauses into its sidecar JSON
cargo run --release -p thrasher-core --features whisper-cuda --example transcribe -- <model.bin> <clip.wav> no
```

## Debugging and automated checks

- **Log:** `%LOCALAPPDATA%\Thrasher\thrasher.log` (also printed to stderr). It has
  startup times, how long each clip takes to read and to start playing, recording
  start and stop, transcription and waveform times, and a line whenever the window
  freezes for more than about a quarter of a second. It starts over past 1 MB.
- **`THRASHER_DIR=<folder>`:** use a scratch clip folder instead of the real one.
- **`THRASHER_SETTINGS=<file>`:** read and save settings there instead of the real
  settings file. Always set it for automated checks.
- **`THRASHER_TEST_WINDOW=1`:** the window opens without taking focus, for checks run
  while someone works at the PC. Drive it without focus or the mouse: find a button
  through UI Automation (every button has an accessible name), read its bounding
  rectangle, and post a click to the window at its centre with `PostMessage`; send
  keys with `PostMessage` too. Do not call UIA `Invoke` (or Toggle, Expand, Select,
  SetValue): measured 2026-10-28, `Invoke` brings the window to the front. Never call
  `SetForegroundWindow` or move the cursor. Run tests from a
  renamed copy of the exe so the user's own Thrasher is never stopped.
  Dialogs would take focus, so in this mode exports save without asking (into the last
  export folder) and Import reads its files from **`THRASHER_TEST_IMPORT`** (paths
  separated by `;`).
- **Hidden desktop (2026-10-30):** with several agents testing in parallel, windows still
  reached the user's screen. Run the whole test session (test exe, shots, posted input,
  any browser) from one script on a second Windows desktop made with `CreateDesktop`
  (the pattern in Prism's `tools/e2e/HiddenDesktop.cs`). Nothing there can take the
  user's foreground; the app renders and `PrintWindow` shots work (checked 2026-10-30).
  Never use `SendKeys`: it types into whatever window has focus.
