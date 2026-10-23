//! Runs Whisper on a background thread and reports back to the window.

use crate::AppWindow;
use slint::ComponentHandle;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use thrasher_core::library::Library;
use thrasher_core::player::load_clip;
use thrasher_core::sidecar::Sidecar;
use thrasher_core::transcribe::{self, Options, Transcriber};
use thrasher_core::whisper::WhisperTranscriber;

/// Keeps the loaded model between runs; loading a large model takes seconds.
#[derive(Clone, Default)]
pub struct Engine {
    loaded: Arc<Mutex<Option<(PathBuf, WhisperTranscriber)>>>,
}

impl Engine {
    pub fn start(
        &self,
        ui: &AppWindow,
        library: Library,
        name: String,
        model: PathBuf,
        language: String,
    ) {
        if !model.exists() {
            ui.set_status(
                format!(
                    "No transcription model at {}. Model downloads come in a later version.",
                    model.display()
                )
                .into(),
            );
            return;
        }
        ui.set_transcribing(true);
        ui.set_transcribe_progress(0.0);
        let weak = ui.as_weak();
        let loaded = self.loaded.clone();
        std::thread::Builder::new()
            .name("thrasher-transcribe".into())
            .spawn(move || {
                let result = run(&loaded, &library, &name, &model, &language, &weak);
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    ui.set_transcribing(false);
                    match result {
                        Ok((text, secs)) => {
                            ui.set_transcript(text.into());
                            ui.set_status(format!("Transcribed {name} in {secs:.0} s").into());
                        }
                        Err(e) => ui.set_status(format!("Transcription failed: {e}").into()),
                    }
                });
            })
            .expect("spawn transcription thread");
    }
}

fn run(
    loaded: &Mutex<Option<(PathBuf, WhisperTranscriber)>>,
    library: &Library,
    name: &str,
    model: &PathBuf,
    language: &str,
    weak: &slint::Weak<AppWindow>,
) -> Result<(String, f64), String> {
    let started = std::time::Instant::now();
    let buffer = load_clip(library, name).map_err(|e| e.to_string())?;
    let mono = transcribe::to_mono(&buffer.samples, buffer.channels);
    let audio = transcribe::resample(&mono, buffer.sample_rate, transcribe::RATE);
    let duration = buffer.duration_secs();

    let mut guard = loaded
        .lock()
        .map_err(|_| "model lock poisoned".to_string())?;
    if guard.as_ref().is_none_or(|(p, _)| p != model) {
        *guard = Some((model.clone(), WhisperTranscriber::load(model, true)?));
    }
    let engine = &mut guard.as_mut().unwrap().1;
    let options = Options {
        language: (!language.is_empty()).then(|| language.to_string()),
        ..Default::default()
    };
    let w = weak.clone();
    let result = engine.transcribe(
        &audio,
        &options,
        &mut |p| {
            let _ = w.upgrade_in_event_loop(move |ui| ui.set_transcribe_progress(p));
        },
        &AtomicBool::new(false),
    )?;

    let silences = transcribe::silences(&result.words, duration, transcribe::DEFAULT_MIN_SILENCE);
    let text = transcribe::render_text(&result.words, &silences);
    let path = library.sidecar_path(name);
    let mut sc = Sidecar::load(&path).unwrap_or_else(|_| Sidecar {
        file: format!("{name}.wav"),
        duration,
        ..Default::default()
    });
    sc.language = result.language;
    sc.words = result.words;
    sc.silences = silences;
    sc.save(&path).map_err(|e| e.to_string())?;
    Ok((text, started.elapsed().as_secs_f64()))
}
