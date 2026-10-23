//! Words typed into the transcript that were never said, spoken in the speaker's own
//! voice. The voice model runs in a separate worker (see the app); this module holds
//! the parts that need no model: picking a voice sample from the clip, building the
//! phrase to generate, finding the new words in the generated audio, and loading the
//! finished audio for playback and export.
//!
//! The model copies a voice from a few seconds of reference audio ("zero-shot"), so
//! nothing is trained. The reference comes from the same clip, so the microphone and
//! the room match.

use crate::edit::{Kind, rms_at, similar, visible};
use crate::library::Library;
use crate::player::Buffer;
use crate::sidecar::{Sidecar, TrackKind, Word};
use crate::transcribe::{resample, to_mono};
use crate::wav;
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

/// Less continuous speech than this cannot give a usable voice sample.
pub const MIN_REFERENCE_SECS: f64 = 5.0;
/// The reference is cut down to about this much speech. A 20 s sample kept Max's
/// accent clearly better than 10 s in a listening test (2026-10-23).
pub const MAX_REFERENCE_SECS: f64 = 20.0;
/// Words further apart than this belong to different stretches of speech.
const RUN_GAP_SECS: f64 = 0.6;
/// Context words generated around the new words, so they get sentence intonation.
const CONTEXT_BEFORE: usize = 3;
const CONTEXT_AFTER: usize = 2;

/// Insert ids are made by `apply_text` as `v<number>`. Ids are used in file names, so
/// anything else (from a hand-edited or foreign sidecar) is ignored.
pub fn valid_id(id: &str) -> bool {
    id.strip_prefix('v')
        .is_some_and(|n| !n.is_empty() && n.len() <= 9 && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The audio file of an insert, inside the clip's `.voice` folder.
pub fn insert_file(id: &str) -> String {
    format!("{id}.wav")
}

/// The clip has too little speech to copy the voice from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TooShort {
    pub speech_secs: f64,
}

impl std::fmt::Display for TooShort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "This clip has only {:.1} s of speech. At least {MIN_REFERENCE_SECS:.0} s is \
             needed to copy your voice, so nothing was added.",
            self.speech_secs
        )
    }
}

/// Picks the voice sample: the longest stretch of continuous speech in the recording,
/// cut to at most 20 s at a word end. When no single stretch is long enough, several
/// are joined. Fails when the clip has less than 5 s of speech in all.
pub fn pick_reference(sc: &Sidecar) -> Result<Vec<(f64, f64)>, TooShort> {
    // Stretches of speech: words closer together than RUN_GAP_SECS.
    let mut runs: Vec<Vec<&Word>> = Vec::new();
    for w in &sc.words {
        match runs.last_mut() {
            Some(run) if w.start - run.last().unwrap().end < RUN_GAP_SECS => run.push(w),
            _ => runs.push(vec![w]),
        }
    }
    let len = |r: &[&Word]| r.last().unwrap().end - r[0].start;
    // A stretch cut to at most MAX_REFERENCE_SECS, ending at a word end.
    let cut = |r: &[&Word]| {
        let start = r[0].start;
        let end = r
            .iter()
            .map(|w| w.end)
            .take_while(|&e| e - start <= MAX_REFERENCE_SECS)
            .last()
            .unwrap_or(r[0].end);
        (start, end)
    };
    let total: f64 = runs.iter().map(|r| len(r)).sum();
    if total < MIN_REFERENCE_SECS {
        return Err(TooShort { speech_secs: total });
    }
    runs.sort_by(|a, b| len(b).total_cmp(&len(a)));
    let mut picked = Vec::new();
    let mut have = 0.0;
    for r in &runs {
        let (s, e) = cut(r);
        picked.push((s, e));
        have += e - s;
        if have >= MAX_REFERENCE_SECS - 1.0 {
            break;
        }
    }
    picked.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(picked)
}

/// What is said in the voice sample: the words inside the picked stretches. Some
/// voice models need it next to the audio.
pub fn reference_text(sc: &Sidecar, segments: &[(f64, f64)]) -> String {
    sc.words
        .iter()
        .filter(|w| {
            segments
                .iter()
                .any(|&(s, e)| w.start >= s - 1e-6 && w.end <= e + 1e-6)
        })
        .map(|w| w.text.trim())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Folder that holds a clip's voice sample and generated words: `<clip>.voice`.
pub fn voice_dir(library: &Library, name: &str) -> PathBuf {
    library.dir().join(format!("{name}.voice"))
}

/// Writes the voice sample as `<clip>.voice/reference.wav`: the picked stretches of
/// the mic track, mono, with a short pause between them.
pub fn write_reference(
    library: &Library,
    name: &str,
    segments: &[(f64, f64)],
) -> io::Result<PathBuf> {
    let (spec, samples) = wav::read_f32(&library.track_path(name, TrackKind::Mic))?;
    let mono = to_mono(&samples, spec.channels);
    let rate = spec.sample_rate as f64;
    let dir = voice_dir(library, name);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("reference.wav");
    let mut out = wav::WavWriter::create(&path, spec.sample_rate, 1)?;
    for (i, &(s, e)) in segments.iter().enumerate() {
        if i > 0 {
            out.write_silence((0.3 * rate) as u64)?;
        }
        let a = (((s - 0.1).max(0.0)) * rate) as usize;
        let b = (((e + 0.1) * rate) as usize).min(mono.len());
        out.write(&mono[a.min(b)..b])?;
    }
    out.finalize()?;
    Ok(path)
}

/// One generation job: inserted words that sit together, with the words around them.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    /// Ids of the inserts, in order.
    pub ids: Vec<String>,
    pub before: Vec<String>,
    pub words: Vec<String>,
    pub after: Vec<String>,
}

impl Job {
    /// The phrase the model speaks.
    pub fn text(&self) -> String {
        [&self.before[..], &self.words[..], &self.after[..]]
            .concat()
            .join(" ")
    }
}

/// Groups the inserts that still need audio into jobs. Inserts next to each other
/// (no recorded word between them) are generated together.
pub fn jobs(sc: &Sidecar) -> Vec<Job> {
    let shown = visible(sc);
    let insert_at: HashMap<(u64, &str), &str> = sc
        .inserts
        .iter()
        .map(|i| ((i.at.to_bits(), i.text.as_str()), i.id.as_str()))
        .collect();
    let pending = |s: &crate::edit::Shown| {
        matches!(
            s.kind,
            Kind::Spoken {
                ready: false,
                failed: false
            }
        ) && insert_at
            .get(&(s.start.to_bits(), s.text.as_str()))
            .is_some_and(|id| valid_id(id))
    };
    let words_only = |range: &[crate::edit::Shown]| -> Vec<String> {
        range
            .iter()
            .filter(|s| s.kind != Kind::Pause)
            .map(|s| s.text.clone())
            .collect()
    };
    let mut out = Vec::new();
    let mut k = 0;
    while k < shown.len() {
        if !pending(&shown[k]) {
            k += 1;
            continue;
        }
        let first = k;
        while k < shown.len() && pending(&shown[k]) {
            k += 1;
        }
        let before = words_only(&shown[..first]);
        let after = words_only(&shown[k..]);
        out.push(Job {
            ids: shown[first..k]
                .iter()
                .filter_map(|s| insert_at.get(&(s.start.to_bits(), s.text.as_str())))
                .map(|id| id.to_string())
                .collect(),
            before: before[before.len().saturating_sub(CONTEXT_BEFORE)..].to_vec(),
            words: shown[first..k].iter().map(|s| s.text.clone()).collect(),
            after: after.into_iter().take(CONTEXT_AFTER).collect(),
        });
    }
    out
}

/// Where the new words are among the generated words, as an index range. Every word
/// matching is best, but the model may say a word differently from how it was typed
/// ("24." becomes "tjuefjerde"), so otherwise the first and last new word, or the
/// context words around them, mark the ends.
fn phrase_span(job: &Job, gen_words: &[Word]) -> Option<(usize, usize)> {
    let (k, n) = (job.words.len(), gen_words.len());
    if k == 0 || n == 0 {
        return None;
    }
    let like = |i: usize, w: &str| similar(&gen_words[i].text, w);
    if n >= k
        && let Some(p) = (0..=n - k)
            .filter(|&p| (0..k).all(|j| like(p + j, &job.words[j])))
            .min_by_key(|&p| p.abs_diff(job.before.len()))
    {
        return Some((p, p + k));
    }
    let b = job.before.len();
    let start = (0..n)
        .filter(|&i| like(i, &job.words[0]))
        .min_by_key(|&i| i.abs_diff(b))
        .or_else(|| match job.before.last() {
            None => Some(0),
            Some(w) => (b > 0 && b <= n && like(b - 1, w)).then_some(b),
        })?;
    let end = (start..n)
        .filter(|&i| like(i, &job.words[k - 1]))
        .min_by_key(|&i| i.abs_diff(start + k - 1))
        .map(|i| i + 1)
        .or_else(|| match job.after.first() {
            None => Some(n),
            Some(w) => (start + 1..n)
                .filter(|&i| like(i, w))
                .min_by_key(|&i| i.abs_diff(start + k)),
        })?;
    (end > start).then_some((start, end))
}

/// Finds the job's new words in the generated phrase, using the generated audio's own
/// transcript, and cuts them out at quiet points, as one piece: words next to each
/// other keep the way they were spoken together. Mono samples at the generated
/// audio's rate.
pub fn extract(job: &Job, generated: &Buffer, gen_words: &[Word]) -> Result<Vec<f32>, String> {
    let n = gen_words.len();
    let lost = || "the new words were not heard in the generated speech".to_string();
    let (s, e) = phrase_span(job, gen_words).ok_or_else(lost)?;
    let dur = generated.duration_secs();
    let mid = |w: &Word| (w.start + w.end) / 2.0;
    // Quiet point before word `i` starts (or the end, for i == n).
    let edge = |i: usize| -> f64 {
        if i == 0 {
            return 0.0;
        }
        if i == n {
            // After the last word: up to where the sound dies away.
            let last = &gen_words[n - 1];
            let mut t = last.end;
            while t < dur && rms_at(generated, t) > 0.003 {
                t += 0.01;
            }
            return (t + 0.1).min(dur);
        }
        // Whisper's times on clean generated speech are close, so cut at the quietest
        // point near its word boundary. (The "latest quiet point" used on recordings
        // could land inside the next word, at a consonant.)
        let (prev, w) = (&gen_words[i - 1], &gen_words[i]);
        let lo = (w.start - 0.1).max(mid(prev));
        let hi = (w.start + 0.06).min(mid(w)).max(lo);
        quietest(generated, lo, hi).unwrap_or(w.start)
    };
    let a = edge(s);
    let mut b = edge(e);
    // Whisper sometimes gives a short last word only a few milliseconds ("fin" came
    // out as 0.04 s), so the next word's start lies inside it. Then move the cut
    // towards the next word, to a quiet point that leaves the word a plausible length.
    let last = &gen_words[e - 1];
    let last_start = if e - 1 == s { a } else { edge(e - 1) };
    let want = min_word_secs(&last.text);
    if e < n && b - last_start < want {
        let lo = last_start + want;
        // The next word still needs its own plausible length.
        let next = &gen_words[e];
        let hi = (next.end - min_word_secs(&next.text)).max(mid(next));
        if hi > lo {
            b = quietest(generated, lo, hi).unwrap_or(lo);
        }
    }
    let mono = to_mono(&generated.samples, generated.channels);
    let rate = generated.sample_rate as f64;
    let at = |t: f64| ((t * rate) as usize).min(mono.len());
    let (a, b) = (at(a), at(b));
    if b <= a {
        return Err(lost());
    }
    Ok(mono[a..b].to_vec())
}

/// The shortest a spoken word can plausibly be: about 60 ms per letter.
fn min_word_secs(text: &str) -> f64 {
    0.06 * text.chars().filter(|c| c.is_alphanumeric()).count().max(1) as f64
}

/// The quietest point in `lo..hi`, in 5 ms steps.
fn quietest(buffer: &Buffer, lo: f64, hi: f64) -> Option<f64> {
    let steps = ((hi - lo) / 0.005) as usize;
    (0..=steps)
        .map(|i| lo + i as f64 * 0.005)
        .min_by(|a, b| rms_at(buffer, *a).total_cmp(&rms_at(buffer, *b)))
}

/// Loudness of speech in `samples`: RMS over the louder half of 20 ms frames, so
/// pauses do not pull it down.
fn speech_rms(samples: &[f32], rate: u32) -> f64 {
    let frame = (rate as usize / 50).max(1);
    let mut levels: Vec<f64> = samples
        .chunks(frame)
        .map(|c| (c.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / c.len() as f64).sqrt())
        .collect();
    if levels.is_empty() {
        return 0.0;
    }
    levels.sort_by(|a, b| b.total_cmp(a));
    let top = &levels[..levels.len().div_ceil(2)];
    top.iter().sum::<f64>() / top.len() as f64
}

/// Makes a generated word as loud as the recorded speech around it, resampled to the
/// clip's rate. `around` is mono recorded speech near the insert.
pub fn fit(word: &[f32], word_rate: u32, around: &[f32], clip_rate: u32) -> Vec<f32> {
    let target = speech_rms(around, clip_rate);
    let have = speech_rms(word, word_rate);
    let gain = if have > 1e-6 && target > 1e-6 {
        (target / have).clamp(0.1, 4.0)
    } else {
        1.0
    };
    resample(word, word_rate, clip_rate)
        .into_iter()
        .map(|x| (x as f64 * gain).clamp(-1.0, 1.0) as f32)
        .collect()
}

/// Mono recorded speech within `secs` around `t`, for loudness matching.
pub fn speech_around(sc: &Sidecar, mono: &[f32], rate: u32, t: f64, secs: f64) -> Vec<f32> {
    let r = rate as f64;
    let mut out = Vec::new();
    for w in sc.words.iter().filter(|w| (w.start - t).abs() < secs) {
        let a = ((w.start * r) as usize).min(mono.len());
        let b = ((w.end * r) as usize).min(mono.len());
        out.extend_from_slice(&mono[a..b]);
    }
    out
}

/// Loads the generated audio of every ready insert, by id: mono at the clip's rate.
pub fn load_voices(library: &Library, name: &str, sc: &Sidecar) -> HashMap<String, Vec<f32>> {
    let dir = voice_dir(library, name);
    sc.inserts
        .iter()
        .filter_map(|i| {
            // Only `<id>.wav` in the clip's own folder; never a path from the sidecar.
            let file = insert_file(&i.id);
            if !valid_id(&i.id) || i.file.as_deref() != Some(file.as_str()) {
                return None;
            }
            let (spec, s) = wav::read_f32(&dir.join(file)).ok()?;
            Some((i.id.clone(), to_mono(&s, spec.channels)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply_text;
    use crate::transcribe::number_words;

    fn words(spec: &[(&str, f64, f64)]) -> Vec<Word> {
        number_words(spec.iter().map(|&(t, a, b)| (t.into(), a, b)).collect())
    }

    #[test]
    fn ids_must_be_plain() {
        assert!(valid_id("v0") && valid_id("v123"));
        for bad in [
            "v",
            "x1",
            "v1/..",
            "../v1",
            "v1.wav",
            "v-1",
            "C:v1",
            "v1234567890",
        ] {
            assert!(!valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn reference_needs_five_seconds() {
        let sc = Sidecar {
            words: words(&[("hei", 0.0, 1.0), ("der", 1.2, 2.0), ("ja", 5.0, 6.5)]),
            ..Default::default()
        };
        let err = pick_reference(&sc).unwrap_err();
        assert!((err.speech_secs - 3.5).abs() < 1e-9);
        assert!(err.to_string().contains("3.5 s"));
    }

    #[test]
    fn reference_prefers_the_longest_stretch() {
        // 3 s stretch, then a 30 s stretch of 1 s words.
        let mut spec = vec![("a", 0.0, 1.0), ("b", 1.0, 2.0), ("c", 2.0, 3.0)];
        let long: Vec<(String, f64, f64)> = (0..30)
            .map(|i| ("w".to_string(), 10.0 + i as f64, 11.0 + i as f64))
            .collect();
        spec.extend(long.iter().map(|(t, a, b)| (t.as_str(), *a, *b)));
        let sc = Sidecar {
            words: words(&spec),
            ..Default::default()
        };
        assert_eq!(pick_reference(&sc).unwrap(), vec![(10.0, 30.0)]);
        // Two short stretches are joined when no single one is long enough.
        let sc = Sidecar {
            words: words(&[("a", 0.0, 3.0), ("b", 5.0, 8.0)]),
            ..Default::default()
        };
        assert_eq!(pick_reference(&sc).unwrap(), vec![(0.0, 3.0), (5.0, 8.0)]);
    }

    #[test]
    fn reference_text_is_the_words_in_the_sample() {
        let sc = Sidecar {
            words: words(&[("Hei", 0.0, 1.0), ("der", 1.0, 2.0), ("ute", 5.0, 6.0)]),
            ..Default::default()
        };
        assert_eq!(reference_text(&sc, &[(0.0, 2.0)]), "Hei der");
        assert_eq!(reference_text(&sc, &[(0.0, 1.0), (5.0, 6.0)]), "Hei ute");
    }

    #[test]
    fn jobs_carry_context() {
        let mut sc = Sidecar {
            words: words(&[
                ("da", 0.0, 0.3),
                ("skal", 0.3, 0.6),
                ("jeg", 0.6, 0.9),
                ("se", 0.9, 1.2),
                ("på", 1.2, 1.4),
                ("det", 1.4, 1.6),
            ]),
            ..Default::default()
        };
        let c = apply_text("da skal jeg virkelig gjerne se på det", &mut sc).unwrap();
        assert_eq!(c.spoken, 2);
        let j = jobs(&sc);
        assert_eq!(j.len(), 1);
        assert_eq!(j[0].words, ["virkelig", "gjerne"]);
        assert_eq!(j[0].before, ["da", "skal", "jeg"]);
        assert_eq!(j[0].after, ["se", "på"]);
        assert_eq!(j[0].text(), "da skal jeg virkelig gjerne se på");
        assert_eq!(j[0].ids, ["v0", "v1"]);
        // Applying the same text again changes nothing.
        let c = apply_text("da skal jeg virkelig gjerne se på det", &mut sc).unwrap();
        assert!(c.is_empty(), "{c:?}");
        // Deleting an inserted word removes it.
        let c = apply_text("da skal jeg virkelig se på det", &mut sc).unwrap();
        assert_eq!(c.unspoken, 1);
        assert_eq!(sc.inserts.len(), 1);
    }

    #[test]
    fn extract_finds_the_new_word() {
        // Generated "jeg virkelig se": tones at 0.0-0.3, 0.4-0.9, 1.0-1.3.
        let rate = 1000u32;
        let samples: Vec<f32> = (0..1500)
            .map(|f| {
                let t = f as f64 / 1000.0;
                let on = [(0.0, 0.3), (0.4, 0.9), (1.0, 1.3)]
                    .iter()
                    .any(|&(a, b)| t >= a && t < b);
                if on {
                    if f % 2 == 0 { 0.5 } else { -0.5 }
                } else {
                    0.0
                }
            })
            .collect();
        let generated = Buffer {
            sample_rate: rate,
            channels: 1,
            samples,
        };
        // Whisper's times on the generated audio, a little off.
        let gen_words = words(&[
            ("jeg", 0.0, 0.35),
            ("virkelig", 0.35, 0.95),
            ("se", 0.95, 1.3),
        ]);
        let job = Job {
            ids: vec!["v0".into()],
            before: vec!["jeg".into()],
            words: vec!["virkelig".into()],
            after: vec!["se".into()],
        };
        let loud = |out: Vec<f32>| out.iter().filter(|x| x.abs() > 0.1).count();
        assert_eq!(loud(extract(&job, &generated, &gen_words).unwrap()), 500);
        // Whisper gives "virkelig" only 0.05 s; the cut still takes the whole word.
        let short = words(&[
            ("jeg", 0.0, 0.35),
            ("virkelig", 0.35, 0.4),
            ("se", 0.4, 1.3),
        ]);
        assert_eq!(loud(extract(&job, &generated, &short).unwrap()), 500);
        // Heard differently from how it was typed ("24." said as "tjuefjerde"): the
        // words around it still mark where it is.
        let other = Job {
            words: vec!["24.".into()],
            ..job.clone()
        };
        assert_eq!(loud(extract(&other, &generated, &gen_words).unwrap()), 500);
        // With neither the word nor its context heard, it is an error, not a wrong splice.
        let lost = Job {
            before: vec!["hallo".into()],
            words: vec!["24.".into()],
            after: vec!["nei".into()],
            ..job
        };
        assert!(extract(&lost, &generated, &gen_words).is_err());
    }

    #[test]
    fn fit_matches_loudness() {
        let quiet: Vec<f32> = (0..1000)
            .map(|i| if i % 2 == 0 { 0.05 } else { -0.05 })
            .collect();
        let loud: Vec<f32> = (0..1000)
            .map(|i| if i % 2 == 0 { 0.2 } else { -0.2 })
            .collect();
        let out = fit(&quiet, 1000, &loud, 1000);
        assert!((out[0] - 0.2).abs() < 1e-6);
    }
}
