// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod settings;
mod transcription;

use settings::Settings;
use slint::{ComponentHandle, LogicalSize, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use thrasher_core::capture::{LoopbackMode, LoopbackSource, MicSource, Source};
use thrasher_core::library::{Clip, Deleted, Library};
use thrasher_core::meter::meter_position;
use thrasher_core::naming::Template;
use thrasher_core::player::{Player, load_clip};
use thrasher_core::recorder::{RecorderOptions, Recording};
use thrasher_core::sidecar::Sidecar;
use thrasher_core::sidecar::TrackKind;
use thrasher_core::wasapi_io::{DeviceInfo, input_devices};

slint::include_modules!();

const FULL_SIZE: LogicalSize = LogicalSize::new(440.0, 640.0);
const WIDGET_SIZE: LogicalSize = LogicalSize::new(240.0, 64.0);
/// Meter range in dBFS.
const METER_FLOOR_DB: f32 = -60.0;

struct App {
    settings: Settings,
    library: Library,
    template: Template,
    devices: Vec<DeviceInfo>,
    clips: Vec<Clip>,
    recording: Option<Recording>,
    player: Player,
    undo: Vec<Deleted>,
    /// Start time of each item (word or pause) in the shown transcript.
    transcript_starts: Vec<f64>,
    /// The clip the player is playing.
    playing_name: Option<String>,
}

fn fmt_time(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

impl App {
    fn refresh_clips(&mut self, ui: &AppWindow) {
        self.clips = self.library.list().unwrap_or_default();
        let rows: Vec<ClipRow> = self
            .clips
            .iter()
            .map(|c| ClipRow {
                name: c.name.clone().into(),
                duration: fmt_time(c.duration).into(),
                has_system: c.has_system,
            })
            .collect();
        ui.set_clips(ModelRc::new(VecModel::from(rows)));
        let sel = ui.get_selected();
        if sel >= self.clips.len() as i32 {
            ui.set_selected(self.clips.len() as i32 - 1);
        }
        ui.set_can_undo(!self.undo.is_empty());
    }

    /// Shows the selected clip's transcript, if it has one: as text for editing and
    /// as a row of words for the synced player.
    fn show_transcript(&mut self, ui: &AppWindow) {
        // Also reads the `transkript` folder Max's older tooling writes to.
        let sidecar = self.selected(ui).and_then(|c| {
            [
                self.library.sidecar_path(&c.name),
                self.library
                    .dir()
                    .join("transkript")
                    .join(format!("{}.json", c.name)),
            ]
            .into_iter()
            .filter_map(|p| Sidecar::load(&p).ok())
            // Old transcripts can belong to an earlier clip with the same name.
            .find(|s| !s.words.is_empty() && (s.duration - c.duration).abs() < 0.5)
        });
        let Some(sc) = sidecar else {
            ui.set_transcript("".into());
            ui.set_words(ModelRc::default());
            self.transcript_starts.clear();
            return;
        };
        ui.set_transcript(thrasher_core::transcribe::render_text(&sc.words, &sc.silences).into());
        let mut items: Vec<(f64, WordRow)> = sc
            .words
            .iter()
            .map(|w| {
                let row = WordRow {
                    text: w.text.clone().into(),
                    is_silence: false,
                };
                (w.start, row)
            })
            .chain(sc.silences.iter().map(|s| {
                let tag = thrasher_core::edit::tag(thrasher_core::edit::kept(s));
                let row = WordRow {
                    text: tag.into(),
                    is_silence: true,
                };
                (s.start, row)
            }))
            .collect();
        items.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.transcript_starts = items.iter().map(|i| i.0).collect();
        let rows: Vec<WordRow> = items.into_iter().map(|i| i.1).collect();
        ui.set_words(ModelRc::new(VecModel::from(rows)));
        ui.set_current_word(-1);
    }

    /// Index of the transcript item being heard at `secs`.
    fn item_at(&self, secs: f64) -> i32 {
        self.transcript_starts.partition_point(|&s| s <= secs) as i32 - 1
    }

    fn play(&mut self, ui: &AppWindow, from_secs: f64) -> bool {
        let Some(name) = self.selected(ui).map(|c| c.name.clone()) else {
            return false;
        };
        match load_clip(&self.library, &name) {
            Ok(buf) => {
                self.player.play(buf, from_secs);
                self.playing_name = Some(name);
                ui.set_playing(true);
                true
            }
            Err(e) => {
                ui.set_status(format!("Cannot open {name}: {e}").into());
                false
            }
        }
    }

    fn select_name(&self, ui: &AppWindow, name: &str) {
        if let Some(i) = self.clips.iter().position(|c| c.name == name) {
            ui.set_selected(i as i32);
        }
    }

    fn selected(&self, ui: &AppWindow) -> Option<&Clip> {
        usize::try_from(ui.get_selected())
            .ok()
            .and_then(|i| self.clips.get(i))
    }

    fn start_recording(&mut self, ui: &AppWindow) {
        self.player.stop();
        let names = self.library.names().unwrap_or_default();
        let name = self.template.next_name(
            names.iter().map(String::as_str),
            chrono::Local::now().naive_local(),
        );
        let mut sources: Vec<(TrackKind, Box<dyn Source>)> = vec![(
            TrackKind::Mic,
            Box::new(MicSource::new(self.settings.mic_device.clone())),
        )];
        if self.settings.record_system {
            sources.push((
                TrackKind::System,
                Box::new(LoopbackSource {
                    mode: LoopbackMode::ExcludeSelf,
                }),
            ));
        }
        match Recording::start(&self.library, &name, sources, RecorderOptions::default()) {
            Ok(rec) => {
                self.recording = Some(rec);
                ui.set_recording(true);
                ui.set_paused(false);
                ui.set_status(format!("Recording {name}").into());
            }
            Err(e) => ui.set_status(format!("Could not start: {e}").into()),
        }
    }

    fn stop_recording(&mut self, ui: &AppWindow) {
        let Some(rec) = self.recording.take() else {
            return;
        };
        ui.set_recording(false);
        ui.set_paused(false);
        ui.set_mic_level(0.0);
        ui.set_system_level(0.0);
        match rec.stop() {
            Ok(done) => {
                let mut msg = format!("Saved {} ({})", done.name, fmt_time(done.duration));
                for w in &done.warnings {
                    msg.push_str(". ");
                    msg.push_str(w);
                }
                ui.set_status(msg.into());
                self.refresh_clips(ui);
                self.select_name(ui, &done.name);
            }
            Err(e) => ui.set_status(format!("Recording failed: {e}").into()),
        }
    }

    /// Called by the UI timer while something is happening. Returns false when idle.
    fn tick(&mut self, ui: &AppWindow) -> bool {
        let mut active = false;
        if let Some(rec) = &self.recording {
            active = true;
            ui.set_elapsed(fmt_time(rec.elapsed()).into());
            for (kind, level) in rec.levels() {
                let pos = meter_position(level.peak, METER_FLOOR_DB);
                match kind {
                    TrackKind::Mic => ui.set_mic_level(pos),
                    TrackKind::System => ui.set_system_level(pos),
                }
            }
            if let Some(e) = rec.error() {
                ui.set_status(e.into());
            }
        }
        let playing = self.player.is_playing();
        ui.set_playing(playing);
        if playing {
            active = true;
            let pos = self.player.position_secs();
            ui.set_play_position(fmt_time(pos).into());
            let shown = self.selected(ui).map(|c| c.name.as_str());
            if shown.is_some() && shown == self.playing_name.as_deref() {
                ui.set_current_word(self.item_at(pos));
            }
        } else if let Some(e) = self.player.error() {
            ui.set_status(format!("Playback failed: {e}").into());
        }
        active
    }
}

/// A 32x32 round tray icon: red while recording, light grey otherwise.
fn tray_image(recording: bool) -> slint::Image {
    let size = 32u32;
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(size, size);
    let (r, g, b) = if recording {
        (0xef, 0x44, 0x44)
    } else {
        (0xe8, 0xe8, 0xe8)
    };
    let c = (size as f32 - 1.0) / 2.0;
    for (i, px) in buf.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = ((i as u32 % size) as f32, (i as u32 / size) as f32);
        let d = ((x - c).powi(2) + (y - c).powi(2)).sqrt();
        // Ring with a filled centre, anti-aliased edges.
        let ring = (1.0 - ((d - 13.0).abs() - 1.5).clamp(0.0, 1.0)) * (d <= 15.5) as u8 as f32;
        let dot = (8.5 - d).clamp(0.0, 1.0);
        let a = ring.max(dot);
        *px = slint::Rgba8Pixel {
            r,
            g,
            b,
            a: (a * 255.0) as u8,
        };
    }
    slint::Image::from_rgba8(buf)
}

fn register_hotkey(ui: &AppWindow, spec: &str) -> Option<global_hotkey::GlobalHotKeyManager> {
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
    if spec.trim().is_empty() {
        return None;
    }
    let hotkey: HotKey = match spec.parse() {
        Ok(h) => h,
        Err(e) => {
            ui.set_status(format!("Hotkey '{spec}' not understood: {e}").into());
            return None;
        }
    };
    let manager = GlobalHotKeyManager::new().ok()?;
    if let Err(e) = manager.register(hotkey) {
        ui.set_status(format!("Hotkey {spec} is taken by another app: {e}").into());
        return None;
    }
    let weak = ui.as_weak();
    let id = hotkey.id();
    GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
        if e.id() == id && e.state() == HotKeyState::Pressed {
            let _ = weak.upgrade_in_event_loop(|ui| ui.invoke_toggle_record());
        }
    }));
    ui.set_hotkey(spec.into());
    Some(manager)
}

fn main() -> Result<(), slint::PlatformError> {
    let settings = Settings::load();
    // For testing against a scratch folder without touching the real clips. Not saved.
    let dir = std::env::var_os("THRASHER_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| settings.output_dir.clone());
    let library = match Library::open(&dir) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot open {}: {e}", dir.display());
            std::process::exit(1);
        }
    };
    let template = Template::parse(&settings.name_template)
        .unwrap_or_else(|_| Template::parse(thrasher_core::naming::DEFAULT_TEMPLATE).unwrap());
    let devices = input_devices().unwrap_or_default();

    let ui = AppWindow::new()?;
    let recovered = library.recover().unwrap_or_default();

    let app = Rc::new(RefCell::new(App {
        settings,
        library,
        template,
        devices,
        clips: Vec::new(),
        recording: None,
        player: Player::new(),
        undo: Vec::new(),
        transcript_starts: Vec::new(),
        playing_name: None,
    }));

    {
        let a = app.borrow();
        let mut names: Vec<SharedString> = vec!["Windows default".into()];
        names.extend(
            a.devices
                .iter()
                .map(|d| SharedString::from(d.name.as_str())),
        );
        ui.set_mic_devices(ModelRc::new(VecModel::from(names)));
        let idx = a
            .settings
            .mic_device
            .as_ref()
            .and_then(|id| a.devices.iter().position(|d| &d.id == id))
            .map_or(0, |i| i as i32 + 1);
        ui.set_mic_index(idx);
        ui.set_record_system(a.settings.record_system);
        ui.set_pinned(a.settings.pinned);
        ui.set_folder(a.library.dir().display().to_string().into());
        if !recovered.is_empty() {
            ui.set_status(format!("Recovered after a crash: {}", recovered.join(", ")).into());
        }
    }
    if std::env::args().any(|a| a == "--widget") {
        ui.set_compact(true);
        ui.window().set_size(WIDGET_SIZE);
    }
    app.borrow_mut().refresh_clips(&ui);
    ui.set_selected(app.borrow().clips.len() as i32 - 1);
    app.borrow_mut().show_transcript(&ui);

    // Polls levels and positions only while recording or playing, so an idle
    // window costs nothing.
    let timer = Rc::new(Timer::default());
    let ensure_timer = {
        let (app, timer, weak) = (app.clone(), timer.clone(), ui.as_weak());
        Rc::new(move || {
            if timer.running() {
                return;
            }
            let (app, weak, t2) = (app.clone(), weak.clone(), Rc::downgrade(&timer));
            timer.start(TimerMode::Repeated, Duration::from_millis(33), move || {
                let Some(ui) = weak.upgrade() else { return };
                if !app.borrow_mut().tick(&ui)
                    && let Some(t) = t2.upgrade()
                {
                    t.stop();
                }
            });
        })
    };

    // Global hotkey. The manager must stay alive for the hotkey to stay registered.
    let _hotkeys = register_hotkey(&ui, &app.borrow().settings.hotkey);

    ui.on_toggle_record({
        let (app, weak, ensure) = (app.clone(), ui.as_weak(), ensure_timer.clone());
        move || {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            if a.recording.is_some() {
                a.stop_recording(&ui);
            } else {
                a.start_recording(&ui);
                ensure();
            }
        }
    });

    ui.on_toggle_pause({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            let ui = weak.unwrap();
            if let Some(rec) = &app.borrow().recording {
                if rec.is_paused() {
                    rec.resume();
                } else {
                    rec.pause();
                }
                ui.set_paused(rec.is_paused());
            }
        }
    });

    ui.on_play_selected({
        let (app, weak, ensure) = (app.clone(), ui.as_weak(), ensure_timer.clone());
        move || {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            if a.recording.is_none() && a.play(&ui, 0.0) {
                ensure();
            }
        }
    });

    ui.on_seek_word({
        let (app, weak, ensure) = (app.clone(), ui.as_weak(), ensure_timer.clone());
        move |i| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let Some(&start) = a.transcript_starts.get(i as usize) else {
                return;
            };
            if a.recording.is_none() && a.play(&ui, start) {
                ui.set_current_word(i);
                ensure();
            }
        }
    });

    ui.on_stop_playback({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            app.borrow_mut().player.stop();
            weak.unwrap().set_playing(false);
        }
    });

    let delete = {
        let (app, weak) = (app.clone(), ui.as_weak());
        move |last: bool| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let name = if last {
                a.clips.last().map(|c| c.name.clone())
            } else {
                a.selected(&ui).map(|c| c.name.clone())
            };
            let Some(name) = name else { return };
            if a.recording.as_ref().is_some_and(|r| r.name() == name) {
                return;
            }
            a.player.stop();
            match a.library.delete(&name) {
                Ok(d) => {
                    a.undo.push(d);
                    ui.set_status(format!("Deleted {name}. Ctrl+Z to undo").into());
                    a.refresh_clips(&ui);
                }
                Err(e) => ui.set_status(e.to_string().into()),
            }
        }
    };
    ui.on_delete_selected({
        let d = delete.clone();
        move || d(false)
    });
    ui.on_delete_last(move || delete(true));

    ui.on_undo_delete({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let Some(d) = a.undo.pop() else { return };
            match a.library.undo_delete(&d) {
                Ok(()) => {
                    ui.set_status(format!("Restored {}", d.name).into());
                    a.refresh_clips(&ui);
                    a.select_name(&ui, &d.name);
                }
                Err(e) => ui.set_status(e.to_string().into()),
            }
        }
    });

    ui.on_rename_selected({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |new| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let Some(old) = a.selected(&ui).map(|c| c.name.clone()) else {
                return;
            };
            a.player.stop();
            match a.library.rename(&old, new.trim()) {
                Ok(()) => {
                    a.refresh_clips(&ui);
                    a.select_name(&ui, new.trim());
                    ui.set_status(format!("Renamed to {}", new.trim()).into());
                }
                Err(e) => ui.set_status(e.to_string().into()),
            }
        }
    });

    ui.on_mic_changed({
        let app = app.clone();
        move |idx| {
            let mut a = app.borrow_mut();
            a.settings.mic_device = usize::try_from(idx - 1)
                .ok()
                .and_then(|i| a.devices.get(i))
                .map(|d| d.id.clone());
            a.settings.save();
        }
    });

    ui.on_system_changed({
        let app = app.clone();
        move |on| {
            let mut a = app.borrow_mut();
            a.settings.record_system = on;
            a.settings.save();
        }
    });

    ui.on_pinned_changed({
        let app = app.clone();
        move |on| {
            let mut a = app.borrow_mut();
            a.settings.pinned = on;
            a.settings.save();
        }
    });

    ui.on_compact_changed({
        let weak = ui.as_weak();
        move |on| {
            let ui = weak.unwrap();
            ui.set_compact(on);
            ui.window()
                .set_size(if on { WIDGET_SIZE } else { FULL_SIZE });
        }
    });

    ui.on_selection_changed({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |_| app.borrow_mut().show_transcript(&weak.unwrap())
    });

    ui.on_apply_transcript({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |text| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let Some(name) = a.selected(&ui).map(|c| c.name.clone()) else {
                return;
            };
            let path = a.library.sidecar_path(&name);
            let Ok(mut sc) = Sidecar::load(&path) else {
                ui.set_status("Transcribe this clip first; older transcripts are read-only".into());
                return;
            };
            match thrasher_core::edit::apply_tags(&text, &mut sc.silences) {
                Ok(n) => match sc.save(&path) {
                    Ok(()) => {
                        ui.set_status(
                            format!("{n} pause(s) changed. Export to hear the result").into(),
                        );
                        ui.set_editing_transcript(false);
                        a.show_transcript(&ui);
                    }
                    Err(e) => ui.set_status(e.to_string().into()),
                },
                Err(e) => ui.set_status(e.into()),
            }
        }
    });

    const EXPORT_FORMATS: [&str; 3] = ["flac", "wav", "mp3"];
    ui.set_export_format(
        EXPORT_FORMATS
            .iter()
            .position(|f| *f == app.borrow().settings.export_format)
            .unwrap_or(0) as i32,
    );
    ui.on_export_format_changed({
        let app = app.clone();
        move |i| {
            let mut a = app.borrow_mut();
            a.settings.export_format = EXPORT_FORMATS[i.clamp(0, 2) as usize].into();
            a.settings.save();
        }
    });

    ui.set_clean_voice(app.borrow().settings.clean_voice);
    ui.on_clean_voice_changed({
        let app = app.clone();
        move |on| {
            let mut a = app.borrow_mut();
            a.settings.clean_voice = on;
            a.settings.save();
        }
    });

    ui.on_export_selected({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            let ui = weak.unwrap();
            let a = app.borrow();
            let Some(name) = a.selected(&ui).map(|c| c.name.clone()) else {
                return;
            };
            use thrasher_core::export::{ExportOptions, Format};
            let mut options = match a.settings.export_format.as_str() {
                "wav" => ExportOptions {
                    format: Format::Wav,
                    bits: 24,
                    ..Default::default()
                },
                "mp3" => ExportOptions {
                    format: Format::Mp3,
                    bits: 16,
                    ..Default::default()
                },
                _ => ExportOptions::default(),
            };
            options.clean_voice = a.settings.clean_voice;
            match thrasher_core::export::export_clip(&a.library, &name, options) {
                Ok(p) => {
                    let subs = thrasher_core::export::export_subtitles(&a.library, &name)
                        .map(|f| f.len())
                        .unwrap_or(0);
                    let extra = if subs > 0 { " with subtitles" } else { "" };
                    ui.set_status(format!("Exported {}{extra}", p.display()).into())
                }
                Err(e) => ui.set_status(format!("Export failed: {e}").into()),
            }
        }
    });

    if transcription::worker_path().is_some() {
        ui.set_can_transcribe(true);
        ui.on_transcribe_selected({
            let (app, weak) = (app.clone(), ui.as_weak());
            move || {
                let ui = weak.unwrap();
                let a = app.borrow();
                let Some(name) = a.selected(&ui).map(|c| c.name.clone()) else {
                    return;
                };
                transcription::start(
                    &ui,
                    &a.library,
                    name,
                    &a.settings.model,
                    a.settings.language.clone(),
                );
            }
        });
        transcription::show_models(&ui, &app.borrow().settings.model);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        ui.on_download_model({
            let (app, weak, cancel) = (app.clone(), ui.as_weak(), cancel.clone());
            move |i| {
                let active = app.borrow().settings.model.clone();
                transcription::download(&weak.unwrap(), i as usize, cancel.clone(), active);
            }
        });
        ui.on_cancel_download(move || cancel.store(true, std::sync::atomic::Ordering::Relaxed));
        ui.on_use_model({
            let (app, weak) = (app.clone(), ui.as_weak());
            move |i| {
                let Some(m) = thrasher_core::models::CATALOG.get(i as usize) else {
                    return;
                };
                let mut a = app.borrow_mut();
                a.settings.model = m.id.into();
                if !m.language.is_empty() {
                    a.settings.language = m.language.into();
                }
                a.settings.save();
                transcription::show_models(&weak.unwrap(), m.id);
            }
        });
    }

    ui.on_open_folder({
        let app = app.clone();
        move || {
            let dir = app.borrow().library.dir().to_path_buf();
            let _ = std::process::Command::new("explorer").arg(dir).spawn();
        }
    });

    // Tray icon. With it, closing the window hides it instead of quitting.
    let tray = Tray::new().ok();
    if let Some(tray) = &tray {
        tray.set_image(tray_image(false));
        tray.on_toggle_record({
            let weak = ui.as_weak();
            move || weak.unwrap().invoke_toggle_record()
        });
        tray.on_show_window({
            let weak = ui.as_weak();
            move || {
                let ui = weak.unwrap();
                ui.invoke_compact_changed(false);
                let _ = ui.show();
            }
        });
        tray.on_show_widget({
            let weak = ui.as_weak();
            move || {
                let ui = weak.unwrap();
                ui.invoke_compact_changed(true);
                let _ = ui.show();
            }
        });
        tray.on_quit(|| {
            let _ = slint::quit_event_loop();
        });
        // Keep the tray's recording state in step with the window.
        let (tray_weak, ui_weak) = (tray.as_weak(), ui.as_weak());
        let sync = Timer::default();
        let mut last = None;
        sync.start(TimerMode::Repeated, Duration::from_millis(250), move || {
            let (Some(tray), Some(ui)) = (tray_weak.upgrade(), ui_weak.upgrade()) else {
                return;
            };
            let rec = ui.get_recording();
            if last != Some(rec) {
                last = Some(rec);
                tray.set_recording(rec);
                tray.set_image(tray_image(rec));
            }
        });
        std::mem::forget(sync);
    }
    ui.on_quit({
        let weak = ui.as_weak();
        let has_tray = tray.is_some();
        move || {
            if has_tray {
                let _ = weak.unwrap().hide();
            } else {
                let _ = slint::quit_event_loop();
            }
        }
    });
    ui.window().on_close_requested({
        let has_tray = tray.is_some();
        move || {
            if !has_tray {
                let _ = slint::quit_event_loop();
            }
            slint::CloseRequestResponse::HideWindow
        }
    });

    ui.show()?;
    slint::run_event_loop_until_quit()?;
    drop(tray);

    // Close files properly if the window is closed mid-recording.
    let mut a = app.borrow_mut();
    a.player.stop();
    if let Some(rec) = a.recording.take() {
        let _ = rec.stop();
    }
    Ok(())
}
