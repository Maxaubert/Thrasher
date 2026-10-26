//! Transcription runs in the `thrasher-transcribe` worker process; this module starts
//! it and reports its progress to the window. It also lists and downloads models.

use crate::AppWindow;
use slint::ComponentHandle;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use thrasher_core::models::{self, CATALOG};

/// The worker sits next to the app, and only exists in builds with transcription.
pub fn worker_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let worker = exe.with_file_name("thrasher-transcribe.exe");
    worker.exists().then_some(worker)
}

/// Transcribes `names` in the clip folder `dir` in one worker run, which loads the
/// model once. The window's `transcribed` callback gets whether it worked and the
/// line to show. The caller checks that the worker and the model are there.
/// With `translate`, the result is the clips' English translation.
pub fn start(
    ui: &AppWindow,
    dir: PathBuf,
    names: Vec<String>,
    model_id: &str,
    language: String,
    translate: bool,
) {
    let Some(worker) = worker_path() else {
        ui.invoke_transcribed(false, "Transcription is not part of this build".into());
        return;
    };
    ui.set_transcribe_progress(0.0);
    let weak = ui.as_weak();
    let model_id = model_id.to_string();
    let list = names.join(", ");
    let doing = if translate {
        "translating"
    } else {
        "transcribing"
    };
    crate::log!("{doing} {list} with {model_id}");
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let result = run_worker(
            &worker, &dir, &names, &model_id, &language, translate, &weak,
        );
        let secs = started.elapsed().as_secs_f64();
        match &result {
            Ok(()) => crate::log!("{doing} {list} with {model_id} took {secs:.1} s"),
            Err(e) => crate::log!("{doing} {list} with {model_id} failed: {e}"),
        }
        let ok = result.is_ok();
        let done = if translate {
            "Translated"
        } else {
            "Transcribed"
        };
        let line = match result {
            Ok(()) if names.len() == 1 => format!("{done} {} in {secs:.0} s", names[0]),
            Ok(()) => format!("{done} {} clips in {secs:.0} s", names.len()),
            Err(e) if translate => format!("Translation failed: {e}"),
            Err(e) => format!("Transcription failed: {e}"),
        };
        let _ = weak.upgrade_in_event_loop(move |ui| ui.invoke_transcribed(ok, line.into()));
    });
}

/// True when the model can transcribe now.
pub fn model_ready(model_id: &str) -> bool {
    models::find(model_id).is_some_and(|m| m.is_downloaded())
}

fn run_worker(
    worker: &Path,
    dir: &Path,
    names: &[String],
    model_id: &str,
    language: &str,
    translate: bool,
    weak: &slint::Weak<AppWindow>,
) -> Result<(), String> {
    let (first, more) = names.split_first().ok_or("nothing to transcribe")?;
    let mut cmd = Command::new(worker);
    if translate {
        cmd.arg("--translate");
    }
    cmd.arg(dir)
        .arg(first)
        .arg(model_id)
        .arg(language)
        .args(more)
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
/// Which models the library shows: search text, chip (0 all, 1 Norwegian, 2 Swedish,
/// 3 English, 4 downloaded) and whether compressed copies are listed.
pub fn matches(m: &models::ModelInfo, query: &str, chip: i32, compressed: bool) -> bool {
    let downloaded = m.is_downloaded();
    let chip_ok = match chip {
        1 => m.language == "no",
        2 => m.language == "sv",
        3 => m.language == "en" || m.language.is_empty(),
        4 => downloaded,
        _ => true,
    };
    let words_ok = query
        .split_whitespace()
        .all(|w| m.name.to_lowercase().contains(&w.to_lowercase()));
    // A compressed copy you already have always shows.
    chip_ok && words_ok && (compressed || !m.compressed || downloaded)
}

fn fmt_size(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1e9)
    } else {
        format!("{} MB", bytes / 1_000_000)
    }
}

/// Fills the model library from the catalog and the filters set in the window.
pub fn show_models(ui: &AppWindow, active: &str) {
    let (query, chip, compressed) = (
        ui.get_model_query().to_string(),
        ui.get_model_filter(),
        ui.get_model_compressed(),
    );
    let mut rows = Vec::new();
    for group in models::Group::ALL {
        let shown: Vec<(usize, &models::ModelInfo)> = CATALOG
            .iter()
            .enumerate()
            .filter(|(_, m)| m.group == group && matches(m, &query, chip, compressed))
            .collect();
        if shown.is_empty() {
            continue;
        }
        rows.push(crate::ModelRow {
            header: true,
            title: group.title().into(),
            by: group.by().into(),
            index: -1,
            ..Default::default()
        });
        let n = shown.len();
        for (k, (i, m)) in shown.into_iter().enumerate() {
            rows.push(crate::ModelRow {
                header: false,
                title: m.name.into(),
                by: "".into(),
                index: i as i32,
                size: fmt_size(m.bytes).into(),
                quality: m.quality as i32,
                speed: m.speed as i32,
                downloaded: m.is_downloaded(),
                active: m.id == active,
                first: k == 0,
                last: k + 1 == n,
            });
        }
    }
    ui.set_models(slint::ModelRc::new(slint::VecModel::from(rows)));
    let have: Vec<&models::ModelInfo> = CATALOG.iter().filter(|m| m.is_downloaded()).collect();
    let used: u64 = have.iter().map(|m| m.bytes).sum();
    let name = models::find(active).map_or("None chosen", |m| m.name);
    ui.set_model_summary(name.into());
    ui.set_models_used(format!("{} used", fmt_size(used)).into());
    ui.set_model_count(CATALOG.len() as i32);
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
