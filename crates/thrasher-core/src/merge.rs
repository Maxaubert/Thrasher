//! Merging clips into one, and taking a merged clip apart again.
//!
//! A merged clip is an ordinary clip whose recording is its parts back to back. Its
//! sidecar remembers each part (`parts`: the original name and where it sits), so the
//! parts can be put in another order or one can be taken out as its own clip later.
//! Transcripts, edits and spoken inserts move along with their part.
//!
//! Merging sends the original clips to the Recycle Bin; their audio lives on in the
//! merged clip.

use crate::library::{Library, LibraryError};
use crate::sidecar::{Insert, Part, RecordingState, Sidecar, Silence, Track, TrackKind, Word};
use crate::transcribe::resample;
use crate::voice::{insert_file, voice_dir};
use crate::wav::{self, WavWriter};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

/// A stretch of a clip that goes into a new clip.
#[derive(Clone, Debug, PartialEq)]
struct Piece {
    clip: String,
    from: f64,
    to: f64,
    /// Name the part keeps in the new clip.
    label: String,
}

/// A clip's sidecar, or for a bare WAV without one, what can be read from the file.
fn sidecar_of(library: &Library, name: &str) -> io::Result<Sidecar> {
    let path = library.sidecar_path(name);
    if path.exists() {
        return Sidecar::load(&path);
    }
    let mic = library.track_path(name, TrackKind::Mic);
    let info = wav::read_info(&mic)?;
    let file = format!("{name}.wav");
    let mut tracks = vec![Track {
        kind: TrackKind::Mic,
        file: file.clone(),
        sample_rate: info.spec.sample_rate,
        channels: info.spec.channels,
        device: None,
    }];
    let system = library.track_path(name, TrackKind::System);
    if let Ok(s) = wav::read_info(&system) {
        tracks.push(Track {
            kind: TrackKind::System,
            file: format!("{name}.system.wav"),
            sample_rate: s.spec.sample_rate,
            channels: s.spec.channels,
            device: None,
        });
    }
    let mut sc = Sidecar {
        file,
        duration: info.duration_secs(),
        ..Default::default()
    };
    sc.thrasher.tracks = tracks;
    Ok(sc)
}

/// The parts of a clip: those it remembers, or the whole clip as one part.
pub fn parts(library: &Library, name: &str) -> io::Result<Vec<Part>> {
    let sc = sidecar_of(library, name)?;
    Ok(if sc.parts.is_empty() {
        vec![Part {
            name: name.into(),
            start: 0.0,
            end: sc.duration,
        }]
    } else {
        sc.parts
    })
}

/// Merges `names`, in that order, into a new clip called `into`. The originals go to
/// the Recycle Bin. A clip that is itself merged brings its own parts along.
pub fn merge(library: &Library, names: &[String], into: &str) -> Result<(), LibraryError> {
    if !crate::library::valid_name(into) {
        return Err(LibraryError::BadName(into.into()));
    }
    if library.exists(into) && !names.iter().any(|n| n == into) {
        return Err(LibraryError::Exists(into.into()));
    }
    let mut pieces = Vec::new();
    for name in names {
        for p in parts(library, name)? {
            pieces.push(Piece {
                clip: name.clone(),
                from: p.start,
                to: p.end,
                label: p.name,
            });
        }
    }
    let staged = build(library, &pieces, into)?;
    for name in names {
        library.delete(name)?;
    }
    staged.commit(library, into)?;
    Ok(())
}

/// Puts the parts of a merged clip in a new order: `order[i]` is the index of the part
/// that comes `i`th.
pub fn reorder(library: &Library, name: &str, order: &[usize]) -> Result<(), LibraryError> {
    let old = parts(library, name)?;
    let mut seen = vec![false; old.len()];
    if order.len() != old.len()
        || order
            .iter()
            .any(|&i| i >= old.len() || std::mem::replace(&mut seen[i], true))
    {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "not an order of the parts").into(),
        );
    }
    let pieces: Vec<Piece> = order
        .iter()
        .map(|&i| Piece {
            clip: name.into(),
            from: old[i].start,
            to: old[i].end,
            label: old[i].name.clone(),
        })
        .collect();
    let staged = build(library, &pieces, name)?;
    staged.replace(library, name)?;
    Ok(())
}

/// Takes part `index` out of a merged clip as a clip of its own, named after the part
/// (with a number added if that name is taken). Returns the new clip's name.
pub fn extract(library: &Library, name: &str, index: usize) -> Result<String, LibraryError> {
    let old = parts(library, name)?;
    if old.len() < 2 || index >= old.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "no such part to take out").into());
    }
    let p = &old[index];
    let mut new_name = p.name.clone();
    let mut n = 2;
    while library.exists(&new_name) || new_name == name {
        new_name = format!("{} ({n})", p.name);
        n += 1;
    }
    let single = Piece {
        clip: name.into(),
        from: p.start,
        to: p.end,
        label: p.name.clone(),
    };
    let rest: Vec<Piece> = old
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != index)
        .map(|(_, p)| Piece {
            clip: name.into(),
            from: p.start,
            to: p.end,
            label: p.name.clone(),
        })
        .collect();
    let mut taken = build(library, &[single], &new_name)?;
    // One part on its own is a plain clip again.
    taken.sidecar.parts.clear();
    let mut kept = build(library, &rest, name)?;
    if rest.len() == 1 {
        kept.sidecar.parts.clear();
    }
    taken.commit(library, &new_name)?;
    kept.replace(library, name)?;
    Ok(new_name)
}

/// A new clip written into a hidden folder of the library, not yet in place.
struct Staged {
    dir: PathBuf,
    sidecar: Sidecar,
}

impl Staged {
    /// Moves the new clip's files into the library as `name`.
    fn commit(self, library: &Library, name: &str) -> io::Result<()> {
        self.sidecar.save(&self.dir.join(format!("{name}.json")))?;
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            std::fs::rename(entry.path(), library.dir().join(entry.file_name()))?;
        }
        std::fs::remove_dir(&self.dir)
    }

    /// Replaces the clip `name` with the new one. The old files are removed for good:
    /// the new clip holds the same audio.
    fn replace(self, library: &Library, name: &str) -> io::Result<()> {
        for kind in [TrackKind::Mic, TrackKind::System] {
            let p = library.track_path(name, kind);
            if p.exists() {
                std::fs::remove_file(p)?;
            }
        }
        let mix = library.dir().join(format!("{name}.mix.wav"));
        if mix.exists() {
            std::fs::remove_file(mix)?;
        }
        let voices = voice_dir(library, name);
        if voices.exists() {
            std::fs::remove_dir_all(voices)?;
        }
        std::fs::remove_file(library.sidecar_path(name))?;
        self.commit(library, name)
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        // Left over only when something failed half way.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Converts interleaved audio to `ch` channels (1 or 2) and `rate`.
fn convert(samples: &[f32], from_ch: u16, from_rate: u32, ch: u16, rate: u32) -> Vec<f32> {
    let from_ch = from_ch.max(1) as usize;
    let chans: Vec<Vec<f32>> = (0..from_ch)
        .map(|c| samples.iter().skip(c).step_by(from_ch).copied().collect())
        .collect();
    let chans: Vec<Vec<f32>> = chans.iter().map(|c| resample(c, from_rate, rate)).collect();
    let frames = chans.iter().map(Vec::len).min().unwrap_or(0);
    let mut out = Vec::with_capacity(frames * ch as usize);
    for f in 0..frames {
        if ch == 1 {
            out.push(chans.iter().map(|c| c[f]).sum::<f32>() / from_ch as f32);
        } else {
            out.push(chans[0][f]);
            out.push(chans[from_ch.min(2) - 1][f]);
        }
    }
    out
}

type Audio = (wav::WavSpec, Vec<f32>);

/// Writes the pieces back to back as a new clip called `name` in a hidden folder of
/// the library, with the transcript, edits and inserts of each piece moved along.
fn build(library: &Library, pieces: &[Piece], name: &str) -> io::Result<Staged> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = library.dir().join(format!(".merging-{stamp}"));
    std::fs::create_dir_all(&dir)?;
    let mut staged = Staged {
        dir: dir.clone(),
        sidecar: Sidecar::default(),
    };

    let mut sidecars: HashMap<String, Sidecar> = HashMap::new();
    for p in pieces {
        if !sidecars.contains_key(&p.clip) {
            sidecars.insert(p.clip.clone(), sidecar_of(library, &p.clip)?);
        }
    }
    // The first clip's rate; stereo when any part is stereo.
    let track_of = |sc: &Sidecar, kind| sc.thrasher.tracks.iter().find(|t| t.kind == kind).cloned();
    let first = &sidecars[&pieces[0].clip];
    let rate = track_of(first, TrackKind::Mic).map_or(48_000, |t| t.sample_rate);
    let mut ch = 1;
    let mut has_system = false;
    for p in pieces {
        let sc = &sidecars[&p.clip];
        if track_of(sc, TrackKind::Mic).is_some_and(|t| t.channels > 1) {
            ch = 2;
        }
        has_system |= library.track_path(&p.clip, TrackKind::System).exists();
    }

    let mic_name = format!("{name}.wav");
    let sys_name = format!("{name}.system.wav");
    let mut mic = WavWriter::create(&dir.join(&mic_name), rate, ch)?;
    let mut sys = if has_system {
        Some(WavWriter::create(&dir.join(&sys_name), rate, ch)?)
    } else {
        None
    };
    let out_voices = dir.join(format!("{name}.voice"));

    let sc_out = &mut staged.sidecar;
    let mut insert_ids: HashMap<(usize, String), String> = HashMap::new();
    // The clip whose audio is loaded: its name, mic track and system track.
    let mut loaded: Option<(String, Audio, Option<Audio>)> = None;
    for (k, p) in pieces.iter().enumerate() {
        if loaded.as_ref().is_none_or(|(c, _, _)| *c != p.clip) {
            let m = wav::read_f32(&library.track_path(&p.clip, TrackKind::Mic))?;
            let sp = library.track_path(&p.clip, TrackKind::System);
            let s = if sp.exists() {
                Some(wav::read_f32(&sp)?)
            } else {
                None
            };
            loaded = Some((p.clip.clone(), m, s));
        }
        let (_, (mspec, msamples), system) = loaded.as_ref().unwrap();
        let slice = |spec: &wav::WavSpec, samples: &[f32]| -> Vec<f32> {
            let c = spec.channels.max(1) as usize;
            let total = samples.len() / c;
            let a = ((p.from * spec.sample_rate as f64).round() as usize).min(total);
            let b = ((p.to * spec.sample_rate as f64).round() as usize).clamp(a, total);
            convert(
                &samples[a * c..b * c],
                spec.channels,
                spec.sample_rate,
                ch,
                rate,
            )
        };
        let start = mic.frames() as f64 / rate as f64;
        let m = slice(mspec, msamples);
        let frames = m.len() / ch as usize;
        mic.write(&m)?;
        if let Some(w) = &mut sys {
            let mut s = match system {
                Some((spec, samples)) => slice(spec, samples),
                None => Vec::new(),
            };
            s.resize(frames * ch as usize, 0.0);
            w.write(&s)?;
        }
        let end = mic.frames() as f64 / rate as f64;
        let shift = start - p.from;
        let inside = |t: f64| t >= p.from && t < p.to;

        let src = &sidecars[&p.clip];
        if sc_out.language.is_none() {
            sc_out.language = src.language.clone();
        }
        for w in src.words.iter().filter(|w| inside(w.start)) {
            sc_out.words.push(Word {
                id: format!("w{}", sc_out.words.len()),
                start: w.start + shift,
                end: (w.end.min(p.to)) + shift,
                ..w.clone()
            });
        }
        for s in src.silences.iter().filter(|s| inside(s.start)) {
            sc_out.silences.push(Silence {
                start: s.start + shift,
                end: s.end.min(p.to) + shift,
                keep: s.keep,
            });
        }
        let src_voices = voice_dir(library, &p.clip);
        for ins in src.inserts.iter().filter(|i| inside(i.at)) {
            let id = format!("v{}", sc_out.inserts.len());
            insert_ids.insert((k, ins.id.clone()), id.clone());
            let mut file = None;
            if ins.file.is_some() {
                let from = src_voices.join(insert_file(&ins.id));
                if from.exists() {
                    std::fs::create_dir_all(&out_voices)?;
                    std::fs::copy(&from, out_voices.join(insert_file(&id)))?;
                    file = Some(insert_file(&id));
                }
            }
            sc_out.inserts.push(Insert {
                id,
                at: ins.at + shift,
                file,
                ..ins.clone()
            });
        }
        let first_insert =
            sc_out.inserts.len() - src.inserts.iter().filter(|i| inside(i.at)).count();
        for ins in &mut sc_out.inserts[first_insert..] {
            ins.with = ins
                .with
                .as_ref()
                .and_then(|w| insert_ids.get(&(k, w.clone())).cloned());
        }
        sc_out.parts.push(Part {
            name: p.label.clone(),
            start,
            end,
        });
    }
    let duration = mic.frames() as f64 / rate as f64;
    mic.finalize()?;
    let mut tracks = vec![Track {
        kind: TrackKind::Mic,
        file: mic_name.clone(),
        sample_rate: rate,
        channels: ch,
        device: None,
    }];
    if let Some(w) = sys {
        w.finalize()?;
        tracks.push(Track {
            kind: TrackKind::System,
            file: sys_name,
            sample_rate: rate,
            channels: ch,
            device: None,
        });
    }
    sc_out.file = mic_name;
    sc_out.duration = duration;
    sc_out.thrasher.version = 1;
    sc_out.thrasher.state = RecordingState::Complete;
    sc_out.thrasher.created = chrono::Local::now().to_rfc3339();
    sc_out.thrasher.tracks = tracks;
    Ok(staged)
}

/// True when `dir` is a folder [`build`] left behind (the app was closed mid-merge).
fn is_leftover(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(".merging-"))
}

/// Removes folders that a merge left behind when the app was closed half way.
pub fn clean_leftovers(library: &Library) {
    let Ok(entries) = std::fs::read_dir(library.dir()) else {
        return;
    };
    for e in entries.flatten() {
        if is_leftover(&e.path()) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::WavWriter;

    /// A clip of `secs` seconds at `rate`, with one word per second.
    fn clip(lib: &Library, name: &str, rate: u32, secs: usize, level: f32) {
        let mut w = WavWriter::create(&lib.track_path(name, TrackKind::Mic), rate, 1).unwrap();
        w.write(&vec![level; rate as usize * secs]).unwrap();
        w.finalize().unwrap();
        let sc = Sidecar {
            file: format!("{name}.wav"),
            duration: secs as f64,
            words: (0..secs)
                .map(|i| Word {
                    id: format!("w{i}"),
                    text: format!("{name}{i}"),
                    start: i as f64 + 0.1,
                    end: i as f64 + 0.6,
                    cut: false,
                })
                .collect(),
            thrasher: crate::sidecar::Meta {
                version: 1,
                state: RecordingState::Complete,
                created: String::new(),
                tracks: vec![Track {
                    kind: TrackKind::Mic,
                    file: format!("{name}.wav"),
                    sample_rate: rate,
                    channels: 1,
                    device: None,
                }],
            },
            ..Default::default()
        };
        sc.save(&lib.sidecar_path(name)).unwrap();
    }

    fn texts(lib: &Library, name: &str) -> Vec<String> {
        let sc = Sidecar::load(&lib.sidecar_path(name)).unwrap();
        sc.words.into_iter().map(|w| w.text).collect()
    }

    #[test]
    fn merge_reorder_and_take_out() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        clip(&lib, "a", 16_000, 2, 0.1);
        clip(&lib, "b", 8_000, 1, 0.5);
        merge(&lib, &["b".into(), "a".into()], "ab").unwrap();
        assert_eq!(lib.names().unwrap(), ["ab"]);
        assert_eq!(texts(&lib, "ab"), ["b0", "a0", "a1"]);
        let p = parts(&lib, "ab").unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].name.as_str(), p[1].name.as_str()), ("b", "a"));
        assert!((p[0].end - 1.0).abs() < 0.01 && (p[1].end - 3.0).abs() < 0.01);
        // "b" was at 8 kHz and now plays at the first clip's rate.
        let (spec, samples) = wav::read_f32(&lib.track_path("ab", TrackKind::Mic)).unwrap();
        assert_eq!(spec.sample_rate, 8_000);
        assert!((samples[100] - 0.5).abs() < 0.05);

        reorder(&lib, "ab", &[1, 0]).unwrap();
        assert_eq!(texts(&lib, "ab"), ["a0", "a1", "b0"]);
        let sc = Sidecar::load(&lib.sidecar_path("ab")).unwrap();
        assert!((sc.words[2].start - 2.1).abs() < 0.01);
        assert!(reorder(&lib, "ab", &[0, 0]).is_err());

        let taken = extract(&lib, "ab", 0).unwrap();
        assert_eq!(taken, "a");
        assert_eq!(texts(&lib, "a"), ["a0", "a1"]);
        assert_eq!(texts(&lib, "ab"), ["b0"]);
        // What is left is a plain clip again.
        assert_eq!(parts(&lib, "ab").unwrap().len(), 1);
        assert!(
            Sidecar::load(&lib.sidecar_path("ab"))
                .unwrap()
                .parts
                .is_empty()
        );
        assert!(
            std::fs::read_dir(dir.path())
                .unwrap()
                .all(|e| !is_leftover(&e.unwrap().path()))
        );
    }

    #[test]
    fn bare_wav_without_sidecar_merges() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        clip(&lib, "a", 8_000, 1, 0.1);
        let mut w = WavWriter::create(&lib.track_path("bare", TrackKind::Mic), 8_000, 1).unwrap();
        w.write(&[0.3; 16_000]).unwrap();
        w.finalize().unwrap();
        merge(&lib, &["a".into(), "bare".into()], "both").unwrap();
        let p = parts(&lib, "both").unwrap();
        assert!((p[1].end - 3.0).abs() < 0.01);
    }

    #[test]
    fn merge_refuses_a_taken_name() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        clip(&lib, "a", 8_000, 1, 0.1);
        clip(&lib, "b", 8_000, 1, 0.1);
        clip(&lib, "c", 8_000, 1, 0.1);
        assert!(matches!(
            merge(&lib, &["a".into(), "b".into()], "c"),
            Err(LibraryError::Exists(_))
        ));
        // Merging into the name of one of the merged clips is fine.
        merge(&lib, &["a".into(), "b".into()], "a").unwrap();
        assert_eq!(texts(&lib, "a"), ["a0", "b0"]);
    }
}
