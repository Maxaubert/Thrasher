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
8. **Memory target.** The plan says under 40 MB idle. The window now uses about 31 MB
   without the tray and 42 MB with it (Slint's software renderer; the GPU renderer
   used 80 MB). Is 42 MB fine, or should the tray be optional?
9. **License.** The crates say `UNLICENSED` (private, all rights reserved) as a
   placeholder. Keep private, or pick a license?
10. **Old transcripts.** `VoiceClips\transkript\clip_001.json` is from 14 October, but
   `clip_001.wav` is the 17-minute recording from 22 October, so they do not match.
   The app only shows an old transcript when its length matches the clip.
