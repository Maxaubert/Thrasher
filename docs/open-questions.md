# Open questions for Max

Collected while building on 2026-10-23 while Max was away. Each has the choice I made
for now, so nothing is blocked. Answer whenever; the most important are first.

1. **GitHub Actions did not run on PR #14:** "recent account payments have failed or
   your spending limit needs to be increased" (Billing & plans in GitHub settings).
   Private repos pay for Windows runner minutes (counted at 2x). Fix billing, or make
   CI run only on demand. fmt, clippy and all tests pass locally.
2. **Your DJI MIC MINI recorded pure silence** (level exactly 0) in my test, while
   system audio worked. Probably the transmitter was off or asleep. Please record a
   few seconds with the mic on and check the level meter moves.
3. **Widget close button.** It now hides Thrasher to the tray (Quit is in the tray
   menu). Is that what you meant by "close to remove it", or should it quit?
4. **Global hotkey.** Ctrl+Alt+R starts and stops recording from any app. Keep it, or
   pick another? (It is a setting: `hotkey` in `%APPDATA%\Thrasher\config\settings.json`.)
5. **Export default.** Export writes 24-bit FLAC into an `export` folder inside the clip
   folder, with shortened pauses applied. A picker next to Export switches to WAV
   (24-bit) or MP3 (192 kbit/s, LAME). Is FLAC the right default?
6. **What gets transcribed when a clip has system audio:** right now the mic and
   system audio mixed together, so both sides of a meeting are in one transcript.
   Alternative: mic only, or two transcripts.
7. **Silence tags** appear for pauses of 1 s or more. Keep 1 s?
8. **Memory target.** The plan says under 40 MB idle. The window uses about 32 MB at idle
   (one measurement with the tray showed 42 MB), with Slint's software renderer; the GPU
   renderer used 80 MB. Fine as is?
9. **License.** The crates say `UNLICENSED` (private, all rights reserved) as a
   placeholder. Keep private, or pick a license?
10. **Old transcripts.** `VoiceClips\transkript\clip_001.json` is from 14 October, but
   `clip_001.wav` is the 17-minute recording from 22 October, so they do not match.
   The app only shows an old transcript when its length matches the clip.
11. **Word timing for cuts (2026-10-23).** Whisper's word times are rough (off by up to
   about 0.15 s, and words sit back to back), so cut edges now snap to the quiet point
   before the next word. If cuts still sound off, the proper fix is a forced aligner
   (a wav2vec2 model for Norwegian, run after Whisper) that finds where each word
   really starts and ends. It is a larger job and one more model download. Worth it?
12. **Voice: does it sound like you? (2026-10-23)** Typing a new word now speaks it in
   your voice (Chatterbox). Listening files are listed in the chat. If Chatterbox is not
   convincing, OmniVoice scored about the same on a voice-match test and is faster, but
   its license forbids commercial use of the output. Keep Chatterbox, or try OmniVoice?
   **Max's answer (2026-10-23):** natural, but not his voice, and a Stavanger accent
   instead of his Bokmål. Settings and a longer sample do not help (similarity 0.70 to
   0.72 vs 0.81 for his own recordings). Options and measurements:
   `research/thrasher/2026-10-23-norwegian-voice-accent.md`. Next choice: Chatterbox V2,
   MOSS-TTS with a Norwegian LoRA, or fine-tuning on his own recordings.
   **Listening test (2026-10-23):** OmniVoice best by ear, then Chatterbox with a 20 s
   sample; the 10 s sample had the wrong dialect. The app now uses 20 s. In the app,
   Chatterbox still had the wrong dialect. **Decision:** an engine setting in Models,
   OmniVoice by default, Chatterbox (MIT) for a build that may be sold. **Later the same
   day:** OmniVoice only; Chatterbox removed. Thrasher cannot be sold while it uses it.
13. **Voice: uv for other people.** The voice engine is installed with `uv`. On your PC
   it is there; for a public release the app should download uv itself, or ship a
   ready Python environment. Decide before the first public release.

## Added 2026-10-24 (Ember UI build)

14. **An old Thrasher is still recording.** A test build from 23 October
    (`targetoicetest5`) has been recording `clip_006` in your real VoiceClips since
    18:14 that day; the file was 7.5 GB at 05:48 the next morning and still growing.
    **Done 2026-10-24:** Max had it deleted (12.3 h, 7.9 GB). Recordings now stop by
    themselves after a limit set in Settings, 2 hours by default (PR #16).
15. **The DJI mic recorded silence again** in the UI test on 24 October (level stays at
    the floor). Same as question 2.
16. **Edit screen look.** The mockup shows cut words crossed out and typed words in
    orange inside the editor. Slint's text editor cannot colour parts of its text, so
    the editor is a plain text box in an orange frame, with the change count and new
    length above it. A custom editor that draws the marks is possible later. Worth it?
17. **Speed** keeps the pitch (a small time-stretch in `thrasher-core/src/stretch.rs`).
    Steps are 0.75, 1.0, 1.25, 1.5 and 2.0. Other steps wanted?
18. **The widget** got the same Ember style as a pill: record button, live level bars,
    timer, open-window and hide buttons. Not designed in the mockups yet.
