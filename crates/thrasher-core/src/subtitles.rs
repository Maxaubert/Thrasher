//! Subtitles from word timings: SRT and WebVTT. Words are grouped into cues that
//! break at pauses, at sentence ends, and when a cue gets too long to read.

use crate::sidecar::Word;
use std::fmt::Write;

const MAX_CHARS: usize = 42;
const MAX_SECS: f64 = 6.0;
/// A pause this long always starts a new cue.
const BREAK_GAP: f64 = 0.7;

#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

pub fn cues(words: &[Word]) -> Vec<Cue> {
    let mut out: Vec<Cue> = Vec::new();
    let mut cur: Option<Cue> = None;
    for w in words {
        if let Some(c) = &mut cur {
            let gap = w.start - c.end;
            let ends_sentence = c.text.ends_with(['.', '?', '!']);
            let too_long = c.text.chars().count() + 1 + w.text.chars().count() > MAX_CHARS
                || w.end - c.start > MAX_SECS;
            if gap >= BREAK_GAP || ends_sentence || too_long {
                out.push(cur.take().unwrap());
            }
        }
        match &mut cur {
            Some(c) => {
                c.text.push(' ');
                c.text.push_str(&w.text);
                c.end = w.end;
            }
            None => {
                cur = Some(Cue {
                    start: w.start,
                    end: w.end,
                    text: w.text.clone(),
                })
            }
        }
    }
    out.extend(cur);
    out
}

fn stamp(secs: f64, sep: char) -> String {
    let ms = (secs.max(0.0) * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}{sep}{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

/// The transcript as readable text, one sentence or spoken stretch per line, each
/// starting with its time: `[01:23] Hei der.`
pub fn script(words: &[Word]) -> String {
    let mut s = String::new();
    let mut line: Option<(f64, String)> = None;
    let mut last_end = 0.0;
    for c in cues(words) {
        // A long pause starts a new line even mid-sentence.
        if let Some((start, text)) = line.take_if(|_| c.start - last_end >= 2.0) {
            let _ = writeln!(s, "[{}] {text}", clock(start));
        }
        match &mut line {
            Some((_, text)) => {
                text.push(' ');
                text.push_str(&c.text);
            }
            None => line = Some((c.start, c.text.clone())),
        }
        last_end = c.end;
        if c.text.ends_with(['.', '?', '!'])
            && let Some((start, text)) = line.take()
        {
            let _ = writeln!(s, "[{}] {text}", clock(start));
        }
    }
    if let Some((start, text)) = line {
        let _ = writeln!(s, "[{}] {text}", clock(start));
    }
    s
}

/// `01:23`, or `1:02:03` past an hour.
fn clock(secs: f64) -> String {
    let t = secs.max(0.0) as u64;
    if t >= 3600 {
        format!("{}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60)
    } else {
        format!("{:02}:{:02}", t / 60, t % 60)
    }
}

pub fn srt(words: &[Word]) -> String {
    let mut s = String::new();
    for (i, c) in cues(words).iter().enumerate() {
        let _ = write!(
            s,
            "{}\r\n{} --> {}\r\n{}\r\n\r\n",
            i + 1,
            stamp(c.start, ','),
            stamp(c.end, ','),
            c.text
        );
    }
    s
}

pub fn vtt(words: &[Word]) -> String {
    let mut s = String::from("WEBVTT\n\n");
    for c in cues(words) {
        let _ = write!(
            s,
            "{} --> {}\n{}\n\n",
            stamp(c.start, '.'),
            stamp(c.end, '.'),
            c.text
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::number_words;

    fn words() -> Vec<Word> {
        number_words(vec![
            ("Hei".into(), 0.5, 0.8),
            ("der.".into(), 0.85, 1.1),
            ("Dette".into(), 1.2, 1.5),
            ("er".into(), 1.55, 1.7),
            ("en".into(), 3.0, 3.1),
            ("test".into(), 3.15, 3.5),
        ])
    }

    #[test]
    fn breaks_at_sentences_and_pauses() {
        let c = cues(&words());
        let texts: Vec<_> = c.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, ["Hei der.", "Dette er", "en test"]);
        assert_eq!((c[1].start, c[1].end), (1.2, 1.7));
    }

    #[test]
    fn long_lines_wrap_into_new_cues() {
        let many = number_words(
            (0..20)
                .map(|i| ("ordet".to_string(), i as f64 * 0.3, i as f64 * 0.3 + 0.25))
                .collect(),
        );
        assert!(
            cues(&many)
                .iter()
                .all(|c| c.text.chars().count() <= MAX_CHARS)
        );
    }

    #[test]
    fn formats() {
        let s = srt(&words());
        assert!(s.starts_with("1\r\n00:00:00,500 --> 00:00:01,100\r\nHei der.\r\n"));
        let v = vtt(&words());
        assert!(v.starts_with("WEBVTT\n\n00:00:00.500 --> 00:00:01.100\nHei der.\n"));
        assert_eq!(stamp(3725.0042, '.'), "01:02:05.004");
    }

    #[test]
    fn script_has_a_time_per_sentence() {
        let s = script(&words());
        let lines: Vec<&str> = s.lines().collect();
        assert!(lines[0].starts_with("[00:00] Hei der."), "{s}");
        assert!(lines.len() >= 2, "{s}");
        assert!(lines.iter().all(|l| l.starts_with('[')));
        assert_eq!(clock(3723.0), "1:02:03");
    }
}
