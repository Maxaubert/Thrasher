//! Transcription support that does not depend on a model: resampling to 16 kHz,
//! cutting long audio into chunks, turning tokens into timed words, and silence tags.
//! The Whisper engine itself lives in `whisper.rs` behind the `whisper` feature.

use crate::sidecar::{Silence, Word};
use std::sync::atomic::AtomicBool;

/// Whisper's input rate.
pub const RATE: u32 = 16_000;
/// Longest chunk handed to the model at once. Whisper's window is 30 s.
const MAX_CHUNK_SECS: f64 = 28.0;
/// Where to look for a quiet split point, counted back from the chunk end.
const SPLIT_SEARCH_SECS: f64 = 6.0;
/// Chunks are never shorter than this (very short windows have crashed whisper.cpp's
/// DTW, see the research notes).
const MIN_CHUNK_SECS: f64 = 3.0;
/// Pauses at least this long become silence tags.
pub const DEFAULT_MIN_SILENCE: f64 = 1.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transcript {
    pub language: Option<String>,
    pub words: Vec<Word>,
}

#[derive(Clone, Debug)]
pub struct Options {
    /// ISO code such as "no" or "en"; `None` lets the model detect it.
    pub language: Option<String>,
    pub beam_size: i32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            language: Some("no".into()),
            beam_size: 5,
        }
    }
}

/// A speech-to-text engine.
pub trait Transcriber {
    /// `audio` is 16 kHz mono. `progress` receives 0..=1. Setting `cancel` stops early.
    fn transcribe(
        &mut self,
        audio: &[f32],
        options: &Options,
        progress: &mut dyn FnMut(f32),
        cancel: &AtomicBool,
    ) -> Result<Transcript, String>;
}

pub fn to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    if ch == 1 {
        return samples.to_vec();
    }
    samples
        .chunks_exact(ch)
        .map(|f| f.iter().sum::<f32>() / ch as f32)
        .collect()
}

/// Windowed-sinc resampler. Slow next to a polyphase design but exact enough for
/// speech recognition, and it handles any pair of rates.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let out_len = (input.len() as f64 * ratio).floor() as usize;
    // Cutoff relative to the input Nyquist frequency, a little below the lower rate.
    let cutoff = ratio.min(1.0) * 0.94;
    let half = (12.0 / cutoff).ceil();
    let n = input.len() as i64;
    let mut out = Vec::with_capacity(out_len);
    for j in 0..out_len {
        let t = j as f64 / ratio;
        let lo = ((t - half).ceil() as i64).max(0);
        let hi = ((t + half).floor() as i64).min(n - 1);
        let mut acc = 0.0f64;
        for k in lo..=hi {
            let x = t - k as f64;
            let arg = x * cutoff * std::f64::consts::PI;
            let sinc = if arg.abs() < 1e-9 {
                1.0
            } else {
                arg.sin() / arg
            };
            let window = 0.5 * (1.0 + (std::f64::consts::PI * x / half).cos());
            acc += input[k as usize] as f64 * sinc * cutoff * window;
        }
        out.push(acc as f32);
    }
    out
}

/// Splits 16 kHz audio into chunks of at most 28 s, cutting at the quietest 50 ms
/// stretch near the end of each window. Returns sample ranges.
pub fn chunks(audio: &[f32]) -> Vec<std::ops::Range<usize>> {
    let rate = RATE as usize;
    let max = (MAX_CHUNK_SECS * rate as f64) as usize;
    let min = (MIN_CHUNK_SECS * rate as f64) as usize;
    let search = (SPLIT_SEARCH_SECS * rate as f64) as usize;
    let win = rate / 20;
    let mut out = Vec::new();
    let mut start = 0;
    while start < audio.len() {
        let left = audio.len() - start;
        if left <= max {
            out.push(start..audio.len());
            break;
        }
        let from = start + max - search;
        let mut best = start + max;
        let mut best_energy = f32::MAX;
        let mut pos = from;
        while pos + win <= start + max {
            let e: f32 = audio[pos..pos + win].iter().map(|s| s * s).sum();
            if e < best_energy {
                best_energy = e;
                best = pos + win / 2;
            }
            pos += win / 2;
        }
        out.push(start..best);
        start = best;
    }
    // Fold a too-short tail into the previous chunk when it fits, so the model never
    // sees a tiny window.
    if out.len() >= 2 {
        let last = out[out.len() - 1].clone();
        if last.len() < min && out[out.len() - 2].len() + last.len() <= 30 * rate {
            out.pop();
            let prev = out.last_mut().unwrap();
            prev.end = last.end;
        }
    }
    out
}

/// One decoded token: its raw bytes and its start time in seconds within the chunk.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub bytes: Vec<u8>,
    pub time: f64,
}

/// Groups tokens into words. A token whose text starts with a space begins a new word.
/// Bytes are joined before decoding, so letters such as æ, ø and å that span two
/// tokens survive. Each word ends where the next begins, capped to a length that
/// fits its letters, so a pause after a word is not counted as part of it.
pub fn words_from_tokens(
    tokens: &[Token],
    offset: f64,
    segment_end: f64,
) -> Vec<(String, f64, f64)> {
    let mut groups: Vec<(Vec<u8>, f64)> = Vec::new();
    for t in tokens {
        let starts_word = t.bytes.first() == Some(&b' ') || groups.is_empty();
        if starts_word {
            groups.push((t.bytes.clone(), t.time));
        } else if let Some(g) = groups.last_mut() {
            g.0.extend_from_slice(&t.bytes);
        }
    }
    let mut out = Vec::new();
    for (i, (bytes, start)) in groups.iter().enumerate() {
        let text = String::from_utf8_lossy(bytes).trim().to_string();
        if text.is_empty() {
            continue;
        }
        let next = groups.get(i + 1).map_or(segment_end, |g| g.1);
        let longest = 0.3 + 0.09 * text.chars().count() as f64;
        let end = next.min(start + longest).max(*start + 0.02);
        out.push((text, offset + start, offset + end));
    }
    out
}

/// Numbers words `w0`, `w1`, ... in the format the video tooling reads.
pub fn number_words(words: Vec<(String, f64, f64)>) -> Vec<Word> {
    words
        .into_iter()
        .enumerate()
        .map(|(i, (text, start, end))| Word {
            id: format!("w{i}"),
            text,
            start: round_ms(start),
            end: round_ms(end),
            cut: false,
        })
        .collect()
}

fn round_ms(t: f64) -> f64 {
    (t * 1000.0).round() / 1000.0
}

/// Pauses of at least `min_gap` seconds between words, and before the first and
/// after the last word.
pub fn silences(words: &[Word], duration: f64, min_gap: f64) -> Vec<Silence> {
    let mut out = Vec::new();
    let mut prev_end = 0.0;
    for w in words {
        if w.start - prev_end >= min_gap {
            out.push(Silence {
                start: round_ms(prev_end),
                end: round_ms(w.start),
                keep: None,
            });
        }
        prev_end = prev_end.max(w.end);
    }
    if duration - prev_end >= min_gap {
        out.push(Silence {
            start: round_ms(prev_end),
            end: round_ms(duration),
            keep: None,
        });
    }
    out
}

/// Plain text with silence tags, such as `Hei [silence:7s] der`. Words and pauses
/// removed in the editor are left out.
pub fn render_text(words: &[Word], silences: &[Silence]) -> String {
    crate::edit::text(&crate::sidecar::Sidecar {
        words: words.to_vec(),
        silences: silences.to_vec(),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_keeps_tone_and_length() {
        let from = 48_000;
        let tone: Vec<f32> = (0..from)
            .map(|i| (i as f32 / from as f32 * 440.0 * std::f32::consts::TAU).sin())
            .collect();
        let out = resample(&tone, from, RATE);
        assert_eq!(out.len(), 16_000);
        // Compare the middle against the ideal tone at 16 kHz.
        let err: f32 = (1000..15_000)
            .map(|i| {
                let ideal = (i as f32 / 16_000.0 * 440.0 * std::f32::consts::TAU).sin();
                (out[i] - ideal).abs()
            })
            .fold(0.0, f32::max);
        assert!(err < 0.02, "max error {err}");
    }

    #[test]
    fn resample_removes_content_above_nyquist() {
        let from = 48_000;
        let high: Vec<f32> = (0..from)
            .map(|i| (i as f32 / from as f32 * 12_000.0 * std::f32::consts::TAU).sin())
            .collect();
        let out = resample(&high, from, RATE);
        let peak = out[1000..15_000].iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(peak < 0.01, "aliasing peak {peak}");
    }

    #[test]
    fn chunks_split_in_quiet_places() {
        let rate = RATE as usize;
        // 70 s of noise-like signal with a quiet gap at 25 s and at 50 s.
        let mut audio: Vec<f32> = (0..70 * rate)
            .map(|i| ((i * 7919) % 200) as f32 / 400.0)
            .collect();
        for gap in [25, 50] {
            audio[gap * rate..gap * rate + rate / 5].fill(0.0);
        }
        let c = chunks(&audio);
        assert_eq!(c.len(), 3);
        assert!(c.iter().all(|r| r.len() <= 28 * rate));
        let cut = c[0].end as f64 / rate as f64;
        assert!((cut - 25.1).abs() < 0.15, "cut at {cut}");
        assert_eq!(c.last().unwrap().end, audio.len());
    }

    #[test]
    fn chunks_fold_short_tail() {
        let rate = RATE as usize;
        // The quiet spot at 27.5 s would leave a 1.5 s tail, which is folded back in.
        let mut audio = vec![0.1f32; 29 * rate];
        audio[27 * rate + rate / 2..28 * rate].fill(0.0);
        let c = chunks(&audio);
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(c[0], 0..audio.len());
    }

    fn tok(s: &[u8], t: f64) -> Token {
        Token {
            bytes: s.to_vec(),
            time: t,
        }
    }

    #[test]
    fn tokens_to_words_keeps_norwegian_letters() {
        // "blåbær" split in the middle of the two-byte 'å' and 'æ'.
        let blaa = " blåbær".as_bytes();
        let tokens = [
            tok(b" Hei", 0.5),
            tok(&blaa[..3], 1.0),
            tok(&blaa[3..6], 1.1),
            tok(&blaa[6..], 1.2),
            tok(b"!", 1.5),
        ];
        let w = words_from_tokens(&tokens, 10.0, 3.0);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].0, "Hei");
        assert_eq!(w[1].0, "blåbær!");
        assert!((w[0].1 - 10.5).abs() < 1e-9);
        assert!((w[0].2 - 11.0).abs() < 1e-9);
        assert!((w[1].1 - 11.0).abs() < 1e-9);
        // Capped: 7 letters at most 0.3 + 0.63 s, not stretched to the segment end.
        assert!((w[1].2 - (11.0 + 0.3 + 0.09 * 7.0)).abs() < 1e-9);
    }

    #[test]
    fn silence_tags() {
        let words = number_words(vec![
            ("Hei".into(), 0.2, 0.5),
            ("der".into(), 7.5, 7.8),
            ("ja".into(), 8.0, 8.2),
        ]);
        let s = silences(&words, 20.0, 1.0);
        assert_eq!(
            s,
            vec![
                Silence {
                    start: 0.5,
                    end: 7.5,
                    keep: None
                },
                Silence {
                    start: 8.2,
                    end: 20.0,
                    keep: None
                }
            ]
        );
        assert_eq!(
            render_text(&words, &s),
            "Hei [silence:7s] der ja [silence:12s]"
        );
        assert_eq!(words[1].id, "w1");
    }
}
