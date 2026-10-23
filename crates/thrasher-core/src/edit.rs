//! Non-destructive edits: removing words and shortening or removing pauses. The
//! recording is never changed; the sidecar marks cut words and how much of each pause
//! to keep, and playback and export render the result.
//!
//! The editor shows the transcript as text, with pauses as `[silence:7s]` tags.
//! Deleting a word cuts it, changing a tag to `[silence:2s]` shortens that pause, and
//! deleting a tag removes the pause. Typing a cut word back in its place restores it.

use crate::player::Buffer;
use crate::sidecar::{Silence, Word};

/// Crossfade length at each cut.
const FADE_SECS: f64 = 0.015;

/// Length a pause will have after editing.
pub fn kept(s: &Silence) -> f64 {
    let full = s.end - s.start;
    s.keep.map_or(full, |k| k.clamp(0.0, full))
}

/// Formats a pause tag, for example `[silence:7s]` or `[silence:1.5s]`.
pub fn tag(secs: f64) -> String {
    if secs >= 10.0 {
        format!("[silence:{}s]", secs.round())
    } else {
        format!("[silence:{secs:.1}s]").replace(".0s", "s")
    }
}

/// A word or a pause, in time order.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Item {
    Word(usize),
    Silence(usize),
}

fn items(words: &[Word], silences: &[Silence]) -> Vec<(f64, Item)> {
    let mut v: Vec<(f64, Item)> = words
        .iter()
        .enumerate()
        .map(|(i, w)| (w.start, Item::Word(i)))
        .chain(
            silences
                .iter()
                .enumerate()
                .map(|(i, s)| (s.start, Item::Silence(i))),
        )
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}

fn removed(s: &Silence) -> bool {
    kept(s) < 1e-6
}

/// The words and pauses still in the edit, in time order: start time in the
/// recording, text (a pause as a tag) and whether it is a pause.
pub fn visible(words: &[Word], silences: &[Silence]) -> Vec<(f64, String, bool)> {
    items(words, silences)
        .into_iter()
        .filter_map(|(t, it)| match it {
            Item::Word(i) if !words[i].cut => Some((t, words[i].text.clone(), false)),
            Item::Silence(i) if !removed(&silences[i]) => Some((t, tag(kept(&silences[i])), true)),
            _ => None,
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Word(String),
    Tag(f64),
}

fn tokenize(text: &str) -> Result<Vec<Token>, String> {
    let mut out = Vec::new();
    let mut rest = text;
    loop {
        let (before, tag) = match rest.find("[silence:") {
            Some(i) => (&rest[..i], Some(&rest[i + "[silence:".len()..])),
            None => (rest, None),
        };
        out.extend(before.split_whitespace().map(|w| Token::Word(w.into())));
        let Some(after) = tag else {
            return Ok(out);
        };
        let end = after.find(']').ok_or("a [silence: tag is not closed")?;
        let inner = after[..end].trim().trim_end_matches('s').trim();
        let v: f64 = inner
            .replace(',', ".")
            .parse()
            .map_err(|_| format!("'{}' is not a number of seconds", &after[..end]))?;
        if v < 0.0 {
            return Err("a pause cannot be negative".into());
        }
        out.push(Token::Tag(v));
        rest = &after[end + 1..];
    }
}

/// What applying edited text changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Changes {
    pub words_cut: usize,
    pub words_restored: usize,
    pub words_retyped: usize,
    pub pauses: usize,
    /// Typed words that match nothing in the recording, so they cannot be heard.
    pub ignored: usize,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        let mut add = |n: usize, what: &str| {
            if n > 0 {
                parts.push(format!("{n} {what}"));
            }
        };
        add(self.words_cut, "word(s) cut");
        add(self.words_restored, "word(s) restored");
        add(self.words_retyped, "word(s) retyped");
        add(self.pauses, "pause(s) changed");
        add(
            self.ignored,
            "new word(s) ignored, there is no audio for them",
        );
        if parts.is_empty() {
            "No changes".into()
        } else {
            parts.join(", ")
        }
    }
}

fn similar(a: &str, b: &str) -> bool {
    let norm = |s: &str| -> Vec<char> {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let (a, b) = (norm(a), norm(b));
    // Levenshtein distance, small strings only.
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(ca != cb))
                .min(prev[j + 1] + 1)
                .min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()] <= (a.len().max(b.len()) / 3).max(1)
}

/// Applies edited transcript text. The text is lined up against every word and pause
/// of the recording (also those cut earlier): words that are gone get cut, words
/// typed back are restored, a changed word keeps its audio with the new spelling, and
/// each tag sets how much of its pause to keep.
pub fn apply_text(
    text: &str,
    words: &mut [Word],
    silences: &mut [Silence],
) -> Result<Changes, String> {
    let tokens = tokenize(text)?;
    let order: Vec<Item> = items(words, silences).into_iter().map(|i| i.1).collect();
    let same = |it: Item, tok: &Token| match (it, tok) {
        (Item::Word(i), Token::Word(w)) => words[i].text == *w,
        (Item::Silence(_), Token::Tag(_)) => true,
        _ => false,
    };
    // Longest common subsequence, after trimming the common start and end so a
    // normal edit only compares a small middle part.
    let (n, m) = (order.len(), tokens.len());
    let mut head = 0;
    while head < n.min(m) && same(order[head], &tokens[head]) {
        head += 1;
    }
    let mut tail = 0;
    while tail < (n - head).min(m - head) && same(order[n - 1 - tail], &tokens[m - 1 - tail]) {
        tail += 1;
    }
    let (a, b) = (&order[head..n - tail], &tokens[head..m - tail]);
    let mut dp = vec![0u32; (a.len() + 1) * (b.len() + 1)];
    let w = b.len() + 1;
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            dp[i * w + j] = if same(a[i], &b[j]) {
                dp[(i + 1) * w + j + 1] + 1
            } else {
                dp[(i + 1) * w + j].max(dp[i * w + j + 1])
            };
        }
    }
    let mut pairs: Vec<(usize, usize)> = (0..head).map(|k| (k, k)).collect();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if same(a[i], &b[j]) {
            pairs.push((head + i, head + j));
            i += 1;
            j += 1;
        } else if dp[(i + 1) * w + j] >= dp[i * w + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs.extend((0..tail).map(|k| (n - tail + k, m - tail + k)));

    let mut changes = Changes::default();
    let set_cut = |words: &mut [Word], i: usize, cut: bool, c: &mut Changes| {
        if words[i].cut != cut {
            words[i].cut = cut;
            if cut {
                c.words_cut += 1;
            } else {
                c.words_restored += 1;
            }
        }
    };
    let set_keep = |s: &mut Silence, v: f64, c: &mut Changes| {
        let full = s.end - s.start;
        // Tags are rounded for display, so an unchanged tag must not count as an edit.
        let shown: f64 = tag(kept(s))
            .trim_start_matches("[silence:")
            .trim_end_matches("s]")
            .parse()
            .unwrap_or(full);
        if removed(s) || (v - shown).abs() > 1e-9 {
            let new = if v >= full { None } else { Some(v) };
            if new != s.keep {
                s.keep = new;
                c.pauses += 1;
            }
        }
    };
    // Walk the gaps between matched pairs, and the matched pairs themselves.
    let mut prev = (0usize, 0usize);
    for &(oi, ti) in pairs.iter().chain(std::iter::once(&(n, m))) {
        let gap_items = &order[prev.0..oi];
        let mut gap_tokens: Vec<&Token> = tokens[prev.1..ti].iter().collect();
        let gap_words: Vec<usize> = gap_items
            .iter()
            .filter_map(|it| match it {
                Item::Word(k) => Some(*k),
                _ => None,
            })
            .collect();
        let typed: Vec<&str> = gap_tokens
            .iter()
            .filter_map(|t| match t {
                Token::Word(w) => Some(w.as_str()),
                _ => None,
            })
            .collect();
        // Pair retyped words with the recorded words they replace.
        let mut used = vec![false; typed.len()];
        let equal = typed.len() == gap_words.len();
        for (k, &wi) in gap_words.iter().enumerate() {
            let hit = if equal {
                Some(k)
            } else {
                (0..typed.len()).find(|&t| !used[t] && similar(&words[wi].text, typed[t]))
            };
            match hit {
                Some(t) => {
                    used[t] = true;
                    if words[wi].text != typed[t] {
                        words[wi].text = typed[t].to_string();
                        changes.words_retyped += 1;
                    }
                    set_cut(words, wi, false, &mut changes);
                }
                None => set_cut(words, wi, true, &mut changes),
            }
        }
        changes.ignored += used.iter().filter(|u| !**u).count();
        // Pauses in the gap: a typed tag there brings one back, the rest are removed.
        gap_tokens.retain(|t| matches!(t, Token::Tag(_)));
        let mut tags = gap_tokens.into_iter();
        for it in gap_items {
            if let Item::Silence(k) = it {
                match tags.next() {
                    Some(Token::Tag(v)) => set_keep(&mut silences[*k], *v, &mut changes),
                    _ => {
                        if !removed(&silences[*k]) {
                            silences[*k].keep = Some(0.0);
                            changes.pauses += 1;
                        }
                    }
                }
            }
        }
        if oi < n {
            match (order[oi], &tokens[ti]) {
                (Item::Word(k), _) => set_cut(words, k, false, &mut changes),
                (Item::Silence(k), Token::Tag(v)) => set_keep(&mut silences[k], *v, &mut changes),
                _ => {}
            }
        }
        prev = (oi + 1, ti + 1);
    }
    Ok(changes)
}

/// Time ranges of the recording that the edit removes, in order and not overlapping.
/// A run of cut words and removed pauses goes as one piece, including the short gaps
/// between them; a shortened pause loses its middle.
pub fn cuts(words: &[Word], silences: &[Silence]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    let mut run: Option<(f64, f64)> = None;
    for (_, it) in items(words, silences) {
        let gone = match it {
            Item::Word(i) => words[i].cut.then(|| (words[i].start, words[i].end)),
            Item::Silence(i) => removed(&silences[i]).then(|| (silences[i].start, silences[i].end)),
        };
        match (gone, &mut run) {
            (Some((s, e)), Some(r)) => r.1 = r.1.max(e).max(s),
            (Some(span), None) => run = Some(span),
            (None, _) => out.extend(run.take()),
        }
        if let Item::Silence(i) = it {
            let s = &silences[i];
            if !removed(s) && kept(s) < s.end - s.start - 1e-6 {
                let half = kept(s) / 2.0;
                out.push((s.start + half, s.end - half));
            }
        }
    }
    out.extend(run);
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (s, e) in out {
        match merged.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ if e > s => merged.push((s, e)),
            _ => {}
        }
    }
    merged
}

/// Maps times between the recording and the edited result.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timeline {
    cuts: Vec<(f64, f64)>,
}

impl Timeline {
    pub fn new(cuts: Vec<(f64, f64)>) -> Self {
        Self { cuts }
    }

    /// A time in the recording, in the edited result. Inside a cut it maps to where
    /// the cut was.
    pub fn to_edited(&self, t: f64) -> f64 {
        let mut removed = 0.0;
        for &(a, b) in &self.cuts {
            if t <= a {
                break;
            }
            removed += t.min(b) - a;
        }
        t - removed
    }

    /// A time in the edited result, in the recording.
    pub fn to_original(&self, t: f64) -> f64 {
        let mut t = t;
        for &(a, b) in &self.cuts {
            if t < a {
                break;
            }
            t += b - a;
        }
        t
    }
}

/// Renders the buffer with the given time ranges removed. Each cut is joined with a
/// short equal-power crossfade that sits just before both ends of the cut, so the
/// result is exactly as much shorter as the cuts add up to.
pub fn render(buffer: &Buffer, cuts: &[(f64, f64)]) -> Buffer {
    let rate = buffer.sample_rate as f64;
    let ch = buffer.channels.max(1) as usize;
    let frames = buffer.frames() as usize;
    let fade = (FADE_SECS * rate) as usize;
    let mut out: Vec<f32> = Vec::with_capacity(buffer.samples.len());
    let mut pos = 0usize;
    for &(s, e) in cuts {
        let a = ((s * rate).round() as usize).min(frames);
        let b = ((e * rate).round() as usize).min(frames);
        if a < pos || b <= a {
            continue;
        }
        out.extend_from_slice(&buffer.samples[pos * ch..a * ch]);
        // Crossfade the audio before the cut into the audio leading up to its end.
        let n = fade.min(a - pos).min(b - a);
        let start = out.len() / ch - n;
        for i in 0..n {
            let t = (i as f64 + 0.5) / n as f64;
            let (g_out, g_in) = (
                (t * std::f64::consts::FRAC_PI_2).cos(),
                (t * std::f64::consts::FRAC_PI_2).sin(),
            );
            for c in 0..ch {
                let o = &mut out[(start + i) * ch + c];
                let x = buffer.samples[(b - n + i) * ch + c];
                *o = (*o as f64 * g_out + x as f64 * g_in) as f32;
            }
        }
        pos = b;
    }
    out.extend_from_slice(&buffer.samples[pos.min(frames) * ch..]);
    Buffer {
        sample_rate: buffer.sample_rate,
        channels: buffer.channels,
        samples: out,
    }
}

/// The words still in the edit, with times moved to the edited result.
pub fn edited_words(words: &[Word], timeline: &Timeline) -> Vec<Word> {
    words
        .iter()
        .filter(|w| !w.cut)
        .map(|w| Word {
            start: timeline.to_edited(w.start),
            end: timeline.to_edited(w.end),
            ..w.clone()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::{number_words, render_text};

    fn sil(start: f64, end: f64) -> Silence {
        Silence {
            start,
            end,
            keep: None,
        }
    }

    /// "Hei [silence:7s] der ja [silence:1.5s] nei"
    fn sample() -> (Vec<Word>, Vec<Silence>) {
        let words = number_words(vec![
            ("Hei".into(), 0.0, 1.0),
            ("der".into(), 8.0, 8.5),
            ("ja".into(), 8.7, 10.0),
            ("nei".into(), 11.5, 12.0),
        ]);
        (words, vec![sil(1.0, 8.0), sil(10.0, 11.5)])
    }

    #[test]
    fn tags_round_trip() {
        assert_eq!(tag(7.0), "[silence:7s]");
        assert_eq!(tag(1.5), "[silence:1.5s]");
        assert_eq!(tag(12.4), "[silence:12s]");
    }

    #[test]
    fn shorten_and_restore_pause() {
        let (mut w, mut s) = sample();
        let c = apply_text("Hei [silence:2s] der ja [silence:1.5s] nei", &mut w, &mut s).unwrap();
        assert_eq!(c.pauses, 1);
        assert_eq!(s[0].keep, Some(2.0));
        assert_eq!(s[1].keep, None);
        apply_text("Hei [silence:7s] der ja [silence:1,5s] nei", &mut w, &mut s).unwrap();
        assert_eq!(s[0].keep, None);
        assert!(apply_text("Hei [silence:abc] der", &mut w, &mut s).is_err());
    }

    #[test]
    fn cut_words_and_pauses() {
        let (mut w, mut s) = sample();
        // Delete "ja", and the second pause.
        let c = apply_text("Hei [silence:7s] der nei", &mut w, &mut s).unwrap();
        assert_eq!((c.words_cut, c.pauses), (1, 1));
        assert!(w[2].cut);
        assert_eq!(s[1].keep, Some(0.0));
        assert_eq!(render_text(&w, &s), "Hei [silence:7s] der nei");
        // "ja" and the pause after it go as one piece.
        assert_eq!(cuts(&w, &s), vec![(8.7, 11.5)]);
        // Typing "ja" back restores it.
        let c = apply_text("Hei [silence:7s] der ja nei", &mut w, &mut s).unwrap();
        assert_eq!(c.words_restored, 1);
        assert!(!w[2].cut);
    }

    #[test]
    fn retyped_words_keep_their_audio() {
        let (mut w, mut s) = sample();
        let c = apply_text(
            "Hei [silence:7s] dér ja [silence:1.5s] nei hallo",
            &mut w,
            &mut s,
        )
        .unwrap();
        assert_eq!((c.words_retyped, c.ignored, c.words_cut), (1, 1, 0));
        assert_eq!(w[1].text, "dér");
        // Deleting one word and fixing its neighbour pairs the fix with the right word.
        let (mut w, mut s) = sample();
        apply_text("Hei [silence:7s] Ja [silence:1.5s] nei", &mut w, &mut s).unwrap();
        assert!(w[1].cut);
        assert_eq!((w[2].text.as_str(), w[2].cut), ("Ja", false));
    }

    #[test]
    fn timeline_maps_both_ways() {
        let t = Timeline::new(vec![(2.0, 5.0), (8.0, 9.0)]);
        assert_eq!(t.to_edited(1.0), 1.0);
        assert_eq!(t.to_edited(3.0), 2.0);
        assert_eq!(t.to_edited(6.0), 3.0);
        assert_eq!(t.to_edited(10.0), 6.0);
        assert_eq!(t.to_original(1.0), 1.0);
        assert_eq!(t.to_original(3.0), 6.0);
        assert_eq!(t.to_original(6.0), 10.0);
    }

    #[test]
    fn render_removes_exactly_the_cuts() {
        let rate = 1000;
        // 1 s of 0.5, 4 s of silence, 1 s of -0.5.
        let mut samples = vec![0.5f32; rate];
        samples.extend(vec![0.0; 4 * rate]);
        samples.extend(vec![-0.5; rate]);
        let buf = Buffer {
            sample_rate: rate as u32,
            channels: 1,
            samples,
        };
        let mut s = sil(1.0, 5.0);
        s.keep = Some(1.0);
        let c = cuts(&[], &[s]);
        assert_eq!(c, vec![(1.5, 4.5)]);
        let out = render(&buf, &c);
        assert_eq!(out.samples.len(), 3 * rate);
        assert_eq!(out.samples[0], 0.5);
        assert_eq!(*out.samples.last().unwrap(), -0.5);
        // Unedited pauses leave the audio untouched.
        assert_eq!(
            render(&buf, &cuts(&[], &[sil(1.0, 5.0)])).samples,
            buf.samples
        );
    }
}
