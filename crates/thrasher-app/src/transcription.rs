//! Transcription runs in the `thrasher-transcribe` worker process; this module starts
//! it and reports its progress to the window. It also lists and downloads models.

use crate::AppWindow;
use slint::ComponentHandle;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use thrasher_core::library::Library;
use thrasher_core::models::{self, CATALOG};

/// The worker sits next to the app, and only exists in builds with transcription.
pub fn worker_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let worker = exe.with_file_name("thrasher-transcribe.exe");
    worker.exists().then_some(worker)
}

pub fn start(ui: &AppWindow, library: &Library, name: String, model_id: &str, language: String) {
    let Some(worker) = worker_path() else {
        ui.set_status("Transcription is not part of this build".into());
        return;
    };
    if !models::find(model_id).is_some_and(|m| m.is_downloaded()) {
        ui.set_status("Download a transcription model first".into());
        ui.set_page(3);
        return;
    }
    ui.set_transcribing(true);
    ui.set_transcribe_progress(0.0);
    let weak = ui.as_weak();
    let dir = library.dir().to_path_buf();
    let model_id = model_id.to_string();
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let result = run_worker(&worker, &dir, &name, &model_id, &language, &weak);
        let secs = started.elapsed().as_secs_f64();
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_transcribing(false);
            match result {
                Ok(()) => {
                    ui.set_status(format!("Transcribed {name} in {secs:.0} s").into());
                    // Reloads the transcript and the synced word list.
                    ui.invoke_selection_changed(ui.get_selected());
                }
                Err(e) => ui.set_status(format!("Transcription failed: {e}").into()),
            }
        });
    });
}

fn run_worker(
    worker: &Path,
    dir: &Path,
    name: &str,
    model_id: &str,
    language: &str,
    weak: &slint::Weak<AppWindow>,
) -> Result<(), String> {
    let mut cmd = Command::new(worker);
    cmd.arg(dir)
        .arg(name)
        .arg(model_id)
        .arg(language)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console window flashes up.
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut stderr = child.stderr.take().unwrap();
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let mut done = false;
    for line in BufReader::new(child.stdout.take().unwrap()).lines() {
        let line = line.map_err(|e| e.to_string())?;
        if let Some(p) = line
            .strip_prefix("progress ")
            .and_then(|p| p.parse::<f32>().ok())
        {
            let _ = weak.upgrade_in_event_loop(move |ui| ui.set_transcribe_progress(p));
        } else if line == "done" {
            done = true;
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let err = err_reader.join().unwrap_or_default();
    if status.success() && done {
        Ok(())
    } else {
        // whisper.cpp logs a lot to stderr; the last line is the useful one.
        let last = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("the worker stopped");
        Err(last.to_string())
    }
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
