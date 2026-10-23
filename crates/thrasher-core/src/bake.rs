//! Saved edits are clips of their own (`clip_001 (edit)`). Once their new words are
//! spoken, the edit is baked in: the audio file is rewritten to hold the edited result
//! (cuts, pauses, spoken words) and the transcript is rebuilt to match, so the file is
//! the edited version wherever it is opened.

use crate::edit::{edited_words, plan};
use crate::library::Library;
use crate::player::load_clip;
use crate::sidecar::{Sidecar, Track, TrackKind};
use crate::transcribe::{DEFAULT_MIN_SILENCE, silences};
use crate::{voice, wav};
use std::io;

/// Bakes the clip's edit into its audio. Mic and system audio are mixed into one track,
/// as heard in the player.
pub fn bake(library: &Library, name: &str) -> io::Result<()> {
    let sc_path = library.sidecar_path(name);
    let sc = Sidecar::load(&sc_path)?;
    let buffer = load_clip(library, name)?;
    let voices = voice::load_voices(library, name, &sc);
    let edl = plan(&sc, &voices, &buffer);
    let out = edl.render(&buffer);
    let mut words = edited_words(&sc, &edl);
    for (i, w) in words.iter_mut().enumerate() {
        w.id = format!("w{i}");
    }
    let duration = out.duration_secs();

    let mic = library.track_path(name, TrackKind::Mic);
    let part = mic.with_extension("part");
    let mut writer = wav::WavWriter::create(&part, out.sample_rate, out.channels)?;
    writer.write(&out.samples)?;
    writer.finalize()?;
    std::fs::rename(&part, &mic)?;
    let system = library.track_path(name, TrackKind::System);
    if system.exists() {
        std::fs::remove_file(system)?;
    }
    let voice_dir = voice::voice_dir(library, name);
    if voice_dir.is_dir() {
        std::fs::remove_dir_all(voice_dir)?;
    }

    let mut baked = Sidecar {
        silences: silences(&words, duration, DEFAULT_MIN_SILENCE),
        words,
        inserts: Vec::new(),
        duration,
        ..sc
    };
    let device = baked.thrasher.tracks.first().and_then(|t| t.device.clone());
    baked.thrasher.tracks = vec![Track {
        kind: TrackKind::Mic,
        file: format!("{name}.wav"),
        sample_rate: out.sample_rate,
        channels: out.channels,
        device,
    }];
    baked.save(&sc_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply_text;
    use crate::sidecar::{Meta, Word};
    use crate::wav::WavWriter;

    /// "a b c": three 1 s tones at 1000 Hz sample rate, 0.5 s apart.
    fn make(lib: &Library, name: &str) {
        let mut samples = Vec::new();
        for _ in 0..3 {
            samples.extend((0..1000).map(|i| if i % 2 == 0 { 0.5 } else { -0.5 }));
            samples.extend(std::iter::repeat_n(0.0, 500));
        }
        let mut w = WavWriter::create(&lib.track_path(name, TrackKind::Mic), 1000, 1).unwrap();
        w.write(&samples).unwrap();
        w.finalize().unwrap();
        let word = |i: usize, t: &str| Word {
            id: format!("w{i}"),
            text: t.into(),
            start: i as f64 * 1.5,
            end: i as f64 * 1.5 + 1.0,
            cut: false,
        };
        Sidecar {
            file: format!("{name}.wav"),
            duration: 4.5,
            words: vec![word(0, "a"), word(1, "b"), word(2, "c")],
            thrasher: Meta {
                tracks: vec![Track {
                    kind: TrackKind::Mic,
                    file: format!("{name}.wav"),
                    sample_rate: 1000,
                    channels: 1,
                    device: None,
                }],
                ..Default::default()
            },
            ..Default::default()
        }
        .save(&lib.sidecar_path(name))
        .unwrap();
    }

    #[test]
    fn saved_edit_is_a_new_baked_clip() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        make(&lib, "clip_001");
        let mut sc = Sidecar::load(&lib.sidecar_path("clip_001")).unwrap();
        apply_text("a c", &mut sc).unwrap();
        let new = lib.edit_name("clip_001");
        assert_eq!(new, "clip_001 (edit)");
        lib.copy_clip("clip_001", &new, &sc).unwrap();
        bake(&lib, &new).unwrap();

        // The original is untouched.
        let original = Sidecar::load(&lib.sidecar_path("clip_001")).unwrap();
        assert!(original.words.iter().all(|w| !w.cut));
        assert!((lib.clip("clip_001").unwrap().duration - 4.5).abs() < 1e-9);
        // The edit holds two words, and its audio is shorter by the cut.
        let baked = Sidecar::load(&lib.sidecar_path(&new)).unwrap();
        let texts: Vec<&str> = baked.words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, ["a", "c"]);
        assert!(baked.words.iter().all(|w| !w.cut));
        let len = lib.clip(&new).unwrap().duration;
        assert!(len < 3.5 && len > 2.5, "{len}");
        assert!((baked.duration - len).abs() < 1e-9);
        assert_eq!(baked.file, format!("{new}.wav"));

        // The next saves count on, also when saving an edit of an edit.
        assert_eq!(lib.edit_name("clip_001"), "clip_001 (edit_2)");
        assert_eq!(lib.edit_name(&new), "clip_001 (edit_2)");
    }
}
