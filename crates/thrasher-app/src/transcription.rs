//! Runs Whisper on a background thread and reports back to the window.

use crate::AppWindow;
use slint::ComponentHandle;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use thrasher_core::library::Library;
use thrasher_core::models::{self, CATALOG, ModelInfo};
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
        model_id: &str,
        language: String,
    ) {
        let Some(model) = models::find(model_id).filter(|m| m.is_downloaded()) else {
            ui.set_status("Download a transcription model first (Models, top right)".into());
            ui.set_show_models(true);
            return;
        };
        ui.set_transcribing(true);
        ui.set_transcribe_progress(0.0);
        let weak = ui.as_weak();
        let loaded = self.loaded.clone();
        std::thread::Builder::new()
            .name("thrasher-transcribe".into())
            .spawn(move || {
                let result = run(&loaded, &library, &name, model, &language, &weak);
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    ui.set_transcribing(false);
                    match result {
                        Ok(secs) => {
                            ui.set_status(format!("Transcribed {name} in {secs:.0} s").into());
                            // Reloads the transcript and the synced word list.
                            ui.invoke_selection_changed(ui.get_selected());
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
    model: &ModelInfo,
    language: &str,
    weak: &slint::Weak<AppWindow>,
) -> Result<f64, String> {
    let started = std::time::Instant::now();
    let buffer = load_clip(library, name).map_err(|e| e.to_string())?;
    let mono = transcribe::to_mono(&buffer.samples, buffer.channels);
    let audio = transcribe::resample(&mono, buffer.sample_rate, transcribe::RATE);
    let duration = buffer.duration_secs();

    let mut guard = loaded
        .lock()
        .map_err(|_| "model lock poisoned".to_string())?;
    let path = model.path().ok_or("no local app data folder")?;
    if guard.as_ref().is_none_or(|(p, _)| p != &path) {
        *guard = None;
        *guard = Some((
            path.clone(),
            WhisperTranscriber::load(&path, model.heads, true)?,
        ));
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
    Ok(started.elapsed().as_secs_f64())
}

/// Fills the model list in the window.
pub fn show_models(ui: &AppWindow, active: &str) {
    let rows: Vec<crate::ModelRow> = CATALOG
        .iter()
        .map(|m| crate::ModelRow {
            name: m.name.into(),
            description: m.description.into(),
            size: format!("{:.1} GB", m.bytes as f64 / 1e9).into(),
            downloaded: m.is_downloaded(),
            active: m.id == active,
        })
        .collect();
    ui.set_models(slint::ModelRc::new(slint::VecModel::from(rows)));
}

/// Downloads one model on a background thread.
pub fn download(ui: &AppWindow, index: usize, cancel: Arc<AtomicBool>, active: String) {
    let Some(model) = CATALOG.get(index) else {
        return;
    };
    cancel.store(false, std::sync::atomic::Ordering::Relaxed);
    ui.set_downloading(index as i32);
    ui.set_download_progress(0.0);
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let w = weak.clone();
        let mut last = 0.0f32;
        let result = models::download(
            model,
            &mut |done, total| {
                let p = done as f32 / total as f32;
                if p - last >= 0.005 {
                    last = p;
                    let _ = w.upgrade_in_event_loop(move |ui| ui.set_download_progress(p));
                }
            },
            &cancel,
        );
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_downloading(-1);
            match result {
                Ok(_) => ui.set_status(format!("Downloaded {}", model.name).into()),
                Err(e) => ui.set_status(format!("{}: {e}", model.name).into()),
            }
            show_models(&ui, &active);
        });
    });
}
