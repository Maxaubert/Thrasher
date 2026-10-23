"""Thrasher voice worker: speaks phrases in the user's own voice.

Thrasher runs this in its own Python environment (set up with uv on first use) and
reads its output, like the transcription worker.

Usage: python worker.py <reference.wav> <reference.txt> <language> <jobs.json> <out dir>
reference.txt holds what is said in the reference. <language> is not used yet (the
model detects it). jobs.json is a list of {"name": ..., "text": ...}. Each phrase is
written as <out dir>/<name>.wav. Prints `progress <done> <next>` lines (the app moves its bar
slowly towards <next> while a step runs), then `done`.

The voice is copied from the reference recording; nothing is trained. The model is
OmniVoice (k2-fsa): code Apache-2.0, model weights CC-BY-NC 4.0, so its output may not
be used commercially.
"""

import json
import os
import sys

os.environ.setdefault("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")


def progress(done: float, next_: float) -> None:
    print(f"progress {done:.3f} {next_:.3f}", flush=True)


def main() -> None:
    reference, reference_txt, language, jobs_path, out_dir = sys.argv[1:6]
    with open(jobs_path, encoding="utf-8") as f:
        jobs = json.load(f)
    for job in jobs:
        name = job["name"]
        if os.path.basename(name) != name or name in ("", ".", ".."):
            raise ValueError(f"bad output name: {name!r}")

    # Importing PyTorch and loading the model take most of the first minute.
    progress(0.0, 0.15)
    import soundfile
    import torch

    device = "cuda" if torch.cuda.is_available() else "cpu"
    progress(0.15, 0.6)
    from omnivoice import OmniVoice

    with open(reference_txt, encoding="utf-8") as f:
        ref_text = f.read().strip()
    model = OmniVoice.from_pretrained(
        "k2-fsa/OmniVoice",
        device_map="cuda:0" if device == "cuda" else "cpu",
        dtype=torch.float16 if device == "cuda" else torch.float32,
    )

    def speak(text):
        # No language: Max's listening test (2026-10-23) used auto-detect.
        return model.generate(text=text, ref_audio=reference, ref_text=ref_text)[0], 24000

    for i, job in enumerate(jobs):
        done = 0.6 + 0.4 * i / len(jobs)
        progress(done, 0.6 + 0.4 * (i + 1) / len(jobs))
        samples, rate = speak(job["text"])
        path = os.path.join(out_dir, job["name"] + ".wav")
        soundfile.write(path, samples, rate, subtype="PCM_16")
    progress(1.0, 1.0)
    print("done", flush=True)


if __name__ == "__main__":
    try:
        main()
    except Exception as e:  # The app shows the last stderr line.
        print(f"{type(e).__name__}: {e}", file=sys.stderr, flush=True)
        sys.exit(1)
