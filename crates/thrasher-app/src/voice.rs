//! Speaks inserted words in the user's own voice. Runs the Python voice worker
//! (`voice/worker.py`) in its own environment, then the transcription worker to find
//! the new words in what it generated, and splices them in through the clip's sidecar.
//! See `docs/voice-plan.md`.
//!
//! Two engines, chosen in the Models panel:
//! - OmniVoice (default): best Norwegian accent in Max's listening test (2026-10-23).
//!   Its weights are CC-BY-NC, so the output may not be used commercially.
//! - Chatterbox Multilingual V3: MIT, so it can be used commercially.
//!
//! Each engine gets its own environment (Python 3.12, PyTorch with CUDA 12.8), created
//! with `uv` on first use in `%LOCALAPPDATA%\Thrasher\voice`, so the installer stays small.

use crate::AppWindow;
use slint::ComponentHandle;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
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
const TORCH: &str = "torch==2.8.0+cu128\ntorchaudio==2.8.0+cu128\n";

/// The voice model that speaks new words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    OmniVoice,
    Chatterbox,
}

impl Engine {
    /// Order of the choices in the Models panel.
    pub const ALL: [Engine; 2] = [Engine::OmniVoice, Engine::Chatterbox];

    /// From the `voice_engine` setting; anything unknown means the default.
    pub fn from_setting(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|e| e.id() == s)
            .unwrap_or(Engine::OmniVoice)
    }

    pub fn id(self) -> &'static str {
        match self {
            Engine::OmniVoice => "omnivoice",
            Engine::Chatterbox => "chatterbox",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Engine::OmniVoice => "OmniVoice",
            Engine::Chatterbox => "Chatterbox",
        }
    }

    fn env_dir(self) -> &'static str {
        match self {
            Engine::OmniVoice => "env-omnivoice",
            // Kept from before there was a choice, so existing setups are reused.
            Engine::Chatterbox => "env",
        }
    }

    /// Written into the environment once it is complete; a change forces a fresh setup.
    fn marker(self) -> &'static str {
        match self {
            Engine::OmniVoice => {
                "omnivoice 0.2.1, transformers 5.20.2, torch 2.8.0+cu128, python 3.12"
            }
            Engine::Chatterbox => {
                "chatterbox 5de7a54, perth ff1c8ac, torch 2.8.0+cu128, python 3.12"
            }
        }
    }

    /// Version overrides and packages to install.
    fn packages(self) -> (String, Vec<String>) {
        match self {
            Engine::OmniVoice => (
                TORCH.to_string(),
                vec![
                    "omnivoice==0.2.1".into(),
                    "transformers==5.20.2".into(),
                    "accelerate==1.15.0".into(),
                    "soundfile".into(),
                ],
            ),
            Engine::Chatterbox => (
                format!("{TORCH}resemble-perth @ {PERTH}\n"),
                vec![
                    // Perth imports pkg_resources, which newer setuptools removed.
                    "setuptools<81".into(),
                    "soundfile".into(),
                    format!("chatterbox-tts @ {CHATTERBOX}"),
                ],
            ),
        }
    }
}

/// Set by Cancel; every step checks it.
static CANCEL: AtomicBool = AtomicBool::new(false);
/// The process running now (setup, voice or transcription worker), so Cancel can stop it.
static RUNNING: AtomicU32 = AtomicU32::new(0);
const CANCELLED: &str = "Cancelled";

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

/// Runs a process, passing `progress <done> [<next>]` lines on. On failure, the error
/// is the last line it wrote to stderr.
fn run(cmd: &mut Command, on_progress: &mut dyn FnMut(f32, f32)) -> Result<(), String> {
    hidden(cmd).stdout(Stdio::piped()).stderr(Stdio::piped());
    if CANCEL.load(Ordering::SeqCst) {
        return Err(CANCELLED.into());
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    RUNNING.store(child.id(), Ordering::SeqCst);
    if CANCEL.load(Ordering::SeqCst) {
        let _ = child.kill();
    }
    let mut stderr = child.stderr.take().unwrap();
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    for line in BufReader::new(child.stdout.take().unwrap()).lines() {
        let line = line.map_err(|e| e.to_string())?;
        let Some(rest) = line.strip_prefix("progress ") else {
            continue;
        };
        let mut nums = rest.split_whitespace().map(|n| n.parse::<f32>().ok());
        if let Some(Some(done)) = nums.next() {
            let next = nums.next().flatten().unwrap_or(done);
            on_progress(done, next);
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    RUNNING.store(0, Ordering::SeqCst);
    let err = err_reader.join().unwrap_or_default();
    if CANCEL.load(Ordering::SeqCst) {
        return Err(CANCELLED.into());
    }
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

/// The Python of the engine's environment, setting it up first if needed.
fn setup(engine: Engine, status: &dyn Fn(String), progress: &Bar) -> Result<PathBuf, String> {
    let base = base().ok_or("no local app data folder")?;
    let env = base.join(engine.env_dir());
    let python = env.join("Scripts").join("python.exe");
    let marker = env.join("thrasher-engine.txt");
    if python.exists() && std::fs::read_to_string(&marker).is_ok_and(|m| m == engine.marker()) {
        return Ok(python);
    }
    let uv = find_uv().ok_or(
        "Speaking new words needs uv, a small Python installer. Install it with \
         \"winget install astral-sh.uv\", then try again.",
    )?;
    status(format!(
        "Setting up {}. First time only, several GB to download…",
        engine.name()
    ));
    progress.set(0.0, 0.3);
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    run(
        Command::new(&uv)
            .args(["venv", "--clear", "--python", "3.12"])
            .arg(&env),
        &mut |_, _| {},
    )?;
    let (overrides, packages) = engine.packages();
    let overrides_file = base.join(format!("overrides-{}.txt", engine.id()));
    std::fs::write(&overrides_file, overrides).map_err(|e| e.to_string())?;
    run(
        Command::new(&uv)
            .args(["pip", "install", "--python"])
            .arg(&python)
            .arg("--override")
            .arg(&overrides_file)
            .args([
                "--extra-index-url",
                "https://download.pytorch.org/whl/cu128",
                "--index-strategy",
                "unsafe-best-match",
            ])
            .args(&packages),
        &mut |_, _| {},
    )?;
    std::fs::write(&marker, engine.marker()).map_err(|e| e.to_string())?;
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

/// The progress bar of the overlay. Steps such as loading the model give no progress
/// of their own, so the bar creeps towards the end of the running step instead of
/// standing still.
#[derive(Clone)]
struct Bar(Arc<Mutex<BarState>>);

#[derive(Default)]
struct BarState {
    shown: f32,
    /// Where the running step started and where it will end.
    done: f32,
    next: f32,
    finished: bool,
}

impl Bar {
    fn new() -> Self {
        Bar(Arc::new(Mutex::new(BarState::default())))
    }

    fn set(&self, done: f32, next: f32) {
        let mut s = self.0.lock().unwrap();
        s.done = s.done.max(done);
        s.next = next.max(s.done);
    }

    /// Moves the shown value a little towards the end of the running step: about 95%
    /// of the way after a minute.
    fn tick(&self) -> Option<f32> {
        let mut s = self.0.lock().unwrap();
        if s.finished {
            return None;
        }
        s.shown = s.shown.max(s.done);
        // Never backwards, even when a step's end is below what is shown.
        s.shown += (s.next - s.shown).max(0.0) * 0.01;
        Some(s.shown)
    }

    fn finish(&self) {
        self.0.lock().unwrap().finished = true;
    }
}

/// Generates audio for every insert of the clip that still needs it, and records the
/// result (a file or an error) in the sidecar.
fn speak(
    library: &Library,
    name: &str,
    engine: Engine,
    model_id: &str,
    language: &str,
    status: &dyn Fn(String),
    progress: &Bar,
) -> Result<usize, String> {
    let sc_path = library.sidecar_path(name);
    let sc = Sidecar::load(&sc_path).map_err(|e| e.to_string())?;
    let jobs = voice::jobs(&sc);
    if jobs.is_empty() {
        return Ok(0);
    }
    let segments = voice::pick_reference(&sc).map_err(|e| e.to_string())?;
    let python = setup(engine, status, progress)?;
    let count: usize = jobs.iter().map(|j| j.words.len()).sum();
    status(format!(
        "{} is generating {count} new word(s). Loading the voice model takes about a minute.",
        engine.name()
    ));
    let reference = voice::write_reference(library, name, &segments).map_err(|e| e.to_string())?;
    let dir = voice::voice_dir(library, name);
    let reference_txt = dir.join("reference.txt");
    std::fs::write(&reference_txt, voice::reference_text(&sc, &segments))
        .map_err(|e| e.to_string())?;
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
            .arg(engine.id())
            .arg(&reference)
            .arg(&reference_txt)
            .arg(if language.is_empty() { "no" } else { language })
            .arg(&jobs_file)
            .arg(&dir)
            .env("HF_HOME", base.join("models"))
            .env("PYTHONIOENCODING", "utf-8"),
        &mut |d, n| progress.set(d * 0.8, n * 0.8),
    )?;
    // 2. Find the new words in them.
    status("Finding the new words in the generated speech…".into());
    progress.set(0.8, 0.97);
    let transcriber = crate::transcription::worker_path()
        .ok_or("Transcription is not part of this build, so new words cannot be placed")?;
    let mut cmd = Command::new(transcriber);
    cmd.arg(&dir)
        .arg(&gen_names[0])
        .arg(model_id)
        .arg(language)
        .args(&gen_names[1..]);
    run(&mut cmd, &mut |d, _| progress.set(0.8 + d * 0.18, 0.99))?;

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

    if CANCEL.load(Ordering::SeqCst) {
        return Err(CANCELLED.into());
    }
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

/// Stops speaking new words. The process running now is stopped with everything it
/// started, and the words that were not spoken yet are removed again.
pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
    let pid = RUNNING.load(Ordering::SeqCst);
    if pid != 0 {
        let _ = hidden(Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Removes inserts that have no audio yet, after a cancel.
fn drop_pending(library: &Library, name: &str) {
    let path = library.sidecar_path(name);
    if let Ok(mut sc) = Sidecar::load(&path) {
        sc.inserts.retain(|i| i.file.is_some() || i.error.is_some());
        let _ = sc.save(&path);
    }
}

/// Starts speaking the clip's new words on a background thread.
pub fn start(
    ui: &AppWindow,
    library: &Library,
    name: String,
    engine: Engine,
    model_id: String,
    language: String,
) {
    if ui.get_speaking() {
        return;
    }
    ui.set_speaking(true);
    ui.set_speak_progress(0.0);
    ui.set_speak_stage("Starting…".into());
    CANCEL.store(false, Ordering::SeqCst);
    let weak = ui.as_weak();
    let dir = library.dir().to_path_buf();
    let bar = Bar::new();
    {
        let (bar, weak) = (bar.clone(), weak.clone());
        std::thread::spawn(move || {
            while let Some(p) = bar.tick() {
                let _ = weak.upgrade_in_event_loop(move |ui| ui.set_speak_progress(p));
                std::thread::sleep(Duration::from_millis(200));
            }
        });
    }
    std::thread::spawn(move || {
        let w = weak.clone();
        let status = move |s: String| {
            let _ = w.upgrade_in_event_loop(move |ui| ui.set_speak_stage(s.into()));
        };
        let started = std::time::Instant::now();
        let lib = Library::open(&dir).map_err(|e| e.to_string());
        let result = lib
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|lib| speak(lib, &name, engine, &model_id, &language, &status, &bar));
        bar.finish();
        let cancelled = CANCEL.load(Ordering::SeqCst);
        if cancelled && let Ok(lib) = &lib {
            drop_pending(lib, &name);
        }
        let secs = started.elapsed().as_secs_f64();
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_speaking(false);
            match result {
                _ if cancelled => ui.set_status("Cancelled. The new words were not added".into()),
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
