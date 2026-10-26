//! Finding text in clip transcripts and names, for the search field over the list.
//!
//! A transcript becomes one lowercase string of its words (cut words left out), so a
//! search can span several words. Each hit knows the word it starts at, so the clip
//! can play from there.

use crate::sidecar::Sidecar;

/// A transcript prepared for searching.
#[derive(Clone, Debug, Default)]
pub struct Doc {
    /// The words, lowercase, joined by single spaces.
    text: String,
    /// Where each word starts in `text`, its start time and its text as said.
    words: Vec<(usize, f64, String)>,
}

/// Words shown before and after the match.
const BEFORE: usize = 4;
const AFTER: usize = 8;

/// Where a search matched, ready to show.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    /// Start of the first matching word, in recording time. 0 for a name match.
    pub at: f64,
    pub before: String,
    pub matched: String,
    pub after: String,
    /// How many more times the text occurs in the transcript.
    pub more: usize,
}

pub fn doc(sc: &Sidecar) -> Doc {
    let mut d = Doc::default();
    for w in sc.words.iter().filter(|w| !w.cut) {
        let word = w.text.trim();
        if word.is_empty() {
            continue;
        }
        if !d.text.is_empty() {
            d.text.push(' ');
        }
        d.words.push((d.text.len(), w.start, word.to_string()));
        d.text.push_str(&word.to_lowercase());
    }
    d
}

/// The word that holds byte `at` of the doc's text.
fn word_at(d: &Doc, at: usize) -> usize {
    d.words
        .partition_point(|(start, _, _)| *start <= at)
        .saturating_sub(1)
}

/// Finds `query` (any case) in the clip's name or transcript. The transcript's first
/// match wins; the name counts when the transcript has none.
pub fn find(name: &str, d: &Doc, query: &str) -> Option<Hit> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return None;
    }
    let mut found = d.text.match_indices(&q).map(|(i, _)| i);
    if let Some(first) = found.next() {
        let (a, b) = (word_at(d, first), word_at(d, first + q.len() - 1));
        let join = |r: std::ops::Range<usize>| {
            d.words[r]
                .iter()
                .map(|(_, _, w)| w.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        };
        return Some(Hit {
            at: d.words[a].1,
            before: join(a.saturating_sub(BEFORE)..a),
            matched: join(a..b + 1),
            after: join(b + 1..(b + 1 + AFTER).min(d.words.len())),
            more: found.count(),
        });
    }
    name.to_lowercase().contains(&q).then(|| Hit {
        at: 0.0,
        before: String::new(),
        matched: String::new(),
        after: String::new(),
        more: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::Word;

    fn sidecar(words: &[(&str, f64)]) -> Sidecar {
        Sidecar {
            words: words
                .iter()
                .enumerate()
                .map(|(i, (t, s))| Word {
                    id: format!("w{i}"),
                    text: t.to_string(),
                    start: *s,
                    end: s + 0.3,
                    cut: false,
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn finds_words_and_phrases_with_their_time() {
        let sc = sidecar(&[
            ("Vi", 0.0),
            ("må", 0.4),
            ("fikse", 0.8),
            ("Taket", 1.2),
            ("i", 1.6),
            ("dag.", 2.0),
            ("Taket", 5.0),
        ]);
        let d = doc(&sc);
        let h = find("notes", &d, "taket i").unwrap();
        assert_eq!(h.at, 1.2);
        assert_eq!(
            (h.before.as_str(), h.matched.as_str()),
            ("Vi må fikse", "Taket i")
        );
        assert_eq!(h.after, "dag. Taket");
        assert_eq!(h.more, 0);
        let h = find("notes", &d, "TAKET").unwrap();
        assert_eq!((h.at, h.more), (1.2, 1));
        // Part of a word matches too, from that word.
        assert_eq!(find("notes", &d, "ikse").unwrap().matched, "fikse");
    }

    #[test]
    fn falls_back_to_the_name_and_skips_cut_words() {
        let mut sc = sidecar(&[("hei", 0.0), ("der", 0.5)]);
        sc.words[1].cut = true;
        let d = doc(&sc);
        assert!(find("Standup", &d, "der").is_none());
        let h = find("Standup notes", &d, "standup").unwrap();
        assert_eq!((h.at, h.matched.as_str()), (0.0, ""));
        assert!(find("x", &d, "  ").is_none());
    }
}
