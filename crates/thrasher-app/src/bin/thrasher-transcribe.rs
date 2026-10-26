//! Transcription worker. The app starts it for each job, so the model and the CUDA
//! runtime (hundreds of MB) leave memory when the job ends, and a crash inside
//! whisper.cpp cannot take a running recording down with it.
//!
//! Usage: thrasher-transcribe [--translate] <clip folder> <clip name> <model id> <language or ""> [more clip names]
//! With `--translate` the result is written in English as the clip's translation, and
//! the transcript is left alone.
//! Prints `progress <0..1>` lines, then `done`. Errors go to stderr with exit code 1.
//! Extra clip names are transcribed with the same loaded model (used to find inserted
//! words in generated speech).

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use thrasher_core::library::Library;
use thrasher_core::models;
use thrasher_core::player::load_clip;
use thrasher_core::sidecar::Sidecar;
use thrasher_core::transcribe::{self, Options, Transcriber};
use thrasher_core::whisper::WhisperTranscriber;

fn run(args: &[String]) -> Result<(), String> {
    let (translate, args) = match args.split_first() {
        Some((first, rest)) if first == "--translate" => (true, rest),
        _ => (false, args),
    };
    let [dir, name, model_id, language, more @ ..] = args else {
        return Err("usage: thrasher-transcribe <folder> <clip> <model id> <language>".into());
    };
    let library = Library::open(PathBuf::from(dir)).map_err(|e| e.to_string())?;
    let model = models::find(model_id).ok_or_else(|| format!("unknown model {model_id}"))?;
    let path = model.path().ok_or("no local app data folder")?;
    let mut engine = WhisperTranscriber::load(&path, model.heads, true)?;
    let options = Options {
        language: (!language.is_empty()).then(|| language.clone()),
        translate,
        ..Default::default()
    };
    let names: Vec<&String> = std::iter::once(name).chain(more).collect();
    for (i, name) in names.iter().enumerate() {
        transcribe_one(&library, name, &mut engine, &options, i, names.len())?;
    }
    println!("done");
    Ok(())
}

fn transcribe_one(
    library: &Library,
    name: &str,
    engine: &mut WhisperTranscriber,
    options: &Options,
    index: usize,
    count: usize,
) -> Result<(), String> {
    let buffer = load_clip(library, name).map_err(|e| e.to_string())?;
    let mono = transcribe::to_mono(&buffer.samples, buffer.channels);
    let audio = transcribe::resample(&mono, buffer.sample_rate, transcribe::RATE);
    let duration = buffer.duration_secs();

    let mut out = std::io::stdout();
    let result = engine.transcribe(
        &audio,
        options,
        &mut |p| {
            let p = (index as f32 + p) / count as f32;
            let _ = writeln!(out, "progress {p}");
            let _ = out.flush();
        },
        &AtomicBool::new(false),
    )?;

    let silences = transcribe::silences(&result.words, duration, transcribe::DEFAULT_MIN_SILENCE);
    let sc_path = library.sidecar_path(name);
    let mut sc = Sidecar::load(&sc_path).unwrap_or_else(|_| Sidecar {
        file: format!("{name}.wav"),
        duration,
        ..Default::default()
    });
    if options.translate {
        sc.translation = result.words;
    } else {
        sc.language = result.language;
        sc.words = result.words;
        sc.silences = silences;
    }
    sc.save(&sc_path).map_err(|e| e.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
