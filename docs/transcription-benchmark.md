# Transcription benchmark (2026-10-23)

The one-day spike from the research report, run on Max's PC (RTX 5090, CUDA 13.4).
Audio: `VoiceClips\clip_001.wav`, a 17 min 23 s Norwegian presentation (44.1 kHz, 16-bit mono).
Both engines used NB-Whisper Large, language `no`, beam size 5, no voice-activity filter.

| Engine | Time | Words |
|---|---|---|
| faster-whisper 1.2.1 (CTranslate2, fp16), Python | 52 s (+9 s load) | 1,686 |
| whisper.cpp in whisper-rs 0.16 (GGML f16, CUDA, DTW) | 80 s | 1,683 |

Word timing, whisper.cpp compared with faster-whisper (1,546 words matched, 92% text agreement):

| | Median start difference | Within 0.2 s | 90th percentile |
|---|---|---|---|
| Raw DTW times | 0.21 s | 48% | 0.75 s |
| DTW times moved 0.15 s earlier | **0.08 s** | **72%** | 0.62 s |

whisper.cpp put words about 0.2 s later than faster-whisper, consistently, so Thrasher
moves DTW times 0.15 s earlier (`DTW_LEAD_SECS` in `crates/thrasher-core/src/whisper.rs`).
faster-whisper is the reference because Max's video tooling uses it and he finds its
timing good.

Conclusions:
- whisper.cpp is about 1.5x slower than faster-whisper here, but at 13x real time a
  17-minute clip still takes under 1.5 minutes. Turning on flash attention would help, but
  whisper.cpp disables it when DTW is on.
- Timing is close to the current setup after the offset. Forced alignment could tighten
  it further later.
- The model download is checked against Hugging Face's SHA-256.

How it was measured: `examples/transcribe.rs` for whisper.cpp, a small faster-whisper
script for the baseline, and a `difflib` word alignment to compare start times. The
scripts lived in the session scratchpad and are not kept.

## Other engines looked at

Checked 2026-11-01 by the Flare session; report at
`C:\Users\Admin\Documents\Claude\research\flare\2026-11-01-nemotron-asr.md`.

- **Nemotron 3.5 Transcribe 30B-A4B** (NVIDIA, released 2026-10-28): speech encoder plus a
  30B MoE language model, about 4.3B active. 40 languages, word timestamps, diarization,
  translation, no streaming. Best open English (4.71 mean WER, Parakeet v3 5.58, Whisper
  large-v3 6.66), but Norwegian is unusable (63.76 WER on FLEURS; NVIDIA says the Norwegian
  data "did not yield usable quality"). About 21 GB of VRAM or RAM loaded. Not for Thrasher:
  NB-Whisper stays far better on Norwegian.
- **Nemotron 3.5 ASR Streaming 0.6B** (released 2026-06-04): 742 MB, streams, Norwegian WER
  18.1 against 6.6 for NB-Whisper, no word timestamps on the model card. Not for Thrasher.
- If either is ever wanted: a second engine behind the `Transcriber` trait, in its own process.
