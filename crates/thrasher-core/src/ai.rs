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

    #[test]
    fn model_files_are_ggufs() {
        for m in AI_MODELS {
            assert!(m.url.ends_with(".gguf"), "{}", m.id);
            assert_eq!(m.sha256.len(), 64);
        }
    }
}
