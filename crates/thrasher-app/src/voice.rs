//! Speaks inserted words in the user's own voice. Runs the Python voice worker
//! (`voice/worker.py`, Chatterbox Multilingual V3) in its own environment, then the
//! transcription worker to find the new words in what it generated, and splices them
//! in through the clip's sidecar. See `docs/voice-plan.md`.
//!
//! The environment (Python 3.12, PyTorch with CUDA 12.8, Chatterbox) is created with
//! `uv` on first use in `%LOCALAPPDATA%\Thrasher\voice`, so the installer stays small.

use crate::AppWindow;
use slint::ComponentHandle;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thrasher_core::library::Library;
use thrasher_core::player::Buffer;
use thrasher_core::sidecar::{Sidecar, TrackKind};
use thrasher_core::transcribe::to_mono;
use thrasher_core::{voice, wav};

const WORKER: &str = include_str!("../voice/worker.py");
/// Pinned sources. Archive links, so no git is needed.
const CHATTERBOX: &str = "https://github.com/resemble-ai/chatterbox/archive/5de7a54aa4e5e2baadb0182dde554908b48b85c2.zip";
const PERTH: &str =
    "https://github.com/resemble-ai/Perth/archive/ff1c8ac55a976971245cdd53c18d6131ca00d993.zip";
/// Written into the environment once it is complete; a change forces a fresh setup.
const ENGINE: &str = "chatterbox 5de7a54, perth ff1c8ac, torch 2.8.0+cu128, python 3.12";

fn base() -> Option<PathBuf> {
    thrasher_core::models::models_dir().and_then(|m| m.parent().map(|p| p.join("voice")))
}

fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console window flashes up.
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

/// Runs a process, passing `progress <x>` lines on. On failure, the error is the last
/// line it wrote to stderr.
fn run(cmd: &mut Command, on_progress: &mut dyn FnMut(f32)) -> Result<(), String> {
    hidden(cmd).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut stderr = child.stderr.take().unwrap();
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    for line in BufReader::new(child.stdout.take().unwrap()).lines() {
        let line = line.map_err(|e| e.to_string())?;
        if let Some(p) = line
            .strip_prefix("progress ")
            .and_then(|p| p.trim().parse::<f32>().ok())
        {
            on_progress(p);
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let err = err_reader.join().unwrap_or_default();
    if status.success() {
        return Ok(());
    }
    let last = err
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.contains('%'))
        .unwrap_or("the worker stopped");
    Err(last.to_string())
}

fn find_uv() -> Option<PathBuf> {
    let ok = |p: &Path| {
        hidden(Command::new(p).arg("--version"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    if ok(Path::new("uv")) {
        return Some(PathBuf::from("uv"));
    }
    let home = directories::BaseDirs::new()?
        .home_dir()
        .join(".local/bin/uv.exe");
    ok(&home).then_some(home)
}

/// The Python of the voice environment, setting it up first if needed.
fn engine(status: &dyn Fn(String)) -> Result<PathBuf, String> {
    let base = base().ok_or("no local app data folder")?;
    let env = base.join("env");
    let python = env.join("Scripts").join("python.exe");
    let marker = env.join("thrasher-engine.txt");
    if python.exists() && std::fs::read_to_string(&marker).is_ok_and(|m| m == ENGINE) {
        return Ok(python);
    }
    let uv = find_uv().ok_or(
        "Speaking new words needs uv, a small Python installer. Install it with \
         \"winget install astral-sh.uv\", then try again.",
    )?;
    status("Setting up the voice engine. First time only, about 4 GB to download…".into());
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    run(
        Command::new(&uv)
            .args(["venv", "--clear", "--python", "3.12"])
            .arg(&env),
        &mut |_| {},
    )?;
    let overrides = base.join("overrides.txt");
    std::fs::write(
        &overrides,
        format!("torch==2.8.0+cu128\ntorchaudio==2.8.0+cu128\nresemble-perth @ {PERTH}\n"),
    )
    .map_err(|e| e.to_string())?;
    run(
        Command::new(&uv)
            .args(["pip", "install", "--python"])
            .arg(&python)
            .arg("--override")
            .arg(&overrides)
            .args([
                "--extra-index-url",
                "https://download.pytorch.org/whl/cu128",
                "--index-strategy",
                "unsafe-best-match",
                "setuptools<81",
                "soundfile",
            ])
            .arg(format!("chatterbox-tts @ {CHATTERBOX}")),
        &mut |_| {},
    )?;
    std::fs::write(&marker, ENGINE).map_err(|e| e.to_string())?;
    Ok(python)
}

/// Mono generated audio from a worker output file.
fn read_mono(path: &Path) -> Result<Buffer, String> {
    let (spec, samples) = wav::read_f32(path).map_err(|e| e.to_string())?;
    Ok(Buffer {
        sample_rate: spec.sample_rate,
        channels: 1,
        samples: to_mono(&samples, spec.channels),
    })
}

/// Generates audio for every insert of the clip that still needs it, and records the
/// result (a file or an error) in the sidecar.
fn speak(
    library: &Library,
    name: &str,
    model_id: &str,
    language: &str,
    status: &dyn Fn(String),
    progress: &dyn Fn(f32),
) -> Result<usize, String> {
    let sc_path = library.sidecar_path(name);
    let sc = Sidecar::load(&sc_path).map_err(|e| e.to_string())?;
    let jobs = voice::jobs(&sc);
    if jobs.is_empty() {
        return Ok(0);
    }
    let segments = voice::pick_reference(&sc).map_err(|e| e.to_string())?;
    let python = engine(status)?;
    let count: usize = jobs.iter().map(|j| j.words.len()).sum();
    status(format!("Speaking {count} new word(s) in your voice…"));
    let reference = voice::write_reference(library, name, &segments).map_err(|e| e.to_string())?;
    let dir = voice::voice_dir(library, name);
    let base = base().ok_or("no local app data folder")?;
    let worker = base.join("worker.py");
    std::fs::write(&worker, WORKER).map_err(|e| e.to_string())?;
    let gen_names: Vec<String> = jobs.iter().map(|j| format!("gen_{}", j.ids[0])).collect();
    let list: Vec<serde_json::Value> = jobs
        .iter()
        .zip(&gen_names)
        .map(|(j, n)| serde_json::json!({ "name": n, "text": j.text() }))
        .collect();
    let jobs_file = dir.join("jobs.json");
    std::fs::write(&jobs_file, serde_json::to_string(&list).unwrap()).map_err(|e| e.to_string())?;

    // 1. Speak the phrases (most of the time goes here).
    run(
        Command::new(&python)
            .arg(&worker)
            .arg(&reference)
            .arg(if language.is_empty() { "no" } else { language })
            .arg(&jobs_file)
            .arg(&dir)
            .env("HF_HOME", base.join("models"))
            .env("PYTHONIOENCODING", "utf-8"),
        &mut |p| progress(p * 0.8),
    )?;
    // 2. Find the new words in them.
    let transcriber = crate::transcription::worker_path()
        .ok_or("Transcription is not part of this build, so new words cannot be placed")?;
    let mut cmd = Command::new(transcriber);
    cmd.arg(&dir)
        .arg(&gen_names[0])
        .arg(model_id)
        .arg(language)
        .args(&gen_names[1..]);
    run(&mut cmd, &mut |p| progress(0.8 + p * 0.2))?;

    // 3. Cut them out, match loudness, save.
    let (spec, mic) =
        wav::read_f32(&library.track_path(name, TrackKind::Mic)).map_err(|e| e.to_string())?;
    let mic = to_mono(&mic, spec.channels);
    let mut results: Vec<(String, Result<String, String>)> = Vec::new();
    for (job, gen_name) in jobs.iter().zip(&gen_names) {
        let outcome = (|| {
            let generated = read_mono(&dir.join(format!("{gen_name}.wav")))?;
            let gen_sc =
                Sidecar::load(&dir.join(format!("{gen_name}.json"))).map_err(|e| e.to_string())?;
            voice::extract(job, &generated, &gen_sc.words).map(|w| (generated.sample_rate, w))
        })();
        match outcome {
            Ok((rate, words)) => {
                for (id, audio) in job.ids.iter().zip(words) {
                    let at = sc
                        .inserts
                        .iter()
                        .find(|i| &i.id == id)
                        .map_or(0.0, |i| i.at);
                    let around = voice::speech_around(&sc, &mic, spec.sample_rate, at, 4.0);
                    let fitted = voice::fit(&audio, rate, &around, spec.sample_rate);
                    // Ids were checked by `voice::jobs` (`v<number>`), so this stays in
                    // the clip's .voice folder.
                    let file = voice::insert_file(id);
                    let saved = wav::WavWriter::create(&dir.join(&file), spec.sample_rate, 1)
                        .and_then(|mut w| {
                            w.write(&fitted)?;
                            w.finalize()
                        })
                        .map(|_| file)
                        .map_err(|e| e.to_string());
                    results.push((id.clone(), saved));
                }
            }
            Err(e) => results.extend(job.ids.iter().map(|id| (id.clone(), Err(e.clone())))),
        }
        for ext in ["wav", "json"] {
            let _ = std::fs::remove_file(dir.join(format!("{gen_name}.{ext}")));
        }
    }
    let _ = std::fs::remove_file(&jobs_file);

    // The user may have edited meanwhile: update only the inserts that still exist.
    let mut sc = Sidecar::load(&sc_path).map_err(|e| e.to_string())?;
    let mut failed = None;
    for (id, r) in results {
        if let Some(ins) = sc.inserts.iter_mut().find(|i| i.id == id) {
            match r {
                Ok(file) => {
                    ins.file = Some(file);
                    ins.error = None;
                }
                Err(e) => {
                    ins.error = Some(e.clone());
                    failed = Some(e);
                }
            }
        }
    }
    sc.save(&sc_path).map_err(|e| e.to_string())?;
    match failed {
        Some(e) => Err(format!(
            "Some words could not be spoken: {e}. Delete and retype them to try again"
        )),
        None => Ok(count),
    }
}

/// Starts speaking the clip's new words on a background thread.
pub fn start(ui: &AppWindow, library: &Library, name: String, model_id: String, language: String) {
    if ui.get_speaking() {
        return;
    }
    ui.set_speaking(true);
    let weak = ui.as_weak();
    let dir = library.dir().to_path_buf();
    std::thread::spawn(move || {
        let w = weak.clone();
        let status = move |s: String| {
            let _ = w.upgrade_in_event_loop(move |ui| ui.set_status(s.into()));
        };
        let w = weak.clone();
        let progress = move |p: f32| {
            let _ = w.upgrade_in_event_loop(move |ui| ui.set_speak_progress(p));
        };
        let started = std::time::Instant::now();
        let result = Library::open(&dir)
            .map_err(|e| e.to_string())
            .and_then(|lib| speak(&lib, &name, &model_id, &language, &status, &progress));
        let secs = started.elapsed().as_secs_f64();
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_speaking(false);
            match result {
                Ok(0) => {}
                Ok(n) => ui.set_status(
                    format!("Spoke {n} new word(s) in your voice in {secs:.0} s").into(),
                ),
                Err(e) => ui.set_status(e.into()),
            }
            ui.invoke_selection_changed(ui.get_selected());
        });
    });
}
