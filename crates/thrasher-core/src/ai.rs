//! Local AI: a small language model, run by the `thrasher-ask` worker, that writes a
//! summary and a to-do list for a transcript. This module holds the model choices and
//! builds the jobs the worker reads; it has no llama.cpp code itself.
//!
//! Model research: `research/thrasher/2026-10-26-local-llm.md` (outside the repo).

use crate::models::{Group, Heads, ModelInfo};
use crate::sidecar::Sidecar;
use serde::Serialize;
use std::path::Path;

/// The models offered, best for Norwegian first. Both are Q4_K_M GGUF files.
pub const AI_MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "borealis-open-4b",
        name: "Borealis 4B",
        group: Group::Norwegian,
        url: "https://huggingface.co/NbAiLab/borealis-open-4b-gguf/resolve/main/borealis-open-4b-Q4_K_M.gguf",
        bytes: 2_489_894_560,
        sha256: "e751152cf2c4cf87fcf75b8c38f3b15b4cc7b86dd75743c26bdace08e7e65c60",
        heads: Heads::Base,
        language: "no",
        quality: 4,
        speed: 3,
        compressed: true,
    },
    ModelInfo {
        id: "ministral-3-3b",
        name: "Ministral 3B",
        group: Group::AllLanguages,
        url: "https://huggingface.co/unsloth/Ministral-3-3B-Instruct-2512-GGUF/resolve/main/Ministral-3-3B-Instruct-2512-Q4_K_M.gguf",
        bytes: 2_146_497_824,
        sha256: "fd46fc371ff0509bfa8657ac956b7de8534d7d9baaa4947975c0648c3aa397f4",
        heads: Heads::Base,
        language: "",
        quality: 3,
        speed: 4,
        compressed: true,
    },
];

pub fn find(id: &str) -> Option<&'static ModelInfo> {
    AI_MODELS.iter().find(|m| m.id == id)
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Message {
    pub role: &'static str,
    pub content: String,
}

/// What the worker reads on stdin; see `thrasher-ask`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Job {
    pub model: String,
    pub max_tokens: u32,
    pub steps: Vec<Vec<Message>>,
}

/// Words per part when a transcript is too long to summarise in one go.
const PART_WORDS: usize = 2500;

struct Prompts {
    system: &'static str,
    whole: &'static str,
    part: &'static str,
    parts: &'static str,
}

const NORWEGIAN: Prompts = Prompts {
    system: "Du lager korte notater fra transkripsjoner av lydopptak. Skriv på norsk bokmål. \
             Bruk bare det som står i teksten, og ikke finn på noe.",
    whole: "Her er en transkripsjon:\n\n{text}\n\nSkriv to deler med disse overskriftene:\n\
            Sammendrag: tre til seks setninger om hva opptaket handler om.\n\
            Gjøremål: en punktliste med det som må gjøres eller følges opp, ett punkt per \
            linje med «- » foran. Skriv «Ingen» hvis det ikke er noen.",
    part: "Her er en del av en transkripsjon:\n\n{text}\n\nSkriv korte stikkord om hva som \
           blir sagt, og alt som må gjøres eller følges opp.",
    parts: "Her er stikkord fra alle delene av et opptak, i rekkefølge:\n\n{prev}\n\n\
            Skriv to deler med disse overskriftene:\n\
            Sammendrag: tre til seks setninger om hva opptaket handler om.\n\
            Gjøremål: en punktliste med det som må gjøres eller følges opp, ett punkt per \
            linje med «- » foran. Skriv «Ingen» hvis det ikke er noen.",
};

const ENGLISH: Prompts = Prompts {
    system: "You write short notes from transcripts of recordings. Use only what the text \
             says and never make anything up. Write in the language of the transcript.",
    whole: "Here is a transcript:\n\n{text}\n\nWrite two parts with these headings:\n\
            Summary: three to six sentences on what the recording is about.\n\
            To do: a list of what must be done or followed up, one item per line \
            starting with \"- \". Write \"None\" if there is nothing.",
    part: "Here is part of a transcript:\n\n{text}\n\nWrite short notes on what is said, \
           and everything that must be done or followed up.",
    parts: "Here are notes from every part of a recording, in order:\n\n{prev}\n\n\
            Write two parts with these headings:\n\
            Summary: three to six sentences on what the recording is about.\n\
            To do: a list of what must be done or followed up, one item per line \
            starting with \"- \". Write \"None\" if there is nothing.",
};

/// The transcript as plain text: the words that are kept, inside the trim.
fn plain_words(sc: &Sidecar) -> Vec<&str> {
    sc.words
        .iter()
        .filter(|w| !w.cut && sc.in_trim(w.start))
        .map(|w| w.text.trim())
        .filter(|w| !w.is_empty())
        .collect()
}

/// The job that writes a summary and a to-do list for the clip, or None when it has
/// no transcript. Norwegian transcripts get Norwegian prompts.
pub fn summary_job(sc: &Sidecar, model: &Path) -> Option<Job> {
    let words = plain_words(sc);
    if words.is_empty() {
        return None;
    }
    let p = match sc.language.as_deref() {
        Some("no" | "nb" | "nn") | None => &NORWEGIAN,
        _ => &ENGLISH,
    };
    let chat = |user: String| {
        vec![
            Message {
                role: "system",
                content: p.system.to_string(),
            },
            Message {
                role: "user",
                content: user,
            },
        ]
    };
    let steps = if words.len() <= PART_WORDS {
        vec![chat(p.whole.replace("{text}", &words.join(" ")))]
    } else {
        let mut steps: Vec<_> = words
            .chunks(PART_WORDS)
            .map(|part| chat(p.part.replace("{text}", &part.join(" "))))
            .collect();
        steps.push(chat(p.parts.to_string()));
        steps
    };
    Some(Job {
        model: model.to_string_lossy().into_owned(),
        max_tokens: 700,
        steps,
    })
}

/// Tidies a model's answer for plain text: markdown headings, bold, italics, rules
/// and bullet stars go.
pub fn tidy(text: &str) -> String {
    text.lines()
        .filter(|l| {
            let t = l.trim();
            // A rule such as `---`.
            t.len() < 3 || !t.chars().all(|c| matches!(c, '-' | '*' | '_' | '='))
        })
        .map(|l| {
            let l = l.trim_end();
            let indent = &l[..l.len() - l.trim_start().len()];
            let t = l.trim_start();
            let (lead, rest) = if let Some(h) = t.strip_prefix('#') {
                ("", h.trim_start_matches('#').trim())
            } else if let Some(item) = t.strip_prefix("* ") {
                ("- ", item)
            } else {
                ("", t)
            };
            let rest = rest.replace(['*', '`'], "").replace("__", "");
            format!("{indent}{lead}{rest}")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// A stretch of one clip's transcript that a question can be answered from.
#[derive(Clone, Debug, PartialEq)]
pub struct Passage {
    pub clip: String,
    /// Where the clip lives, as the caller numbers places (projects).
    pub place: i32,
    /// When the clip was made, for "the last lecture".
    pub created: u64,
    /// Start of the passage, in recording seconds.
    pub at: f64,
    pub text: String,
}

/// Words per passage, and how far each next one starts (they overlap a little).
const PASSAGE_WORDS: usize = 120;
const PASSAGE_STEP: usize = 90;
/// Passages handed to the model with a question.
const ASK_PASSAGES: usize = 8;

/// Cuts a clip's transcript into overlapping passages.
pub fn passages(sc: &Sidecar, clip: &str, place: i32, created: u64) -> Vec<Passage> {
    let words: Vec<_> = sc
        .words
        .iter()
        .filter(|w| !w.cut && sc.in_trim(w.start) && !w.text.trim().is_empty())
        .collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let part = &words[i..(i + PASSAGE_WORDS).min(words.len())];
        out.push(Passage {
            clip: clip.to_string(),
            place,
            created,
            at: part[0].start,
            text: part
                .iter()
                .map(|w| w.text.trim())
                .collect::<Vec<_>>()
                .join(" "),
        });
        if i + PASSAGE_WORDS >= words.len() {
            break;
        }
        i += PASSAGE_STEP;
    }
    out
}

/// Common words that say nothing about the subject.
const STOP: &[&str] = &[
    "og", "i", "jeg", "det", "at", "en", "et", "den", "til", "er", "som", "på", "de", "med", "han",
    "av", "ikke", "der", "så", "var", "meg", "seg", "men", "ett", "har", "om", "vi", "min", "mitt",
    "ha", "hadde", "hun", "nå", "over", "da", "ved", "fra", "du", "ut", "sin", "dem", "oss", "opp",
    "man", "kan", "hans", "hvor", "eller", "hva", "skal", "selv", "sjøl", "her", "alle", "vil",
    "bli", "ble", "blitt", "kunne", "inn", "når", "være", "kom", "noen", "noe", "ville", "dere",
    "deres", "kun", "ja", "etter", "ned", "skulle", "denne", "for", "deg", "si", "sine", "sitt",
    "mot", "å", "meget", "hvorfor", "dette", "disse", "uten", "hvordan", "ingen", "din", "ditt",
    "blir", "samme", "hvilken", "hvilke", "sånn", "inni", "the", "a", "an", "and", "or", "of",
    "to", "in", "on", "at", "is", "are", "was", "were", "it", "that", "this", "what", "which",
    "who", "how", "why", "do", "did", "does", "i", "you", "we", "my", "me", "for", "with", "about",
    "be", "been", "from", "as", "by",
];

/// Words that ask about the newest recording.
const RECENT: &[&str] = &[
    "siste",
    "sist",
    "forrige",
    "nyeste",
    "idag",
    "igår",
    "last",
    "latest",
    "newest",
    "previous",
    "today",
    "yesterday",
    "recent",
];

/// Lowercase words, stop words left out, cut to six letters so endings such as
/// "-ene" or "-ing" still match.
fn terms(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 2 && !STOP.contains(w))
        .map(|w| w.chars().take(6).collect())
        .collect()
}

/// The passages that best fit `question`, best first: BM25 over the passages, and
/// for a question about the latest recording only the newest clip's passages.
pub fn retrieve(question: &str, all: &[Passage]) -> Vec<Passage> {
    let lower = question.to_lowercase().replace(' ', "");
    let recent = RECENT.iter().any(|w| lower.contains(w));
    let newest = all.iter().map(|p| p.created).max().unwrap_or(0);
    let pool: Vec<&Passage> = all
        .iter()
        .filter(|p| !recent || p.created == newest)
        .collect();
    let q = terms(question);
    let docs: Vec<Vec<String>> = pool.iter().map(|p| terms(&p.text)).collect();
    let n = docs.len().max(1) as f64;
    let avg = docs.iter().map(Vec::len).sum::<usize>() as f64 / n;
    let (k1, b) = (1.2, 0.75);
    let mut scored: Vec<(f64, usize)> = docs
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let len = d.len() as f64;
            let score: f64 = q
                .iter()
                .map(|t| {
                    let tf = d.iter().filter(|w| *w == t).count() as f64;
                    if tf == 0.0 {
                        return 0.0;
                    }
                    let df = docs.iter().filter(|d| d.contains(t)).count() as f64;
                    let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
                    idf * tf * (k1 + 1.0) / (tf + k1 * (1.0 - b + b * len / avg.max(1.0)))
                })
                .sum();
            (score, i)
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut picked: Vec<Passage> = scored
        .iter()
        .filter(|(s, _)| *s > 0.0)
        .take(ASK_PASSAGES)
        .map(|(_, i)| pool[*i].clone())
        .collect();
    // "What happened in the last lecture" has no subject words: read it from the start.
    if picked.is_empty() && recent {
        picked = pool
            .iter()
            .take(ASK_PASSAGES)
            .map(|p| (*p).clone())
            .collect();
    }
    picked
}

/// The job that answers `question` from `picked` (see `retrieve`), citing them as
/// [1], [2] and so on in that order.
pub fn ask_job(question: &str, picked: &[Passage], model: &Path) -> Job {
    let excerpts = picked
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let t = p.at as u64;
            format!(
                "[{}] {} ({}:{:02})\n{}",
                i + 1,
                p.clip,
                t / 60,
                t % 60,
                p.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let system = "You answer questions about the user's own recordings, using only the \
                  excerpts from their transcripts below. Answer in the same language as \
                  the question, briefly. Put the number of each excerpt you use in square \
                  brackets, like [2]. If the excerpts do not answer the question, say so.";
    let user = format!("Excerpts:\n\n{excerpts}\n\nQuestion: {question}");
    Job {
        model: model.to_string_lossy().into_owned(),
        max_tokens: 500,
        steps: vec![vec![
            Message {
                role: "system",
                content: system.to_string(),
            },
            Message {
                role: "user",
                content: user,
            },
        ]],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::number_words;

    fn sc(words: usize, language: &str) -> Sidecar {
        Sidecar {
            language: Some(language.into()),
            words: number_words(
                (0..words)
                    .map(|i| (format!("ord{i}"), i as f64, i as f64 + 0.5))
                    .collect(),
            ),
            ..Default::default()
        }
    }

    #[test]
    fn short_transcripts_take_one_step_long_ones_parts() {
        let model = Path::new("m.gguf");
        let job = summary_job(&sc(10, "no"), model).unwrap();
        assert_eq!(job.steps.len(), 1);
        assert!(job.steps[0][1].content.contains("ord9"));
        assert!(job.steps[0][1].content.contains("Gjøremål"));
        let job = summary_job(&sc(6000, "en"), model).unwrap();
        // Three parts, then the whole.
        assert_eq!(job.steps.len(), 4);
        assert!(job.steps[3][1].content.contains("{prev}"));
        assert!(job.steps[3][1].content.contains("To do"));
        assert!(summary_job(&Sidecar::default(), model).is_none());
    }

    #[test]
    fn tidy_drops_markdown() {
        assert_eq!(
            tidy("## **Sammendrag**\nDet gikk *bra*.\n\n---\n* fikse taket\n  - ringe Per"),
            "Sammendrag\nDet gikk bra.\n\n- fikse taket\n  - ringe Per"
        );
    }

    fn talk(words: &[&str]) -> Sidecar {
        Sidecar {
            words: number_words(
                words
                    .iter()
                    .enumerate()
                    .map(|(i, w)| (w.to_string(), i as f64 * 0.5, i as f64 * 0.5 + 0.4))
                    .collect(),
            ),
            ..Default::default()
        }
    }

    #[test]
    fn passages_overlap_and_keep_their_time() {
        let words: Vec<String> = (0..250).map(|i| format!("w{i}")).collect();
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let ps = passages(&talk(&refs), "lecture", 0, 1);
        assert_eq!(ps.len(), 3);
        assert_eq!(ps[1].at, 45.0);
        assert!(ps[0].text.ends_with("w119") && ps[1].text.starts_with("w90"));
    }

    #[test]
    fn retrieves_by_subject_or_by_recency() {
        let mut all = passages(&talk(&["taket", "lekker", "på", "kontoret"]), "site", 0, 10);
        all.extend(passages(
            &talk(&["matte", "integraler", "var", "vanskelige"]),
            "lecture 1",
            1,
            20,
        ));
        all.extend(passages(
            &talk(&["fysikk", "bølger", "var", "greit"]),
            "lecture 2",
            1,
            30,
        ));
        let hit = retrieve("Hva sa jeg om taket?", &all);
        assert_eq!(hit[0].clip, "site");
        let hit = retrieve("hvilke integraler var vanskelig", &all);
        assert_eq!(hit[0].clip, "lecture 1");
        // "The last lecture" reads only the newest clip, even with no subject word.
        let hit = retrieve(
            "Hva var det jeg syntes var vanskelig i siste forelesning?",
            &all,
        );
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].clip, "lecture 2");
        let job = ask_job("hva?", &hit, Path::new("m.gguf"));
        assert!(job.steps[0][1].content.contains("[1] lecture 2 (0:00)"));
    }

    #[test]
    fn model_files_are_ggufs() {
        for m in AI_MODELS {
            assert!(m.url.ends_with(".gguf"), "{}", m.id);
            assert_eq!(m.sha256.len(), 64);
        }
    }
}
