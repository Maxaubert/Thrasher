//! Non-destructive edits: removing words, and shortening, lengthening, removing or
//! adding pauses. The recording is never changed; the sidecar marks cut words and how
//! long each pause should be, and playback and export render the result.
//!
//! The editor shows the transcript as text, with pauses as `[silence:7s]` tags.
//! Deleting a word cuts it, changing a tag sets that pause's length, deleting a tag
//! removes the pause, and typing a new tag between two words adds a pause there.
//! Typing a cut word back in its place restores it. A new word that was never said
//! becomes an *insert*, spoken in the speaker's voice by a voice model (see `voice`).
//!
//! Whisper's word times are only roughly right and words sit back to back, so cut
//! edges are moved to the quietest point near each word start, found in the audio.

use crate::player::Buffer;
use crate::sidecar::{Insert, Sidecar, Silence, Word};
use std::collections::HashMap;

/// Crossfade length at each cut.
const FADE_SECS: f64 = 0.015;
/// How far a cut edge may move from Whisper's word time to find a quiet point.
const SNAP_SECS: f64 = 0.2;

/// Length a pause will have after editing.
pub fn kept(s: &Silence) -> f64 {
    s.keep.map_or(s.end - s.start, |k| k.max(0.0))
}

/// Formats a pause tag, for example `[silence:7s]` or `[silence:1.5s]`.
pub fn tag(secs: f64) -> String {
    if secs >= 10.0 {
        format!("[silence:{}s]", secs.round())
    } else {
        format!("[silence:{secs:.1}s]").replace(".0s", "s")
    }
}

/// A word, a pause or an inserted word, in time order.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Item {
    Word(usize),
    Silence(usize),
    Insert(usize),
}

fn items(sc: &Sidecar) -> Vec<(f64, Item)> {
    let mut v: Vec<(f64, Item)> = sc
        .words
        .iter()
        .enumerate()
        .map(|(i, w)| (w.start, Item::Word(i)))
        .chain(
            sc.silences
                .iter()
                .enumerate()
                .map(|(i, s)| (s.start, Item::Silence(i))),
        )
        .chain(
            sc.inserts
                .iter()
                .enumerate()
                .map(|(i, s)| (s.at, Item::Insert(i))),
        )
        .collect();
    // Something added at a word's start time comes before that word.
    v.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then_with(|| matches!(a.1, Item::Word(_)).cmp(&matches!(b.1, Item::Word(_))))
    });
    v
}

fn span(sc: &Sidecar, it: Item) -> (f64, f64) {
    match it {
        Item::Word(i) => (sc.words[i].start, sc.words[i].end),
        Item::Silence(i) => (sc.silences[i].start, sc.silences[i].end),
        Item::Insert(i) => (sc.inserts[i].at, sc.inserts[i].at),
    }
}

fn removed(s: &Silence) -> bool {
    kept(s) < 1e-6
}

/// What a shown transcript item is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Word,
    Pause,
    /// An inserted word: still being generated, ready, or failed.
    Spoken {
        ready: bool,
        failed: bool,
    },
}

/// One item of the transcript as shown: start time in the recording, text (a pause as
/// a tag) and kind.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub start: f64,
    pub text: String,
    pub kind: Kind,
}

/// The words, pauses and inserted words still in the edit, in time order.
pub fn visible(sc: &Sidecar) -> Vec<Shown> {
    items(sc)
        .into_iter()
        .filter_map(|(start, it)| {
            let (text, kind) = match it {
                Item::Word(i) if !sc.words[i].cut => (sc.words[i].text.clone(), Kind::Word),
                Item::Silence(i) if !removed(&sc.silences[i]) => {
                    (tag(kept(&sc.silences[i])), Kind::Pause)
                }
                Item::Insert(i) => {
                    let ins = &sc.inserts[i];
                    let kind = Kind::Spoken {
                        ready: has_audio(sc, ins),
                        failed: ins.error.is_some(),
                    };
                    (ins.text.clone(), kind)
                }
                _ => return None,
            };
            Some(Shown { start, text, kind })
        })
        .collect()
}

/// True when an insert's audio exists: its own file, or the phrase it is part of.
fn has_audio(sc: &Sidecar, ins: &Insert) -> bool {
    ins.file.is_some()
        || ins
            .with
            .as_ref()
            .is_some_and(|w| sc.inserts.iter().any(|j| &j.id == w && j.file.is_some()))
}

/// Runs of inserts with no kept word or pause between them, in time order.
fn insert_runs(sc: &Sidecar) -> Vec<Vec<usize>> {
    let mut runs: Vec<Vec<usize>> = Vec::new();
    let mut open = false;
    for (_, it) in items(sc) {
        match it {
            Item::Insert(i) if open => runs.last_mut().unwrap().push(i),
            Item::Insert(i) => {
                runs.push(vec![i]);
                open = true;
            }
            Item::Word(i) if sc.words[i].cut => {}
            Item::Silence(i) if removed(&sc.silences[i]) => {}
            _ => open = false,
        }
    }
    runs
}

/// Typed words next to each other are spoken as one phrase, stored in the first word's
/// file (the others point to it with `with`), because word by word they sound jumbled.
/// After an edit, a run that no longer matches how it was spoken (a word added, removed
/// or moved) is reset, so the whole run is spoken again as one phrase.
fn regroup(sc: &mut Sidecar, broken: &[String]) {
    for run in insert_runs(sc) {
        let lead = &sc.inserts[run[0]];
        let lost_word = broken.contains(&lead.id);
        let followers = sc
            .inserts
            .iter()
            .filter(|x| x.with.as_deref() == Some(lead.id.as_str()))
            .count();
        let intact = !lost_word
            && lead.with.is_none()
            && lead.file.is_some()
            && followers == run.len() - 1
            && run[1..]
                .iter()
                .all(|&j| sc.inserts[j].with.as_deref() == Some(lead.id.as_str()));
        if !intact {
            for &j in &run {
                let x = &mut sc.inserts[j];
                x.file = None;
                x.with = None;
                x.error = None;
            }
        }
    }
}

/// The transcript as editable text, such as `Hei [silence:7s] der`.
pub fn text(sc: &Sidecar) -> String {
    visible(sc)
        .into_iter()
        .map(|s| s.text)
        .collect::<Vec<_>>()
        .join(" ")
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
    /// New words that will be spoken in the speaker's voice.
    pub spoken: usize,
    /// Inserted words that were deleted again.
    pub unspoken: usize,
    /// Typed text with no letters or digits (lone punctuation), which has no sound.
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
        add(self.spoken, "new word(s) to speak in your voice");
        add(self.unspoken, "spoken word(s) removed");
        add(
            self.ignored,
            "punctuation mark(s) ignored, they have no sound",
        );
        if parts.is_empty() {
            "No changes".into()
        } else {
            parts.join(", ")
        }
    }
}

pub(crate) fn similar(a: &str, b: &str) -> bool {
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
/// of the recording (also those cut earlier) and every inserted word: words that are
/// gone get cut, words typed back are restored, a changed word keeps its audio with
/// the new spelling, each tag sets how much of its pause to keep, and words that were
/// never said become inserts.
pub fn apply_text(text: &str, sc: &mut Sidecar) -> Result<Changes, String> {
    let tokens = tokenize(text)?;
    let order: Vec<Item> = items(sc).into_iter().map(|i| i.1).collect();
    let mut next_id = sc
        .inserts
        .iter()
        .filter_map(|i| i.id.strip_prefix('v')?.parse::<u32>().ok())
        .max()
        .map_or(0, |n| n + 1);
    let Sidecar {
        words,
        silences,
        inserts,
        ..
    } = sc;
    let same = |it: Item, tok: &Token| match (it, tok) {
        (Item::Word(i), Token::Word(w)) => words[i].text == *w,
        (Item::Insert(i), Token::Word(w)) => inserts[i].text == *w,
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
            let new = if (v - full).abs() < 1e-9 {
                None
            } else {
                Some(v)
            };
            if new != s.keep {
                s.keep = new;
                c.pauses += 1;
            }
        }
    };
    let mut added: Vec<Silence> = Vec::new();
    let mut new_inserts: Vec<Insert> = Vec::new();
    let mut dropped: Vec<usize> = Vec::new();
    // Walk the gaps between matched pairs, and the matched pairs themselves.
    let mut prev = (0usize, 0usize);
    for &(oi, ti) in pairs.iter().chain(std::iter::once(&(n, m))) {
        let gap_items = &order[prev.0..oi];
        let gap_tokens: Vec<&Token> = tokens[prev.1..ti].iter().collect();
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
        // Pair retyped words with the recorded words they replace. Only a similar
        // spelling counts, so deleting a word never hands its audio to another word.
        let mut used = vec![false; typed.len()];
        // The recorded word each typed word kept, if any.
        let mut kept_word: Vec<Option<usize>> = vec![None; typed.len()];
        for &wi in &gap_words {
            let hit = (0..typed.len()).find(|&t| !used[t] && similar(&words[wi].text, typed[t]));
            match hit {
                Some(t) => {
                    used[t] = true;
                    kept_word[t] = Some(wi);
                    if words[wi].text != typed[t] {
                        words[wi].text = typed[t].to_string();
                        changes.words_retyped += 1;
                    }
                    set_cut(words, wi, false, &mut changes);
                }
                None => set_cut(words, wi, true, &mut changes),
            }
        }
        // Inserted words in the gap were deleted or changed.
        for it in gap_items {
            if let Item::Insert(k) = it {
                dropped.push(*k);
                changes.unspoken += 1;
            }
        }
        // Pauses in the gap: a typed tag there brings one back, the rest are removed.
        let gap_silences = gap_items
            .iter()
            .filter(|it| matches!(it, Item::Silence(_)))
            .count();
        let mut tags = gap_tokens.iter().filter(|t| matches!(t, Token::Tag(_)));
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
        // Left over tags become new pauses and left over words become inserts, placed
        // between the items around the gap, in the order they were typed.
        let span_of = |it: Item| match it {
            Item::Word(i) => (words[i].start, words[i].end),
            Item::Silence(i) => (silences[i].start, silences[i].end),
            Item::Insert(i) => (inserts[i].at, inserts[i].at),
        };
        let gap_left = prev.0.checked_sub(1).map(|p| span_of(order[p]).1);
        let gap_right = (oi < n).then(|| span_of(order[oi]).0);
        // Retyped words in the gap keep their recording, so they split it: each new
        // token goes between the nearest kept words (or the gap's ends) around it.
        let mut word_index = 0;
        let anchors: Vec<Option<(f64, f64)>> = gap_tokens
            .iter()
            .map(|t| match t {
                Token::Word(_) => {
                    word_index += 1;
                    kept_word[word_index - 1].map(|wi| (words[wi].start, words[wi].end))
                }
                Token::Tag(_) => None,
            })
            .collect();
        let (mut tag_no, mut word_no) = (0, 0);
        for (k, t) in gap_tokens.iter().enumerate() {
            if anchors[k].is_some() {
                word_no += 1;
                continue;
            }
            let before = (0..k).rev().find(|&j| anchors[j].is_some());
            let after = (k + 1..anchors.len()).find(|&j| anchors[j].is_some());
            let left = before.map_or(gap_left, |j| anchors[j].map(|a| a.1));
            let right = after.map_or(gap_right, |j| anchors[j].map(|a| a.0));
            let first = before.map_or(0, |j| j + 1);
            let (k, count) = (k - first, after.unwrap_or(anchors.len()) - first);
            // Each token gets its own time strictly between the items around it, so
            // sorting by time keeps the typed order, also next to earlier inserts that
            // sit only a microsecond apart.
            let at = match (left, right) {
                (Some(l), Some(r)) if r > l => l + (r - l) * (k + 1) as f64 / (count + 1) as f64,
                // No room (the next word starts where the last one ends): step back
                // from the next item.
                (_, Some(r)) => r - (count - k) as f64 * 1e-6,
                (Some(l), None) => l + (k + 1) as f64 * 1e-6,
                (None, None) => (k + 1) as f64 * 1e-6,
            };
            match t {
                Token::Tag(v) => {
                    tag_no += 1;
                    if tag_no > gap_silences && *v > 0.0 {
                        added.push(Silence {
                            start: at,
                            end: at,
                            keep: Some(*v),
                        });
                        changes.pauses += 1;
                    }
                }
                Token::Word(w) => {
                    word_no += 1;
                    if used[word_no - 1] {
                        continue;
                    }
                    if !w.chars().any(char::is_alphanumeric) {
                        changes.ignored += 1;
                        continue;
                    }
                    new_inserts.push(Insert {
                        id: format!("v{next_id}"),
                        text: w.clone(),
                        at,
                        file: None,
                        error: None,
                        with: None,
                    });
                    next_id += 1;
                    changes.spoken += 1;
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
    // Added pauses that were deleted again are dropped.
    silences.retain(|s| s.end - s.start > 1e-9 || !removed(s));
    silences.extend(added);
    silences.sort_by(|a, b| a.start.total_cmp(&b.start));
    // Phrases that lose a word are spoken again.
    let broken: Vec<String> = dropped
        .iter()
        .filter_map(|&k| inserts.get(k))
        .map(|i| i.with.clone().unwrap_or_else(|| i.id.clone()))
        .collect();
    let mut k = 0;
    inserts.retain(|_| {
        k += 1;
        !dropped.contains(&(k - 1))
    });
    inserts.extend(new_inserts);
    inserts.sort_by(|a, b| a.at.total_cmp(&b.at));
    regroup(sc, &broken);
    Ok(changes)
}

/// One piece of the edited result: a stretch of the recording, an added pause, or an
/// inserted word.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Piece {
    Audio(f64, f64),
    /// Added silence of `len` seconds, placed at `at` in the recording.
    Gap {
        at: f64,
        len: f64,
    },
    /// Generated speech number `idx` in `Edl::voices`, `len` seconds long.
    Voice {
        at: f64,
        len: f64,
        idx: usize,
    },
}

/// The edited result as a list of pieces, plus time mapping both ways. Built from the
/// sidecar's edits and the audio (to place cut edges at quiet points).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edl {
    pieces: Vec<Piece>,
    /// Inserted words: mono samples at the clip's sample rate.
    voices: Vec<Vec<f32>>,
    /// The insert id of each entry in `voices`.
    voice_ids: Vec<String>,
}

/// Mono loudness (RMS) of a 20 ms window centred on `t`.
pub(crate) fn rms_at(buffer: &Buffer, t: f64) -> f64 {
    let rate = buffer.sample_rate as f64;
    let ch = buffer.channels.max(1) as usize;
    let frames = buffer.frames() as usize;
    let half = (0.01 * rate) as usize;
    let c = (t * rate) as usize;
    let (a, b) = (c.saturating_sub(half), (c + half).min(frames));
    if b <= a {
        return 0.0;
    }
    let sum: f64 = (a..b)
        .map(|f| {
            let m: f32 = buffer.samples[f * ch..f * ch + ch].iter().sum::<f32>() / ch as f32;
            (m as f64) * (m as f64)
        })
        .sum();
    (sum / (b - a) as f64).sqrt()
}

fn db(rms: f64) -> f64 {
    20.0 * (rms + 1e-9).log10()
}

/// The latest quiet point in `lo..hi`. Quiet is judged against the speech around it:
/// closer to the noise floor than to the loudest speech nearby (40% of the way up,
/// in dB). A short breath between two words counts, so a cut stops before the
/// word in front of it even when Whisper's times are well off. Cutting right before
/// a word starts keeps the pause in front of the cut, so the sentence keeps its
/// rhythm.
pub(crate) fn quiet_before(buffer: &Buffer, lo: f64, hi: f64, fallback: f64) -> f64 {
    let hi = hi.min(buffer.duration_secs());
    if hi - lo < 0.01 {
        return fallback.clamp(lo.min(hi), hi.max(lo));
    }
    let level = |t: f64| db(rms_at(buffer, t));
    let range = |a: f64, b: f64| -> Vec<(f64, f64)> {
        let steps = ((b - a) / 0.005).max(0.0) as usize;
        (0..=steps)
            .map(|i| {
                let t = a + i as f64 * 0.005;
                (t, level(t))
            })
            .collect()
    };
    let levels = range(lo, hi);
    let around = range((lo - 0.5).max(0.0), hi + 0.5);
    // Digital silence would make every real gap look loud, so the floor stops at -100 dB.
    let floor = around
        .iter()
        .map(|l| l.1)
        .fold(f64::INFINITY, f64::min)
        .max(-100.0);
    let peak = around.iter().map(|l| l.1).fold(f64::NEG_INFINITY, f64::max);
    let limit = floor + (peak - floor) * 0.4;
    if let Some(l) = levels.iter().rev().find(|l| l.1 <= limit) {
        return l.0;
    }
    // No real gap (fast speech): the quietest point.
    levels
        .iter()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(fallback, |l| l.0)
}

/// A stretch of the quietest audio near `at`, used as room tone for added pauses so
/// they sound like the room instead of dead silence. Tries 200 ms, then 50 ms for
/// fast speech with only short gaps; returns nothing (plain silence) when every
/// stretch has speech in it.
fn room_tone(buffer: &Buffer, at: f64) -> Vec<f32> {
    let rate = buffer.sample_rate as f64;
    let ch = buffer.channels.max(1) as usize;
    let frames = buffer.frames() as usize;
    let lo = ((at - 5.0).max(0.0) * rate) as usize;
    let hi = (((at + 5.0) * rate) as usize).min(frames);
    let mean_sq = |a: usize, len: usize| -> f64 {
        buffer.samples[a * ch..(a + len) * ch]
            .iter()
            .map(|x| (*x as f64) * (*x as f64))
            .sum::<f64>()
            / (len * ch) as f64
    };
    let quietest = |len: usize| -> Option<(usize, f64)> {
        let step = ((0.005 * rate) as usize).max(1);
        (lo..hi.checked_sub(len)?.max(lo) + 1)
            .step_by(step)
            .map(|a| (a, mean_sq(a, len)))
            .min_by(|x, y| x.1.total_cmp(&y.1))
    };
    // The noise floor: the quietest 10 ms.
    let Some((_, floor)) = quietest(((0.01 * rate) as usize).max(1)) else {
        return Vec::new();
    };
    for secs in [0.2, 0.05] {
        let len = (secs * rate) as usize;
        // Within 6 dB of the floor counts as room tone.
        if let Some((a, e)) = quietest(len)
            && e <= floor * 4.0 + 1e-10
        {
            return buffer.samples[a * ch..(a + len) * ch].to_vec();
        }
    }
    Vec::new()
}

/// Equal-power gains for step `i` of an `n`-step crossfade: (fading out, fading in).
fn gains(i: usize, n: usize) -> (f64, f64) {
    let t = (i as f64 + 0.5) / n as f64 * std::f64::consts::FRAC_PI_2;
    (t.cos(), t.sin())
}

/// Crossfades the last `n` frames of `out` into `with` (n frames).
fn crossfade(out: &mut [f32], with: &[f32], ch: usize) {
    let n = with.len() / ch;
    let start = out.len() / ch - n;
    for i in 0..n {
        let (g_out, g_in) = gains(i, n);
        for c in 0..ch {
            let o = &mut out[(start + i) * ch + c];
            *o = (*o as f64 * g_out + with[i * ch + c] as f64 * g_in) as f32;
        }
    }
}

/// Something added to the recording at a point in time.
enum Add {
    Gap(f64),
    Voice(usize),
}

/// Plans the edit for a clip: which parts of the recording to play, where to add
/// pauses, and where inserted words go. `voices` holds the generated audio of inserts
/// that are ready (by insert id, mono, at the clip's sample rate); inserts without
/// audio yet are skipped.
pub fn plan(sc: &Sidecar, voices: &HashMap<String, Vec<f32>>, buffer: &Buffer) -> Edl {
    let dur = buffer.duration_secs();
    let rate = buffer.sample_rate as f64;
    let all = items(sc);
    // Cuts are planned on the recording alone; inserts are placed afterwards.
    let its: Vec<(f64, Item)> = all
        .iter()
        .copied()
        .filter(|(_, it)| !matches!(it, Item::Insert(_)))
        .collect();
    let gone = |it: Item| match it {
        Item::Word(i) => sc.words[i].cut,
        Item::Silence(i) => removed(&sc.silences[i]),
        Item::Insert(_) => false,
    };
    let sp = |it: Item| span(sc, it);
    let mid = |it: Item| {
        let (s, e) = sp(it);
        match it {
            Item::Word(_) => (s + e) / 2.0,
            _ => e,
        }
    };
    // A point between two items with no gap goes at the quiet point there.
    let snap = |k_next: usize, t: f64| {
        let lo = k_next.checked_sub(1).map_or(0.0, |p| mid(its[p].1));
        let hi = its
            .get(k_next)
            .map_or(dur, |n| (sp(n.1).0 + SNAP_SECS).min(mid(n.1)));
        quiet_before(buffer, lo.max(t - SNAP_SECS), hi, t)
    };
    let mut cuts: Vec<(f64, f64)> = Vec::new();
    let mut adds: Vec<(f64, usize, Add)> = Vec::new();
    let mut k = 0;
    while k < its.len() {
        let it = its[k].1;
        if !gone(it) {
            if let Item::Silence(i) = it {
                let s = &sc.silences[i];
                let full = s.end - s.start;
                let keep = kept(s);
                if keep < full - 1e-6 {
                    cuts.push((s.start + keep / 2.0, s.end - keep / 2.0));
                } else if keep > full + 1e-6 {
                    let at = if full < 0.05 {
                        snap(k + 1, s.start)
                    } else {
                        (s.start + s.end) / 2.0
                    };
                    let seq = all.iter().position(|x| x.1 == it).unwrap_or(0);
                    adds.push((at, seq, Add::Gap(keep - full)));
                }
            }
            k += 1;
            continue;
        }
        // A run of removed words and pauses goes as one piece. It runs from just
        // before its first word starts to just before the next kept word starts.
        let first = k;
        while k < its.len() && gone(its[k].1) {
            k += 1;
        }
        let (fs, _) = sp(its[first].1);
        let (ls, le) = sp(its[k - 1].1);
        let a = if first == 0 {
            0.0
        } else {
            let prev = its[first - 1].1;
            let lo = (sp(prev).1 - SNAP_SECS).max(mid(prev).min(fs));
            let hi = (fs + SNAP_SECS).min(match its[first].1 {
                Item::Word(_) => mid(its[first].1),
                _ => sp(its[first].1).1,
            });
            quiet_before(buffer, lo.min(fs), hi.max(fs), fs)
        };
        let b = match its.get(k) {
            None => dur,
            Some(&(ns, next)) => {
                let lo = (le - SNAP_SECS)
                    .max(ls.max(mid(its[k - 1].1).min(le)))
                    .max(a);
                let hi = (ns + SNAP_SECS).min(mid(next));
                quiet_before(buffer, lo.min(ns), hi.max(ns), ns)
            }
        };
        if b > a {
            cuts.push((a, b));
        }
    }
    // Inserted words with audio go at the quiet point between their neighbours.
    let mut voice_audio = Vec::new();
    let mut voice_ids = Vec::new();
    for (seq, (t, it)) in all.iter().enumerate() {
        let Item::Insert(i) = *it else { continue };
        let Some(v) = voices.get(&sc.inserts[i].id).filter(|v| !v.is_empty()) else {
            continue;
        };
        let next = its.partition_point(|x| x.0 <= *t);
        adds.push((snap(next, *t), seq, Add::Voice(voice_audio.len())));
        voice_audio.push(v.clone());
        voice_ids.push(sc.inserts[i].id.clone());
    }
    cuts.sort_by(|x, y| x.0.total_cmp(&y.0));
    adds.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
    let mut pieces = Vec::new();
    let mut pos = 0.0;
    let piece = |at: f64, add: &Add| match *add {
        Add::Gap(len) => Piece::Gap { at, len },
        Add::Voice(idx) => Piece::Voice {
            at,
            // The first crossfade overlaps the audio before it.
            len: (voice_audio[idx].len() as f64 / rate - FADE_SECS).max(0.0),
            idx,
        },
    };
    let mut ins = adds.iter().peekable();
    for (a, b) in cuts {
        while let Some(&&(at, _, ref add)) = ins.peek().filter(|i| i.0 <= a) {
            let at = at.max(pos);
            if at > pos {
                pieces.push(Piece::Audio(pos, at));
            }
            pieces.push(piece(at, add));
            pos = at;
            ins.next();
        }
        if a > pos {
            pieces.push(Piece::Audio(pos, a));
        }
        pos = pos.max(b);
    }
    for (at, _, add) in ins {
        let at = at.max(pos);
        if at > pos {
            pieces.push(Piece::Audio(pos, at));
        }
        pieces.push(piece(at, add));
        pos = at;
    }
    if dur > pos {
        pieces.push(Piece::Audio(pos, dur));
    }
    Edl {
        pieces,
        voices: voice_audio,
        voice_ids,
    }
}

/// Length of the clip as edited, from the sidecar alone, for showing in the clip
/// list. Cut edges are not moved to quiet points here, so it can differ from `plan`
/// by a fraction of a second. `voices` holds the length in seconds of each inserted
/// word's audio, by insert id.
pub fn edited_length(sc: &Sidecar, voices: &HashMap<String, f64>) -> f64 {
    let its: Vec<(f64, Item)> = items(sc)
        .into_iter()
        .filter(|(_, it)| !matches!(it, Item::Insert(_)))
        .collect();
    let gone = |it: Item| match it {
        Item::Word(i) => sc.words[i].cut,
        Item::Silence(i) => removed(&sc.silences[i]),
        Item::Insert(_) => false,
    };
    let mut len = sc.duration;
    let mut k = 0;
    while k < its.len() {
        let it = its[k].1;
        if !gone(it) {
            if let Item::Silence(i) = it {
                let s = &sc.silences[i];
                len += kept(s) - (s.end - s.start);
            }
            k += 1;
            continue;
        }
        // Like `plan`: a run of removed items goes from its first start to the start
        // of the next kept item.
        let a = if k == 0 { 0.0 } else { span(sc, it).0 };
        while k < its.len() && gone(its[k].1) {
            k += 1;
        }
        let b = its.get(k).map_or(sc.duration, |n| span(sc, n.1).0);
        len -= (b - a).max(0.0);
    }
    let spoken: f64 = sc
        .inserts
        .iter()
        .filter_map(|i| voices.get(&i.id))
        .map(|s| (s - FADE_SECS).max(0.0))
        .sum();
    (len + spoken).max(0.0)
}

impl Edl {
    /// True when the edit changes nothing.
    pub fn is_identity(&self) -> bool {
        match self.pieces.as_slice() {
            [] => true,
            [Piece::Audio(a, _)] => *a == 0.0,
            _ => false,
        }
    }

    /// A time in the recording, in the edited result. Inside a cut it maps to where
    /// the cut was.
    pub fn to_edited(&self, t: f64) -> f64 {
        if self.pieces.is_empty() {
            return t;
        }
        let mut acc = 0.0;
        for p in &self.pieces {
            match *p {
                Piece::Audio(a, b) => {
                    if t < a {
                        return acc;
                    }
                    if t < b {
                        return acc + t - a;
                    }
                    acc += b - a;
                }
                Piece::Gap { at, len } | Piece::Voice { at, len, .. } => {
                    if t < at {
                        return acc;
                    }
                    acc += len;
                }
            }
        }
        acc
    }

    /// A time in the edited result, in the recording.
    pub fn to_original(&self, t: f64) -> f64 {
        let mut t = t;
        let mut end = t;
        for p in &self.pieces {
            match *p {
                Piece::Audio(a, b) => {
                    if t <= b - a {
                        return a + t;
                    }
                    t -= b - a;
                    end = b;
                }
                Piece::Gap { at, len } | Piece::Voice { at, len, .. } => {
                    if t < len {
                        return at;
                    }
                    t -= len;
                    end = at;
                }
            }
        }
        if self.pieces.is_empty() { t } else { end }
    }

    /// Renders the edited audio. Each join gets a short equal-power crossfade that
    /// sits before the join, so every piece keeps exactly its length.
    pub fn render(&self, buffer: &Buffer) -> Buffer {
        if self.is_identity() {
            return buffer.clone();
        }
        let rate = buffer.sample_rate as f64;
        let ch = buffer.channels.max(1) as usize;
        let frames = buffer.frames() as usize;
        let fade = (FADE_SECS * rate) as usize;
        let at_frame = |t: f64| ((t * rate).round() as usize).min(frames);
        let mut out: Vec<f32> = Vec::with_capacity(buffer.samples.len());
        let mut last_end: Option<usize> = None;
        for p in &self.pieces {
            match *p {
                Piece::Audio(a, b) => {
                    let (fa, fb) = (at_frame(a), at_frame(b));
                    if fb <= fa {
                        continue;
                    }
                    if last_end != Some(fa) {
                        let n = fade.min(out.len() / ch).min(fa);
                        crossfade(&mut out, &buffer.samples[(fa - n) * ch..fa * ch], ch);
                    }
                    out.extend_from_slice(&buffer.samples[fa * ch..fb * ch]);
                    last_end = Some(fb);
                }
                Piece::Gap { at, len } => {
                    let want = (len * rate).round() as usize;
                    let n = fade.min(out.len() / ch);
                    let fill = tile(&room_tone(buffer, at), want + n, ch, fade);
                    crossfade(&mut out, &fill[..n * ch], ch);
                    out.extend_from_slice(&fill[n * ch..]);
                    last_end = None;
                }
                Piece::Voice { at, len, idx } => {
                    // Generated speech is studio clean; the room's own background noise
                    // under it keeps the noise floor from dropping out.
                    let v = &self.voices[idx];
                    let want = (len * rate).round() as usize;
                    let n = fade.min(out.len() / ch).min(v.len());
                    let total = (want + n).min(v.len());
                    let tone = tile(&room_tone(buffer, at), total, ch, fade);
                    let mut fill = vec![0f32; total * ch];
                    for f in 0..total {
                        for c in 0..ch {
                            fill[f * ch + c] = v[f] + tone[f * ch + c];
                        }
                    }
                    crossfade(&mut out, &fill[..n * ch], ch);
                    out.extend_from_slice(&fill[n * ch..]);
                    last_end = None;
                }
            }
        }
        Buffer {
            sample_rate: buffer.sample_rate,
            channels: buffer.channels,
            samples: out,
        }
    }
}

/// Repeats `seg` to `frames` frames, crossfading each repeat into the next.
fn tile(seg: &[f32], frames: usize, ch: usize, fade: usize) -> Vec<f32> {
    let seg_frames = seg.len() / ch;
    if seg_frames <= fade * 2 {
        return vec![0.0; frames * ch];
    }
    let mut out: Vec<f32> = Vec::with_capacity((frames + seg_frames) * ch);
    while out.len() / ch < frames {
        if out.is_empty() {
            out.extend_from_slice(seg);
        } else {
            crossfade(&mut out, &seg[..fade * ch], ch);
            out.extend_from_slice(&seg[fade * ch..]);
        }
    }
    out.truncate(frames * ch);
    out
}

impl Edl {
    /// Where each inserted word plays in the edited result: (insert id, start, end).
    pub fn spoken(&self) -> Vec<(String, f64, f64)> {
        let mut acc = 0.0;
        let mut out = Vec::new();
        for p in &self.pieces {
            match *p {
                Piece::Audio(a, b) => acc += b - a,
                Piece::Gap { len, .. } => acc += len,
                Piece::Voice { len, idx, .. } => {
                    out.push((self.voice_ids[idx].clone(), acc, acc + len));
                    acc += len;
                }
            }
        }
        out
    }
}

/// The words in the edited result with their times there: recorded words that were
/// not cut, and inserted words that have audio. They keep transcript order; a word
/// whose rough Whisper time falls inside an inserted word starts after it.
pub fn edited_words(sc: &Sidecar, edl: &Edl) -> Vec<Word> {
    let mut spoken: HashMap<String, (f64, f64)> = edl
        .spoken()
        .into_iter()
        .map(|(id, s, e)| (id, (s, e)))
        .collect();
    // A phrase is one piece of audio: share its time out over its words by length.
    for run in insert_runs(sc) {
        let lead = &sc.inserts[run[0]].id;
        let Some(&(s, e)) = spoken.get(lead) else {
            continue;
        };
        let members: Vec<&Insert> = run
            .iter()
            .map(|&j| &sc.inserts[j])
            .filter(|x| &x.id == lead || x.with.as_ref() == Some(lead))
            .collect();
        let letters = |x: &Insert| {
            x.text
                .chars()
                .filter(|c| c.is_alphanumeric())
                .count()
                .max(1)
        };
        let total: usize = members.iter().map(|x| letters(x)).sum();
        let mut t = s;
        for x in members {
            let len = (e - s) * letters(x) as f64 / total as f64;
            spoken.insert(x.id.clone(), (t, t + len));
            t += len;
        }
    }
    let mut out: Vec<Word> = Vec::new();
    for (_, it) in items(sc) {
        let mut w = match it {
            Item::Word(i) if !sc.words[i].cut => {
                let w = &sc.words[i];
                Word {
                    start: edl.to_edited(w.start),
                    end: edl.to_edited(w.end),
                    ..w.clone()
                }
            }
            Item::Insert(i) => {
                let ins = &sc.inserts[i];
                let Some(&(start, end)) = spoken.get(&ins.id) else {
                    continue;
                };
                Word {
                    id: ins.id.clone(),
                    text: ins.text.clone(),
                    start,
                    end,
                    cut: false,
                }
            }
            _ => continue,
        };
        if let Some(prev) = out.last() {
            w.start = w.start.max(prev.end);
            w.end = w.end.max(w.start);
        }
        out.push(w);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::{number_words, render_text};

    /// `apply_text` on loose word and pause lists.
    fn apply(text: &str, w: &mut Vec<Word>, s: &mut Vec<Silence>) -> Result<Changes, String> {
        let mut sc = Sidecar {
            words: w.clone(),
            silences: s.clone(),
            ..Default::default()
        };
        let r = apply_text(text, &mut sc);
        *w = sc.words;
        *s = sc.silences;
        r
    }

    fn plan_ws(w: &[Word], s: &[Silence], buf: &Buffer) -> Edl {
        let sc = Sidecar {
            words: w.to_vec(),
            silences: s.to_vec(),
            ..Default::default()
        };
        plan(&sc, &HashMap::new(), buf)
    }

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
        let c = apply("Hei [silence:2s] der ja [silence:1.5s] nei", &mut w, &mut s).unwrap();
        assert_eq!(c.pauses, 1);
        assert_eq!(s[0].keep, Some(2.0));
        assert_eq!(s[1].keep, None);
        apply("Hei [silence:7s] der ja [silence:1,5s] nei", &mut w, &mut s).unwrap();
        assert_eq!(s[0].keep, None);
        assert!(apply("Hei [silence:abc] der", &mut w, &mut s).is_err());
    }

    #[test]
    fn cut_words_and_pauses() {
        let (mut w, mut s) = sample();
        // Delete "ja", and the second pause.
        let c = apply("Hei [silence:7s] der nei", &mut w, &mut s).unwrap();
        assert_eq!((c.words_cut, c.pauses), (1, 1));
        assert!(w[2].cut);
        assert_eq!(s[1].keep, Some(0.0));
        assert_eq!(render_text(&w, &s), "Hei [silence:7s] der nei");
        // Typing "ja" back restores it.
        let c = apply("Hei [silence:7s] der ja nei", &mut w, &mut s).unwrap();
        assert_eq!(c.words_restored, 1);
        assert!(!w[2].cut);
    }

    #[test]
    fn retyped_words_keep_their_audio() {
        let (mut w, mut s) = sample();
        let c = apply(
            "Hei [silence:7s] dér ja [silence:1.5s] nei hallo",
            &mut w,
            &mut s,
        )
        .unwrap();
        assert_eq!((c.words_retyped, c.spoken, c.words_cut), (1, 1, 0));
        assert_eq!(w[1].text, "dér");
        // Deleting one word and fixing its neighbour pairs the fix with the right word.
        let (mut w, mut s) = sample();
        apply("Hei [silence:7s] Ja [silence:1.5s] nei", &mut w, &mut s).unwrap();
        assert!(w[1].cut);
        assert_eq!((w[2].text.as_str(), w[2].cut), ("Ja", false));
    }

    /// 1 kHz test audio: a loud tone where `spans` say, silence elsewhere.
    fn audio(spans: &[(f64, f64)], dur: f64) -> Buffer {
        let rate = 1000;
        let samples = (0..(dur * rate as f64) as usize)
            .map(|f| {
                let t = f as f64 / rate as f64;
                let on = spans.iter().any(|&(a, b)| t >= a && t < b);
                if on {
                    if f % 2 == 0 { 0.5 } else { -0.5 }
                } else {
                    0.0
                }
            })
            .collect();
        Buffer {
            sample_rate: rate,
            channels: 1,
            samples,
        }
    }

    fn loud_frames(b: &Buffer) -> usize {
        b.samples.iter().filter(|x| x.abs() > 0.1).count()
    }

    /// "da skal jeg se": real word sounds, and Whisper's rougher, back-to-back times.
    fn speech() -> (Vec<Word>, Buffer) {
        let words = number_words(vec![
            ("da".into(), 0.0, 0.55),
            ("skal".into(), 0.55, 1.05),
            ("jeg".into(), 1.05, 1.48),
            ("se".into(), 1.48, 2.0),
        ]);
        let buf = audio(&[(0.0, 0.5), (0.6, 1.0), (1.1, 1.4), (1.5, 1.9)], 2.0);
        (words, buf)
    }

    #[test]
    fn cut_word_snaps_to_quiet_points() {
        let (mut w, buf) = speech();
        let mut s = Vec::new();
        apply("da skal se", &mut w, &mut s).unwrap();
        let edl = plan_ws(&w, &s, &buf);
        let out = edl.render(&buf);
        // All of "jeg" is gone, and nothing of "skal" or "se" is lost.
        assert_eq!(loud_frames(&out), 500 + 400 + 400);
        // The pause after "skal" stays, so the words do not run together.
        let gap = edl.to_edited(1.5) - edl.to_edited(1.0);
        assert!(gap > 0.08 && gap < 0.12, "gap {gap}");
    }

    #[test]
    fn cut_keeps_a_word_whisper_placed_too_early() {
        // Real case: after a long pause Whisper put "OK." 0.7 s before it was said.
        // Cutting the next word must not take "OK." with it. Low room noise between
        // the words, deeper silence before "OK.".
        let rate = 1000usize;
        let mut buf = audio(&[(1.6, 2.0), (2.3, 2.8), (3.0, 3.4)], 3.6);
        for (f, x) in buf.samples.iter_mut().enumerate() {
            let t = f as f64 / rate as f64;
            if *x == 0.0 && t > 1.45 {
                // About -66 dB, like the breath gaps in Max's recordings.
                *x = if f % 2 == 0 { 0.0005 } else { -0.0005 };
            }
        }
        let mut w = number_words(vec![
            ("OK.".into(), 0.86, 1.43),
            ("Så".into(), 2.34, 2.82),
            ("litt".into(), 3.0, 3.4),
        ]);
        apply("OK. litt", &mut w, &mut Vec::new()).unwrap();
        let out = plan_ws(&w, &[], &buf).render(&buf);
        assert_eq!(loud_frames(&out), 400 + 400);
    }

    #[test]
    fn punctuation_alone_is_not_a_respelling() {
        let mut w = number_words(vec![
            ("litt".into(), 0.0, 0.3),
            ("om".into(), 0.3, 0.6),
            ("meg.".into(), 0.6, 1.0),
        ]);
        // Deleting "meg" but leaving its period cuts the word; the lone "." has no
        // audio of its own, so it is not mistaken for a respelling of "meg.".
        let c = apply("litt om .", &mut w, &mut Vec::new()).unwrap();
        assert!(w[2].cut);
        assert_eq!((c.words_cut, c.words_retyped, c.ignored), (1, 0, 1));
    }

    #[test]
    fn add_pause_between_words() {
        let (mut w, buf) = speech();
        let mut s = Vec::new();
        let c = apply("da skal [silence:1s] jeg se", &mut w, &mut s).unwrap();
        assert_eq!(c.pauses, 1);
        assert_eq!(render_text(&w, &s), "da skal [silence:1s] jeg se");
        let edl = plan_ws(&w, &s, &buf);
        let out = edl.render(&buf);
        assert_eq!(out.samples.len(), 3000);
        assert_eq!(loud_frames(&out), loud_frames(&buf));
        // "jeg" now starts a second later.
        assert!((edl.to_edited(1.1) - 2.1).abs() < 0.01);
        // Deleting the tag again removes the added pause.
        apply("da skal jeg se", &mut w, &mut s).unwrap();
        assert!(s.is_empty());
        // Lengthening an existing pause works too.
        let mut s = vec![sil(0.5, 0.6)];
        let mut w2 = speech().0;
        apply("da [silence:2s] skal jeg se", &mut w2, &mut s).unwrap();
        assert_eq!(s[0].keep, Some(2.0));
        assert_eq!(plan_ws(&w2, &s, &buf).render(&buf).samples.len(), 3900);
    }

    /// The transcript's words, without pauses.
    fn shown_words(sc: &Sidecar) -> String {
        visible(sc)
            .into_iter()
            .filter(|i| i.kind != Kind::Pause)
            .map(|i| i.text)
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn words_typed_between_new_words_keep_their_order() {
        let (w, _) = speech();
        let mut sc = Sidecar {
            words: w,
            ..Default::default()
        };
        apply_text("da skal hei alle sammen jeg se", &mut sc).unwrap();
        assert_eq!(shown_words(&sc), "da skal hei alle sammen jeg se");
        // New words between the earlier ones, which sit close together.
        let t = "da skal hei halla alle godt sammen folkens jeg se";
        apply_text(t, &mut sc).unwrap();
        assert_eq!(shown_words(&sc), t);
        let t = "da skal hei halla alle godt sammen x y z folkens jeg se";
        apply_text(t, &mut sc).unwrap();
        assert_eq!(shown_words(&sc), t);
    }

    #[test]
    fn new_words_stay_on_their_side_of_a_retyped_word() {
        // "jeg" retyped as "Jeg" keeps its recording; the new words around it must not
        // spread across it.
        for t in ["da skal trolig var Jeg se", "da skal Jeg trolig var se"] {
            let (w, _) = speech();
            let mut sc = Sidecar {
                words: w,
                ..Default::default()
            };
            apply_text(t, &mut sc).unwrap();
            assert_eq!(shown_words(&sc), t);
        }
    }

    #[test]
    fn a_changed_phrase_is_spoken_again_whole() {
        let (w, _) = speech();
        let mut sc = Sidecar {
            words: w,
            ..Default::default()
        };
        apply_text("da skal hei alle sammen jeg se", &mut sc).unwrap();
        // Spoken as one phrase: the first word holds the audio.
        let lead = sc.inserts[0].id.clone();
        for (k, ins) in sc.inserts.iter_mut().enumerate() {
            if k == 0 {
                ins.file = Some(format!("{lead}.wav"));
            } else {
                ins.with = Some(lead.clone());
            }
        }
        let ready = |sc: &Sidecar| {
            visible(sc)
                .iter()
                .filter(|i| matches!(i.kind, Kind::Spoken { ready: true, .. }))
                .count()
        };
        assert_eq!(ready(&sc), 3);
        // An unrelated edit keeps it.
        apply_text("da skal hei alle sammen jeg", &mut sc).unwrap();
        assert_eq!(ready(&sc), 3);
        // Removing a word of the phrase means speaking the rest again, as one phrase.
        apply_text("da skal hei sammen jeg", &mut sc).unwrap();
        assert_eq!(ready(&sc), 0);
        assert!(
            sc.inserts
                .iter()
                .all(|i| i.file.is_none() && i.with.is_none())
        );
    }

    #[test]
    fn inserted_word_plays_between_its_neighbours() {
        let (w, buf) = speech();
        let mut sc = Sidecar {
            words: w,
            ..Default::default()
        };
        let c = apply_text("da skal jeg virkelig se", &mut sc).unwrap();
        assert_eq!(c.spoken, 1);
        // Not generated yet: plays as before.
        assert_eq!(plan(&sc, &HashMap::new(), &buf).render(&buf), buf);
        // 0.3 s of generated speech.
        let voice: Vec<f32> = (0..300)
            .map(|f| if f % 2 == 0 { 0.4 } else { -0.4 })
            .collect();
        let voices = HashMap::from([(sc.inserts[0].id.clone(), voice)]);
        let edl = plan(&sc, &voices, &buf);
        let out = edl.render(&buf);
        assert_eq!(out.samples.len(), 2000 + 300 - 15);
        // It sits in the quiet gap before "se" (sound at 1.5 s), after "jeg".
        let (_, start, end) = edl.spoken()[0].clone();
        assert!((start - 1.495).abs() < 0.01, "{start}");
        assert!((end - start - 0.285).abs() < 1e-9);
        assert!((edl.to_edited(1.5) - (1.5 + 0.285)).abs() < 0.01);
        let words = edited_words(&sc, &edl);
        assert_eq!(
            words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(),
            ["da", "skal", "jeg", "virkelig", "se"]
        );
    }

    #[test]
    fn edl_maps_both_ways() {
        let edl = Edl {
            pieces: vec![
                Piece::Audio(0.0, 2.0),
                Piece::Audio(5.0, 8.0),
                Piece::Gap { at: 8.0, len: 1.0 },
                Piece::Audio(8.0, 10.0),
            ],
            voices: Vec::new(),
            voice_ids: Vec::new(),
        };
        assert_eq!(edl.to_edited(1.0), 1.0);
        assert_eq!(edl.to_edited(3.0), 2.0);
        assert_eq!(edl.to_edited(6.0), 3.0);
        assert_eq!(edl.to_edited(8.0), 6.0);
        assert_eq!(edl.to_edited(10.0), 8.0);
        assert_eq!(edl.to_original(1.0), 1.0);
        assert_eq!(edl.to_original(3.0), 6.0);
        assert_eq!(edl.to_original(5.5), 8.0);
        assert_eq!(edl.to_original(7.0), 9.0);
    }

    #[test]
    fn shortened_pause_removes_exact_length() {
        let buf = audio(&[(0.0, 1.0), (5.0, 6.0)], 6.0);
        let mut s = sil(1.0, 5.0);
        s.keep = Some(1.0);
        let out = plan_ws(&[], &[s], &buf).render(&buf);
        assert_eq!(out.samples.len(), 3000);
        assert_eq!(loud_frames(&out), 2000);
        // Unedited pauses leave the audio untouched.
        assert_eq!(plan_ws(&[], &[sil(1.0, 5.0)], &buf).render(&buf), buf);
    }

    #[test]
    fn edited_length_follows_the_edit() {
        let (mut words, mut silences) = sample();
        words[1].cut = true; // "der" (8.0) goes until "ja" starts (8.7)
        silences[0].keep = Some(2.0); // 7 s pause becomes 2 s
        let mut sc = Sidecar {
            duration: 12.0,
            words,
            silences,
            ..Default::default()
        };
        assert!((edited_length(&sc, &HashMap::new()) - 6.3).abs() < 1e-9);
        sc.inserts.push(Insert {
            id: "v1".into(),
            text: "faktisk".into(),
            at: 0.5,
            file: Some("v1.wav".into()),
            error: None,
            with: None,
        });
        let voices = HashMap::from([("v1".to_string(), 1.0)]);
        let want = 6.3 + 1.0 - FADE_SECS;
        assert!((edited_length(&sc, &voices) - want).abs() < 1e-9);
    }
}
