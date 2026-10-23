# Building Thrasher

## Plain build (no transcription)

```
cargo run --release -p thrasher-app
```

Needs only Rust (stable, MSVC). This is what CI builds and tests.

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
```

Build them with **two commands**, in this order. Cargo merges features across the
binaries of one build, so building both with `--features whisper-cuda` puts CUDA into
`thrasher.exe` too (18 MB becomes 71 MB, and idle memory goes from about 32 MB to 200 MB).

Transcription runs in `thrasher-transcribe.exe`, which the app starts for each job and
finds next to itself. The app shows the Transcribe button only when the worker is there.
Running it as its own process means the model and the CUDA runtime (about 600 MB of RAM
and 3 GB of video memory) are released when the job ends, and a whisper.cpp crash cannot
stop a recording. The first CUDA build takes several minutes.

Models are downloaded from the app (Models, top right) into
`%LOCALAPPDATA%\Thrasher\models`. They are never committed.

## Command-line examples

```
# Record 5 s of mic + system audio into a folder
cargo run -p thrasher-core --example record -- <folder> 5

# Transcribe a clip and write words and pauses into its sidecar JSON
cargo run --release -p thrasher-core --features whisper-cuda --example transcribe -- <model.bin> <clip.wav> no
```
