"""Thrasher voice worker: speaks phrases in the user's own voice.

Thrasher runs this in its own Python environment (set up with uv on first use) and
reads its output, like the transcription worker.

Usage: python worker.py <reference.wav> <language> <jobs.json> <output folder>
jobs.json is a list of {"name": ..., "text": ...}. Each phrase is written as
<output folder>/<name>.wav. Prints `progress <0..1>` lines, then `done`.

The model is Chatterbox Multilingual V3 by Resemble AI (MIT license). It copies the
voice from the reference recording; nothing is trained. Every output carries
Resemble's inaudible Perth watermark, which marks it as generated speech.
"""

import json
import os
import sys

os.environ.setdefault("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")


def main() -> None:
    reference, language, jobs_path, out_dir = sys.argv[1:5]
    with open(jobs_path, encoding="utf-8") as f:
        jobs = json.load(f)

    print("progress 0", flush=True)
    import soundfile
    import torch
    from chatterbox.mtl_tts import ChatterboxMultilingualTTS

    device = "cuda" if torch.cuda.is_available() else "cpu"
    model = ChatterboxMultilingualTTS.from_pretrained(device=device, t3_model="v3")
    for i, job in enumerate(jobs):
        wav = model.generate(job["text"], language_id=language, audio_prompt_path=reference)
        samples = wav.squeeze(0).detach().cpu().numpy()
        path = os.path.join(out_dir, job["name"] + ".wav")
        soundfile.write(path, samples, model.sr, subtype="PCM_16")
        print(f"progress {(i + 1) / len(jobs)}", flush=True)
    print("done", flush=True)


if __name__ == "__main__":
    try:
        main()
    except Exception as e:  # The app shows the last stderr line.
        print(f"{type(e).__name__}: {e}", file=sys.stderr, flush=True)
        sys.exit(1)
