// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod settings;

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
            ui.set_play_position(fmt_time(self.player.position_secs()).into());
        } else if let Some(e) = self.player.error() {
            ui.set_status(format!("Playback failed: {e}").into());
        }
        active
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let settings = Settings::load();
    let library = match Library::open(&settings.output_dir) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot open {}: {e}", settings.output_dir.display());
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
            if a.recording.is_some() {
                return;
            }
            let Some(name) = a.selected(&ui).map(|c| c.name.clone()) else {
                return;
            };
            match load_clip(&a.library, &name) {
                Ok(buf) => {
                    a.player.play(buf, 0.0);
                    ui.set_playing(true);
                    ensure();
                }
                Err(e) => ui.set_status(format!("Cannot open {name}: {e}").into()),
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

    ui.on_open_folder({
        let app = app.clone();
        move || {
            let dir = app.borrow().library.dir().to_path_buf();
            let _ = std::process::Command::new("explorer").arg(dir).spawn();
        }
    });

    ui.on_quit({
        let weak = ui.as_weak();
        move || {
            let _ = weak.unwrap().hide();
        }
    });

    ui.run()?;

    // Close files properly if the window is closed mid-recording.
    let mut a = app.borrow_mut();
    a.player.stop();
    if let Some(rec) = a.recording.take() {
        let _ = rec.stop();
    }
    Ok(())
}
