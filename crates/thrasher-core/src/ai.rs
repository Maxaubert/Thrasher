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
    /// Pass on every step's answer, not just the last (a translation in parts).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub emit_all: bool,
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
        emit_all: false,
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
        emit_all: false,
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

/// Words of the transcript the model reads to name a clip.
const NAME_WORDS: usize = 600;

/// The job that names a clip from its transcript: a few words, in the transcript's
/// language. None when there is no transcript.
pub fn name_job(sc: &Sidecar, model: &Path) -> Option<Job> {
    let words = plain_words(sc);
    if words.is_empty() {
        return None;
    }
    let text = words[..words.len().min(NAME_WORDS)].join(" ");
    // Two steps: a sentence on what the recording is, then a title made from that.
    // Asked for a title straight away, a small model tends to copy the first words;
    // given an example title, it copies the example.
    let (system, describe, title) = match sc.language.as_deref() {
        Some("no" | "nb" | "nn") | None => (
            "Du lager korte notater fra lydopptak. Skriv på norsk bokmål.",
            format!(
                "Transkripsjon:\n\"\"\"\n{text}\n\"\"\"\n\nSkriv én kort setning om hva slags \
                 opptak dette er og hva det handler om."
            ),
            "Her er en beskrivelse av et lydopptak:\n{prev}\n\nLag en kort tittel på 2 til 5 \
             ord og høyst 40 tegn. Begynn med hva slags opptak det er (presentasjon, møte, \
             forelesning, befaring, huskeliste, samtale eller notat), og si så hva det \
             handler om, med navn fra beskrivelsen. Skriv vanlige ord med mellomrom. Svar \
             bare med tittelen, uten punktum.",
        ),
        _ => (
            "You write short notes from recordings, in the language of the transcript.",
            format!(
                "Transcript:\n\"\"\"\n{text}\n\"\"\"\n\nWrite one short sentence on what kind \
                 of recording this is and what it is about."
            ),
            "Here is a description of a recording:\n{prev}\n\nWrite a short title of 2 to 5 \
             words and at most 40 letters. Start with the kind of recording (presentation, \
             meeting, lecture, site visit, to-do list, conversation or note), then say what \
             it is about, with names from the description. Use normal words with spaces. \
             Reply with the title only, no full stop.",
        ),
    };
    let chat = |user: String| {
        vec![
            Message {
                role: "system",
                content: system.to_string(),
            },
            Message {
                role: "user",
                content: user,
            },
        ]
    };
    Some(Job {
        model: model.to_string_lossy().into_owned(),
        max_tokens: 60,
        steps: vec![chat(describe), chat(title.to_string())],
        emit_all: false,
    })
}

/// Longest clip name the AI may give.
const TITLE_CHARS: usize = 40;
/// Small words a cut title must not end on.
const DANGLING: &[&str] = &[
    "om", "og", "av", "på", "i", "til", "for", "med", "å", "fra", "ved", "en", "et", "about",
    "and", "of", "on", "in", "to", "for", "with", "the", "a", "an", "from",
];

/// Makes a clip name of the model's title: one line, no quotes, labels, markdown or
/// end punctuation, nothing a file name cannot hold, at most 40 letters cut at a word
/// and not ending on a word such as "om". The first letter is a capital. None when
/// nothing usable is left.
pub fn clean_title(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut t = line.to_string();
    for label in ["Tittel:", "tittel:", "Title:", "title:"] {
        if let Some(rest) = t.strip_prefix(label) {
            t = rest.to_string();
        }
    }
    let t: String = t
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .filter(|c| !matches!(c, '«' | '»' | '“' | '”' | '‘' | '’' | '`' | '*'))
        .collect();
    let t = t
        .trim()
        .trim_matches(|c: char| matches!(c, '\'' | '.' | '!' | ',' | ';' | '-'))
        .trim();
    let mut out = String::new();
    for w in t.split_whitespace() {
        if out.chars().count() + w.chars().count() + 1 > TITLE_CHARS {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w);
    }
    // A cut title should not end on "om", "og" and the like.
    while let Some((head, last)) = out.rsplit_once(' ') {
        if DANGLING.contains(&last.to_lowercase().as_str()) {
            out.truncate(head.len());
        } else {
            break;
        }
    }
    let mut chars = out.chars();
    let out: String = match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    };
    crate::library::valid_name(&out).then_some(out)
}

/// A language the translate menu offers: code, English name, and its own name for
/// searching.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Language {
    pub code: &'static str,
    pub name: &'static str,
    pub native: &'static str,
}

pub const LANGUAGES: &[Language] = &[
    Language {
        code: "en",
        name: "English",
        native: "English",
    },
    Language {
        code: "no",
        name: "Norwegian",
        native: "Norsk",
    },
    Language {
        code: "sv",
        name: "Swedish",
        native: "Svenska",
    },
    Language {
        code: "da",
        name: "Danish",
        native: "Dansk",
    },
    Language {
        code: "fi",
        name: "Finnish",
        native: "Suomi",
    },
    Language {
        code: "de",
        name: "German",
        native: "Deutsch",
    },
    Language {
        code: "nl",
        name: "Dutch",
        native: "Nederlands",
    },
    Language {
        code: "fr",
        name: "French",
        native: "Français",
    },
    Language {
        code: "es",
        name: "Spanish",
        native: "Español",
    },
    Language {
        code: "pt",
        name: "Portuguese",
        native: "Português",
    },
    Language {
        code: "it",
        name: "Italian",
        native: "Italiano",
    },
    Language {
        code: "pl",
        name: "Polish",
        native: "Polski",
    },
    Language {
        code: "uk",
        name: "Ukrainian",
        native: "Українська",
    },
    Language {
        code: "ar",
        name: "Arabic",
        native: "العربية",
    },
    Language {
        code: "zh",
        name: "Chinese",
        native: "中文",
    },
    Language {
        code: "ja",
        name: "Japanese",
        native: "日本語",
    },
];

/// Words per part when the local AI translates a transcript.
const TRANSLATE_WORDS: usize = 1200;

/// The AI model that translates. Borealis, tuned for Norwegian, answers a request to
/// translate with the text unchanged (tested 2026-10-26); Ministral translates well,
/// into Nordic languages too.
pub const TRANSLATOR: &str = "ministral-3-3b";

/// The English name of a language code, for prompts.
fn language_name(code: &str) -> &'static str {
    let code = if matches!(code, "nb" | "nn") {
        "no"
    } else {
        code
    };
    LANGUAGES
        .iter()
        .find(|l| l.code == code)
        .map_or("the original language", |l| l.name)
}

/// The job that translates the clip's transcript into `language` with the local AI,
/// a part at a time, every part passed on. None without a transcript.
pub fn translate_job(sc: &Sidecar, language: &Language, model: &Path) -> Option<Job> {
    let words = plain_words(sc);
    if words.is_empty() {
        return None;
    }
    let from = language_name(sc.language.as_deref().unwrap_or("no"));
    let to = language.name;
    let steps = words
        .chunks(TRANSLATE_WORDS)
        .map(|part| {
            vec![
                Message {
                    role: "system",
                    content: "You are a translator. You always reply in the target language."
                        .to_string(),
                },
                Message {
                    role: "user",
                    content: format!(
                        "Translate the following {from} text into {to}. Reply with the {to} \
                         translation only.\n\n{from}: {}\n{to}:",
                        part.join(" ")
                    ),
                },
            ]
        })
        .collect();
    Some(Job {
        model: model.to_string_lossy().into_owned(),
        max_tokens: 2400,
        steps,
        emit_all: true,
    })
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
    fn titles_become_clean_clip_names() {
        assert_eq!(
            clean_title("«Møte om taket.»").as_deref(),
            Some("Møte om taket")
        );
        assert_eq!(
            clean_title("Tittel: Plan for uke 12\nmer").as_deref(),
            Some("Plan for uke 12")
        );
        assert_eq!(
            clean_title("A/B test: results?").as_deref(),
            Some("A B test results")
        );
        assert!(clean_title("  \n ").is_none());
        let long = "ord ".repeat(40);
        assert!(clean_title(&long).unwrap().chars().count() <= 40);
        assert_eq!(
            clean_title("presentasjon om opprettelse av HMS-agent og rapportering").as_deref(),
            Some("Presentasjon om opprettelse av HMS-agent")
        );
        assert_eq!(
            clean_title("**Møte om taket**").as_deref(),
            Some("Møte om taket")
        );
        let job = name_job(&sc(10, "no"), Path::new("m.gguf")).unwrap();
        assert_eq!(job.steps.len(), 2);
        assert!(job.steps[1][1].content.contains("{prev}"));
    }

    #[test]
    fn translations_go_in_parts_and_pass_every_part_on() {
        let lang = LANGUAGES.iter().find(|l| l.code == "de").unwrap();
        let job = translate_job(&sc(3000, "no"), lang, Path::new("m.gguf")).unwrap();
        assert_eq!(job.steps.len(), 3);
        assert!(job.emit_all);
        assert!(
            job.steps[0][1]
                .content
                .contains("Norwegian text into German")
        );
        assert!(translate_job(&Sidecar::default(), lang, Path::new("m.gguf")).is_none());
        assert!(
            serde_json::to_string(&job)
                .unwrap()
                .contains("\"emit_all\":true")
        );
    }

    #[test]
    fn model_files_are_ggufs() {
        for m in AI_MODELS {
            assert!(m.url.ends_with(".gguf"), "{}", m.id);
            assert_eq!(m.sha256.len(), 64);
        }
    }
}
