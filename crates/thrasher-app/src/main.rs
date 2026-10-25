// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod log;
mod settings;
mod startup;
mod transcription;
mod voice;
mod waves;

use settings::Settings;
use slint::{
    ComponentHandle, LogicalSize, Model, ModelRc, SharedString, Timer, TimerMode, VecModel,
};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thrasher_core::capture::{LoopbackMode, LoopbackSource, MicSource, Source};
use thrasher_core::edit::{self, Edl};
use thrasher_core::library::{Clip, Library};
use thrasher_core::meter::meter_position;
use thrasher_core::naming::Template;
use thrasher_core::player::{Buffer, Player, load_clip};
use thrasher_core::recorder::{RecorderOptions, Recording};
use thrasher_core::sidecar::Sidecar;
use thrasher_core::sidecar::TrackKind;
use thrasher_core::wasapi_io::{DeviceInfo, input_devices};

slint::include_modules!();

const FULL_SIZE: LogicalSize = LogicalSize::new(520.0, 600.0);
const WIDGET_SIZE: LogicalSize = LogicalSize::new(320.0, 64.0);
/// Meter range in dBFS.
const METER_FLOOR_DB: f32 = -60.0;
/// Bars in the live recording waveform: about 2.6 s at the 33 ms tick.
const LIVE_BARS: usize = 80;
/// Columns of the small waveform in the clip list.
const ROW_COLUMNS: usize = 30;
const SPEEDS: [f64; 5] = [0.75, 1.0, 1.25, 1.5, 2.0];
const NORMAL_SPEED: usize = 1;
/// Transcription languages offered in settings: code and name.
const LANGUAGES: [(&str, &str); 21] = [
    ("no", "Norwegian"),
    ("en", "English"),
    ("sv", "Swedish"),
    ("da", "Danish"),
    ("fi", "Finnish"),
    ("de", "German"),
    ("nl", "Dutch"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("it", "Italian"),
    ("pt", "Portuguese"),
    ("pl", "Polish"),
    ("uk", "Ukrainian"),
    ("ru", "Russian"),
    ("tr", "Turkish"),
    ("ar", "Arabic"),
    ("hi", "Hindi"),
    ("zh", "Chinese"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("", "Detect the language"),
];
const EXPORT_FORMATS: [&str; 3] = ["flac", "wav", "mp3"];

/// The editor text as markup for the coloured layer behind it: pause tags in orange,
/// everything else escaped so it shows exactly as typed.
fn edit_markup(text: &str) -> String {
    fn escape(out: &mut String, s: &str) {
        for c in s.chars() {
            match c {
                '\n' => out.push_str("\\\n"),
                c if c.is_ascii_punctuation() => {
                    out.push('\\');
                    out.push(c);
                }
                c => out.push(c),
            }
        }
    }
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("[silence:") {
        escape(&mut out, &rest[..i]);
        let end = rest[i..].find(']').map_or(rest.len(), |e| i + e + 1);
        out.push_str("<font color=\"#ff6a1a\">");
        escape(&mut out, &rest[i..end]);
        out.push_str("</font>");
        rest = &rest[end..];
    }
    escape(&mut out, rest);
    out
}

/// What the export menu offers, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Export {
    Audio(thrasher_core::export::Format),
    Subtitles(thrasher_core::export::Subtitles),
    /// A folder with the audio in the default format and every subtitle file.
    Project(thrasher_core::export::Format),
}

impl Export {
    const ALL: [Export; 8] = {
        use thrasher_core::export::{Format, Subtitles};
        [
            Export::Audio(Format::Flac),
            Export::Audio(Format::Wav),
            Export::Audio(Format::Mp3),
            Export::Subtitles(Subtitles::Srt),
            Export::Subtitles(Subtitles::Vtt),
            Export::Subtitles(Subtitles::Words),
            Export::Subtitles(Subtitles::Script),
            Export::Project(Format::Flac),
        ]
    };

    fn title(self) -> &'static str {
        use thrasher_core::export::{Format, Subtitles};
        match self {
            Export::Audio(Format::Flac) => "FLAC audio",
            Export::Audio(Format::Wav) => "WAV audio",
            Export::Audio(Format::Mp3) => "MP3 audio",
            Export::Subtitles(Subtitles::Srt) => "SRT subtitles",
            Export::Subtitles(Subtitles::Vtt) => "VTT subtitles",
            Export::Subtitles(Subtitles::Words) => "word timings",
            Export::Subtitles(Subtitles::Script) => "timestamped script",
            Export::Project(_) => "whole project",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Export::Audio(f) => f.extension(),
            Export::Subtitles(s) => s.extension(),
            Export::Project(_) => "",
        }
    }

    /// The last part of the extension, which is what the save dialog filters on.
    fn filter(self) -> &'static str {
        self.extension().rsplit('.').next().unwrap_or("")
    }

    /// Writes the export. False means a subtitle export of a clip with no transcript.
    fn write(
        self,
        library: &Library,
        name: &str,
        clean_voice: bool,
        path: &std::path::Path,
    ) -> std::io::Result<bool> {
        use thrasher_core::export::{ExportOptions, Format};
        let options = |format| ExportOptions {
            format,
            // MP3 is always 16 bit; WAV and FLAC keep 24.
            bits: if format == Format::Mp3 { 16 } else { 24 },
            clean_voice,
        };
        match self {
            Export::Audio(format) => {
                thrasher_core::export::export_clip_to(library, name, options(format), path)?;
                Ok(true)
            }
            Export::Project(format) => {
                thrasher_core::export::export_project(library, name, options(format), path)?;
                Ok(true)
            }
            Export::Subtitles(kind) => {
                thrasher_core::export::export_subtitles_to(library, name, kind, path)
            }
        }
    }
}

struct App {
    settings: Settings,
    library: Library,
    template: Template,
    devices: Vec<DeviceInfo>,
    clips: Vec<Clip>,
    recording: Option<Recording>,
    player: Player,
    /// Start time of each item (word or pause) in the shown transcript.
    transcript_starts: Vec<f64>,
    /// The clip in the player, playing or paused.
    active: Option<String>,
    /// The player is running (not paused, not finished).
    running: bool,
    /// Where the active clip stands while paused, in recording time.
    paused_at: f64,
    /// The clip last read for playback, kept so replaying, seeking and changing
    /// speed start at once; and the one being read now.
    loaded: Option<Loaded>,
    loading: Option<Loading>,
    speed: usize,
    /// Maps the edited audio being played back to times in the recording.
    timeline: Edl,
    waves: waves::Waves,
    live: VecDeque<f32>,
    live_model: Rc<VecModel<f32>>,
}

/// A clip as edited, ready to play.
struct Loaded {
    name: String,
    stamp: Stamp,
    buffer: Arc<Buffer>,
    timeline: Edl,
}

struct Loading {
    name: String,
    from: f64,
    rx: std::sync::mpsc::Receiver<Result<Loaded, String>>,
    asked: std::time::Instant,
}

/// Changes when a clip's audio or edits change, so a stale copy is read again.
type Stamp = Vec<Option<(u64, SystemTime)>>;

fn clip_stamp(library: &Library, name: &str) -> Stamp {
    [
        library.track_path(name, TrackKind::Mic),
        library.track_path(name, TrackKind::System),
        library.sidecar_path(name),
    ]
    .iter()
    .map(|p| {
        let m = std::fs::metadata(p).ok()?;
        Some((m.len(), m.modified().ok()?))
    })
    .collect()
}

/// Reads a clip and applies its edits: cut words and shortened pauses are left out,
/// and inserted words are spoken.
fn load_for_playback(library: &Library, name: &str) -> Result<(Buffer, Edl), String> {
    let t = std::time::Instant::now();
    let buffer = load_clip(library, name).map_err(|e| e.to_string())?;
    crate::log!(
        "read {name}: {} of audio, {} MB in memory, {} ms",
        fmt_time(buffer.duration_secs()),
        buffer.samples.len() * 4 / 1_000_000,
        log::ms(t)
    );
    let t = std::time::Instant::now();
    let timeline = Sidecar::load(&library.sidecar_path(name))
        .map(|sc| {
            let voices = thrasher_core::voice::load_voices(library, name, &sc);
            edit::plan(&sc, &voices, &buffer)
        })
        .unwrap_or_default();
    let buffer = if timeline.is_identity() {
        buffer
    } else {
        timeline.render(&buffer)
    };
    crate::log!("applied the edits of {name} in {} ms", log::ms(t));
    Ok((buffer, timeline))
}

fn fmt_time(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// The recording timer as "02:47" and ".3".
fn fmt_elapsed(secs: f64) -> (String, String) {
    let tenths = (secs.max(0.0) * 10.0) as u64;
    let s = tenths / 10;
    let big = if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    };
    (big, format!(".{}", tenths % 10))
}

/// "Today 09:14", "Yesterday 15:30", "Tue 11:02", or "12 Oct" for older clips.
fn when(t: SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Local> = t.into();
    let days = (chrono::Local::now().date_naive() - dt.date_naive()).num_days();
    let hm = dt.format("%H:%M");
    match days {
        0 => format!("Today {hm}"),
        1 => format!("Yesterday {hm}"),
        2..=6 => format!("{} {hm}", dt.format("%a")),
        _ => dt.format("%-d %b %Y").to_string(),
    }
}

/// A folder as "…\Music\VoiceClips" when the full path is long.
fn short_path(p: &std::path::Path) -> String {
    let full = p.display().to_string();
    if full.chars().count() <= 28 {
        return full;
    }
    let tail: Vec<String> = p
        .components()
        .rev()
        .take(2)
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    format!(
        "…\\{}",
        tail.into_iter().rev().collect::<Vec<_>>().join("\\")
    )
}

/// Length in seconds of each inserted word's audio, by insert id.
fn voice_lengths(library: &Library, name: &str, sc: &Sidecar) -> HashMap<String, f64> {
    let rate = sc.thrasher.tracks.first().map_or(48_000, |t| t.sample_rate) as f64;
    thrasher_core::voice::load_voices(library, name, sc)
        .into_iter()
        .map(|(id, v)| (id, v.len() as f64 / rate))
        .collect()
}

/// A clip's length as edited: cut words and pauses, changed pauses and spoken words
/// count, so the list shows what playback and export give.
fn edited_secs(library: &Library, c: &Clip, sc: &Sidecar) -> f64 {
    let edited = sc.words.iter().any(|w| w.cut)
        || sc.silences.iter().any(|s| s.keep.is_some())
        || !sc.inserts.is_empty();
    if !edited || (sc.duration - c.duration).abs() >= 0.5 {
        return c.duration;
    }
    edit::edited_length(sc, &voice_lengths(library, &c.name, sc))
}

impl App {
    fn refresh_clips(&mut self, ui: &AppWindow) {
        self.clips = self.library.list().unwrap_or_default();
        let mut total = 0.0;
        let rows: Vec<ClipRow> = self
            .clips
            .iter()
            .map(|c| {
                let sc = Sidecar::load(&self.library.sidecar_path(&c.name)).ok();
                let secs = sc
                    .as_ref()
                    .map_or(c.duration, |sc| edited_secs(&self.library, c, sc));
                total += secs;
                let mut meta = when(c.created);
                if sc.as_ref().is_some_and(|s| !s.words.is_empty()) {
                    meta.push_str(" · transcribed");
                }
                if c.has_system {
                    meta.push_str(" · + system");
                }
                ClipRow {
                    name: c.name.clone().into(),
                    duration: fmt_time(secs).into(),
                    meta: meta.into(),
                    has_system: c.has_system,
                    transcribed: sc.as_ref().is_some_and(|s| !s.words.is_empty()),
                    wave: self.row_wave(c),
                }
            })
            .collect();
        ui.set_clips(ModelRc::new(VecModel::from(rows)));
        let n = self.clips.len();
        ui.set_clips_summary(
            match n {
                0 => "No clips yet".to_string(),
                _ if total < 60.0 => {
                    format!("{n} clip{} · {total:.0} s", if n == 1 { "" } else { "s" })
                }
                _ => format!(
                    "{n} clip{} · {:.0} min",
                    if n == 1 { "" } else { "s" },
                    total / 60.0
                ),
            }
            .into(),
        );
        let sel = ui.get_selected();
        if sel >= n as i32 {
            ui.set_selected(n as i32 - 1);
        }
        self.sync_active(ui);
        let (library, clips) = (self.library.clone(), self.clips.clone());
        self.waves.request(ui, &library, &clips);
    }

    fn row_wave(&self, c: &Clip) -> ModelRc<f32> {
        let w = self.waves.get(c).map_or_else(
            || vec![0.06; ROW_COLUMNS],
            |w| waves::columns(&w, ROW_COLUMNS),
        );
        ModelRc::new(VecModel::from(w))
    }

    /// Puts newly drawn waveforms into the list and the clip page.
    fn apply_waves(&self, ui: &AppWindow) {
        let rows = ui.get_clips();
        for (i, c) in self.clips.iter().enumerate() {
            if let (Some(mut row), Some(w)) = (rows.row_data(i), self.waves.get(c))
                && row.wave.row_count() > 0
                && row.wave.row_data(0) == Some(0.06)
            {
                row.wave = ModelRc::new(VecModel::from(waves::columns(&w, ROW_COLUMNS)));
                rows.set_row_data(i, row);
            }
        }
        self.show_detail_wave(ui);
    }

    fn show_detail_wave(&self, ui: &AppWindow) {
        let w = self
            .selected(ui)
            .and_then(|c| self.waves.get(c))
            .unwrap_or_else(|| vec![0.06; waves::COLUMNS]);
        ui.set_detail_wave(ModelRc::new(VecModel::from(w)));
    }

    fn device_label(&self) -> String {
        let mic = self
            .settings
            .mic_device
            .as_ref()
            .and_then(|id| self.devices.iter().find(|d| &d.id == id))
            .map_or("Default microphone", |d| d.name.as_str());
        if self.settings.record_system {
            format!("{mic} + system audio")
        } else {
            mic.to_string()
        }
    }

    /// Shows the selected clip on the clip page: details, waveform and transcript.
    fn show_transcript(&mut self, ui: &AppWindow) {
        self.update_length(ui);
        if let Some(c) = self.selected(ui) {
            // The same length as the list shows.
            let shown = ui
                .get_clips()
                .row_data(ui.get_selected() as usize)
                .map_or_else(|| fmt_time(c.duration), |r| r.duration.to_string());
            let tracks = if c.has_system { "mic + system" } else { "mic" };
            let file = c
                .path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            ui.set_detail_meta(format!("{} · {shown} · {tracks} · {file}", when(c.created)).into());
        }
        self.show_detail_wave(ui);
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
        ui.set_transcript(edit::text(&sc).into());
        let items = edit::visible(&sc);
        self.transcript_starts = items.iter().map(|i| i.start).collect();
        let rows: Vec<WordRow> = items
            .into_iter()
            .map(|i| WordRow {
                text: i.text.into(),
                is_silence: i.kind == edit::Kind::Pause,
                spoken: matches!(i.kind, edit::Kind::Spoken { .. }),
                pending: matches!(
                    i.kind,
                    edit::Kind::Spoken {
                        ready: false,
                        failed: false
                    }
                ),
                failed: matches!(i.kind, edit::Kind::Spoken { failed: true, .. }),
            })
            .collect();
        ui.set_words(ModelRc::new(VecModel::from(rows)));
        ui.set_current_word(-1);
        // Typed words still waiting (the app was closed while speaking them) are
        // spoken now.
        if !ui.get_speaking()
            && let Some(name) = self.selected(ui).map(|c| c.name.clone())
            && Sidecar::load(&self.library.sidecar_path(&name))
                .is_ok_and(|own| !thrasher_core::voice::jobs(&own).is_empty())
        {
            voice::start(
                ui,
                &self.library,
                name,
                self.settings.model.clone(),
                self.settings.language.clone(),
                false,
            );
        }
    }

    /// Shows the selected clip's edited length in the list, after an edit.
    fn update_length(&self, ui: &AppWindow) {
        let Some(c) = self.selected(ui) else { return };
        let Ok(sc) = Sidecar::load(&self.library.sidecar_path(&c.name)) else {
            return;
        };
        let i = ui.get_selected() as usize;
        let clips = ui.get_clips();
        if let Some(mut row) = clips.row_data(i) {
            let shown: SharedString = fmt_time(edited_secs(&self.library, c, &sc)).into();
            if row.duration != shown {
                row.duration = shown;
                clips.set_row_data(i, row);
            }
        }
    }

    /// Counts the changes in the transcript being edited and the length it gives.
    fn preview_edit(&self, ui: &AppWindow, text: &str) {
        ui.set_edit_badge("".into());
        let Some(c) = self.selected(ui) else { return };
        let Ok(mut sc) = Sidecar::load(&self.library.sidecar_path(&c.name)) else {
            ui.set_edit_was(fmt_time(c.duration).into());
            ui.set_edit_length(fmt_time(c.duration).into());
            return;
        };
        let was = edited_secs(&self.library, c, &sc);
        ui.set_edit_was(fmt_time(was).into());
        ui.set_edit_length(fmt_time(was).into());
        let Ok(ch) = edit::apply_text(text, &mut sc) else {
            return;
        };
        let n = ch.words_cut
            + ch.words_restored
            + ch.words_retyped
            + ch.pauses
            + ch.spoken
            + ch.unspoken;
        if n == 0 {
            return;
        }
        ui.set_edit_badge(format!("{n} change{}", if n == 1 { "" } else { "s" }).into());
        // New words have no audio yet: count a typical spoken word for each.
        let voices = voice_lengths(&self.library, &c.name, &sc);
        let missing = sc
            .inserts
            .iter()
            .filter(|i| !voices.contains_key(&i.id))
            .count();
        let len = edit::edited_length(&sc, &voices) + missing as f64 * 0.35;
        ui.set_edit_length(fmt_time(len).into());
    }

    /// Index of the transcript item being heard at `secs`.
    fn item_at(&self, secs: f64) -> i32 {
        self.transcript_starts.partition_point(|&s| s <= secs) as i32 - 1
    }

    fn clip_named(&self, name: &str) -> Option<&Clip> {
        self.clips.iter().find(|c| c.name == name)
    }

    fn sync_active(&self, ui: &AppWindow) {
        let i = self
            .active
            .as_ref()
            .and_then(|n| self.clips.iter().position(|c| &c.name == n))
            .map_or(-1, |i| i as i32);
        ui.set_active_clip(i);
    }

    /// Plays clip `name` as edited, from `from` seconds of the recording, at the
    /// chosen speed. A clip not in memory yet is read on a background thread first,
    /// so the window never waits on a long one.
    fn play_clip(&mut self, ui: &AppWindow, name: &str, from: f64) -> bool {
        self.player.stop();
        self.running = false;
        self.active = Some(name.to_string());
        self.paused_at = from;
        ui.set_playing(true);
        self.sync_active(ui);
        let stamp = clip_stamp(&self.library, name);
        if let Some(l) = self
            .loaded
            .as_ref()
            .filter(|l| l.name == name && l.stamp == stamp)
        {
            let buffer = l.buffer.clone();
            self.timeline = l.timeline.clone();
            self.start(buffer, from);
            crate::log!(
                "play {name} from {} at {}x (already in memory)",
                fmt_time(from),
                SPEEDS[self.speed]
            );
        } else if let Some(l) = self.loading.as_mut().filter(|l| l.name == name) {
            l.from = from;
        } else {
            crate::log!("play {name} from {}: reading it", fmt_time(from));
            self.loaded = None;
            self.timeline = Edl::default();
            let (tx, rx) = std::sync::mpsc::channel();
            let (library, n) = (self.library.clone(), name.to_string());
            let _ = std::thread::Builder::new()
                .name("thrasher-load".into())
                .spawn(move || {
                    let loaded = load_for_playback(&library, &n).map(|(buffer, timeline)| Loaded {
                        name: n,
                        stamp,
                        buffer: Arc::new(buffer),
                        timeline,
                    });
                    let _ = tx.send(loaded);
                });
            self.loading = Some(Loading {
                name: name.to_string(),
                from,
                rx,
                asked: std::time::Instant::now(),
            });
        }
        self.show_position(ui);
        true
    }

    fn start(&mut self, buffer: Arc<Buffer>, from: f64) {
        let start = self
            .timeline
            .to_edited(from)
            .clamp(0.0, buffer.duration_secs());
        self.player.play(buffer, start, SPEEDS[self.speed]);
        self.running = true;
    }

    /// Picks up a clip the background thread has finished reading.
    fn poll_loading(&mut self, ui: &AppWindow) {
        let Some(l) = &self.loading else { return };
        let result = match l.rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(_) => Err("the clip could not be read".into()),
        };
        let Some(Loading {
            name, from, asked, ..
        }) = self.loading.take()
        else {
            return;
        };
        match result {
            Ok(loaded) => {
                let buffer = loaded.buffer.clone();
                self.timeline = loaded.timeline.clone();
                self.loaded = Some(loaded);
                if self.active.as_deref() == Some(name.as_str()) {
                    self.start(buffer, from);
                    crate::log!(
                        "play {name}: sound started {} ms after the click",
                        log::ms(asked)
                    );
                }
            }
            Err(e) => {
                crate::log!("play {name} failed: {e}");
                ui.set_status(format!("Cannot open {name}: {e}").into());
                self.stop(ui);
            }
        }
    }

    fn edited_pos(&self) -> f64 {
        self.player.position_secs()
    }

    /// Where the active clip stands, in recording time.
    fn position(&self) -> f64 {
        if self.running {
            self.timeline.to_original(self.edited_pos())
        } else {
            self.paused_at
        }
    }

    fn pause(&mut self, ui: &AppWindow) {
        if self.loading.take().is_some() {
            ui.set_playing(false);
            return;
        }
        if !self.running {
            return;
        }
        self.paused_at = self.position();
        self.player.stop();
        self.running = false;
        ui.set_playing(false);
    }

    /// Stops playback and unloads the clip.
    fn stop(&mut self, ui: &AppWindow) {
        self.player.stop();
        self.loading = None;
        self.running = false;
        self.active = None;
        self.paused_at = 0.0;
        ui.set_playing(false);
        ui.set_play_fraction(0.0);
        ui.set_current_word(-1);
        self.sync_active(ui);
    }

    /// Play or pause `name`: a paused clip continues where it stopped.
    fn toggle(&mut self, ui: &AppWindow, name: &str) -> bool {
        if self.active.as_deref() != Some(name) {
            return self.play_clip(ui, name, 0.0);
        }
        if self.running || self.loading.is_some() {
            self.pause(ui);
            return false;
        }
        let len = self.clip_named(name).map_or(0.0, |c| c.duration);
        let from = if self.paused_at >= len - 0.05 {
            0.0
        } else {
            self.paused_at
        };
        self.play_clip(ui, name, from)
    }

    /// Moves the selected clip `secs` forward or back, in the edited audio.
    fn skip(&mut self, ui: &AppWindow, secs: f64) -> bool {
        let Some(c) = self.selected(ui).cloned() else {
            return false;
        };
        if self.active.as_deref() != Some(c.name.as_str()) {
            return self.play_clip(ui, &c.name, secs.max(0.0));
        }
        let edited = self.timeline.to_edited(self.position()) + secs;
        let to = self
            .timeline
            .to_original(edited.max(0.0))
            .clamp(0.0, c.duration);
        if self.running || self.loading.is_some() {
            return self.play_clip(ui, &c.name, to);
        }
        self.paused_at = to;
        self.show_position(ui);
        false
    }

    fn show_position(&self, ui: &AppWindow) {
        let Some(c) = self.active.as_deref().and_then(|n| self.clip_named(n)) else {
            return;
        };
        let orig = self.position();
        ui.set_play_fraction((orig / c.duration.max(1e-3)).clamp(0.0, 1.0) as f32);
        let edited = if self.running {
            self.edited_pos()
        } else {
            self.timeline.to_edited(orig)
        };
        ui.set_play_position(fmt_time(edited).into());
        if self.selected(ui).is_some_and(|s| s.name == c.name) {
            ui.set_current_word(self.item_at(orig));
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
        self.stop(ui);
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
                crate::log!(
                    "recording {name} (system audio {})",
                    if self.settings.record_system {
                        "on"
                    } else {
                        "off"
                    }
                );
                self.recording = Some(rec);
                self.live.clear();
                ui.set_recording(true);
                ui.set_paused(false);
                ui.set_status(format!("Recording {name}").into());
            }
            Err(e) => {
                crate::log!("recording {name} could not start: {e}");
                ui.set_status(format!("Could not start: {e}").into());
            }
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
        let (big, tenth) = fmt_elapsed(0.0);
        ui.set_elapsed(big.into());
        ui.set_elapsed_tenth(tenth.into());
        self.live.clear();
        self.live_model.set_vec(vec![0.06; LIVE_BARS]);
        match rec.stop() {
            Ok(done) => {
                crate::log!("saved {} ({})", done.name, fmt_time(done.duration));
                let mut msg = format!("Saved {} ({})", done.name, fmt_time(done.duration));
                for w in &done.warnings {
                    msg.push_str(". ");
                    msg.push_str(w);
                }
                ui.set_status(msg.into());
                self.refresh_clips(ui);
                // Nothing highlighted, so Space records again.
                ui.set_selected(-1);
            }
            Err(e) => ui.set_status(format!("Recording failed: {e}").into()),
        }
    }

    /// Called by the UI timer while something is happening. Returns false when idle.
    fn tick(&mut self, ui: &AppWindow) -> bool {
        let mut active = false;
        if let Some(rec) = &self.recording {
            active = true;
            let (big, tenth) = fmt_elapsed(rec.elapsed());
            ui.set_elapsed(big.into());
            ui.set_elapsed_tenth(tenth.into());
            let mut loudest = 0f32;
            for (kind, level) in rec.levels() {
                let pos = meter_position(level.peak, METER_FLOOR_DB);
                loudest = loudest.max(pos);
                match kind {
                    TrackKind::Mic => ui.set_mic_level(pos),
                    TrackKind::System => ui.set_system_level(pos),
                }
            }
            if rec.is_paused() {
                loudest = 0.0;
            }
            if self.live.len() >= LIVE_BARS {
                self.live.pop_front();
            }
            self.live.push_back(loudest.max(0.06));
            let mut bars = vec![0.06; LIVE_BARS - self.live.len()];
            bars.extend(self.live.iter().copied());
            self.live_model.set_vec(bars);
            if let Some(e) = rec.error() {
                ui.set_status(e.into());
            }
            let limit = self.settings.max_recording_minutes;
            if settings::over_limit(rec.elapsed(), limit) {
                self.stop_recording(ui);
                let label = settings::RECORDING_LIMITS
                    .iter()
                    .find(|(m, _)| *m == limit)
                    .map_or_else(|| format!("{limit} minutes"), |(_, l)| l.to_string());
                ui.set_status(
                    format!("Stopped and saved at the {label} limit. Change it in Settings.")
                        .into(),
                );
            }
        }
        if self.loading.is_some() {
            active = true;
            self.poll_loading(ui);
        }
        if self.running {
            active = true;
            if self.player.is_playing() {
                self.show_position(ui);
            } else if let Some(e) = self.player.error() {
                ui.set_status(format!("Playback failed: {e}").into());
                self.stop(ui);
            } else if ui.get_looping()
                && let Some(name) = self.active.clone()
            {
                self.play_clip(ui, &name, 0.0);
            } else {
                self.stop(ui);
            }
        }
        active
    }
}

/// A round icon: ring with a dot. Orange while recording, light otherwise.
fn dot_image(size: u32, recording: bool) -> slint::Image {
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(size, size);
    let (r, g, b) = if recording {
        (0xff, 0x6a, 0x1a)
    } else {
        (0xf1, 0xec, 0xe8)
    };
    let scale = size as f32 / 32.0;
    let c = (size as f32 - 1.0) / 2.0;
    for (i, px) in buf.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = ((i as u32 % size) as f32, (i as u32 / size) as f32);
        let d = ((x - c).powi(2) + (y - c).powi(2)).sqrt() / scale;
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

/// The global record hotkey. The manager must stay alive for it to stay registered.
#[derive(Default)]
struct Hotkey {
    manager: Option<global_hotkey::GlobalHotKeyManager>,
    current: Option<global_hotkey::hotkey::HotKey>,
}

impl Hotkey {
    /// Registers `spec` ("Ctrl+Alt+R"), replacing the previous hotkey. Empty turns
    /// it off. Returns false, with a status message, if it cannot be used.
    fn set(&mut self, ui: &AppWindow, spec: &str) -> bool {
        use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
        let hotkey: Option<HotKey> = if spec.trim().is_empty() {
            None
        } else {
            match spec.parse() {
                Ok(h) => Some(h),
                Err(e) => {
                    ui.set_status(format!("Hotkey '{spec}' not understood: {e}").into());
                    return false;
                }
            }
        };
        if self.manager.is_none() {
            self.manager = GlobalHotKeyManager::new().ok();
        }
        let Some(manager) = &self.manager else {
            return false;
        };
        if let Some(old) = self.current.take() {
            let _ = manager.unregister(old);
        }
        let Some(hotkey) = hotkey else {
            return true;
        };
        if let Err(e) = manager.register(hotkey) {
            ui.set_status(format!("Hotkey {spec} is taken by another app: {e}").into());
            return false;
        }
        self.current = Some(hotkey);
        let weak = ui.as_weak();
        let id = hotkey.id();
        GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
            if e.id() == id && e.state() == HotKeyState::Pressed {
                let _ = weak.upgrade_in_event_loop(|ui| ui.invoke_toggle_record());
            }
        }));
        true
    }
}

fn main() -> Result<(), slint::PlatformError> {
    log::init();
    crate::log!("starting Thrasher {}", env!("CARGO_PKG_VERSION"));
    test_window();
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
    let t = std::time::Instant::now();
    let devices = input_devices().unwrap_or_default();
    crate::log!("found {} microphones in {} ms", devices.len(), log::ms(t));

    let t = std::time::Instant::now();
    let ui = AppWindow::new()?;
    crate::log!("window built in {} ms", log::ms(t));
    let recovered = library.recover().unwrap_or_default();
    let live_model = Rc::new(VecModel::from(vec![0.06; LIVE_BARS]));
    ui.set_live(ModelRc::from(live_model.clone()));
    ui.set_window_icon(dot_image(64, true));

    let app = Rc::new(RefCell::new(App {
        settings,
        library,
        template,
        devices,
        clips: Vec::new(),
        recording: None,
        player: Player::new(),
        transcript_starts: Vec::new(),
        active: None,
        running: false,
        paused_at: 0.0,
        loaded: None,
        loading: None,
        speed: NORMAL_SPEED,
        timeline: Edl::default(),
        waves: waves::Waves::default(),
        live: VecDeque::new(),
        live_model,
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
        ui.set_device_label(a.device_label().into());
        ui.set_pinned(a.settings.pinned);
        ui.set_folder(short_path(a.library.dir()).into());
        ui.set_name_template(a.settings.name_template.clone().into());
        ui.set_close_to_tray(a.settings.close_to_tray);
        let limits: Vec<SharedString> = settings::RECORDING_LIMITS
            .iter()
            .map(|(_, l)| (*l).into())
            .collect();
        ui.set_recording_limits(ModelRc::new(VecModel::from(limits)));
        ui.set_recording_limit_index(
            settings::RECORDING_LIMITS
                .iter()
                .position(|(m, _)| *m == a.settings.max_recording_minutes)
                .unwrap_or(2) as i32,
        );
        ui.set_start_with_windows(startup::enabled());
        ui.set_speed_index(NORMAL_SPEED as i32);
        let langs: Vec<SharedString> = LANGUAGES.iter().map(|(_, n)| (*n).into()).collect();
        ui.set_languages(ModelRc::new(VecModel::from(langs)));
        ui.set_language_index(
            LANGUAGES
                .iter()
                .position(|(c, _)| *c == a.settings.language)
                .unwrap_or(0) as i32,
        );
        if !recovered.is_empty() {
            ui.set_status(format!("Recovered after a crash: {}", recovered.join(", ")).into());
        }
    }
    if std::env::args().any(|a| a == "--widget") {
        ui.set_compact(true);
        ui.window().set_size(WIDGET_SIZE);
    }
    let t = std::time::Instant::now();
    app.borrow_mut().refresh_clips(&ui);
    // Nothing highlighted at start: Space records until a clip is clicked.
    ui.set_selected(-1);
    app.borrow_mut().show_transcript(&ui);
    crate::log!(
        "listed {} clips in {} ms",
        app.borrow().clips.len(),
        log::ms(t)
    );

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

    let hotkey = Rc::new(RefCell::new(Hotkey::default()));
    {
        let spec = app.borrow().settings.hotkey.clone();
        if hotkey.borrow_mut().set(&ui, &spec) {
            show_hotkey(&ui, &spec);
        }
    }
    ui.on_hotkey_accepted({
        let (app, weak, hotkey) = (app.clone(), ui.as_weak(), hotkey.clone());
        move |spec| {
            let ui = weak.unwrap();
            let spec = spec.trim().to_string();
            if hotkey.borrow_mut().set(&ui, &spec) {
                let mut a = app.borrow_mut();
                a.settings.hotkey = spec.clone();
                a.settings.save();
                show_hotkey(&ui, &spec);
                ui.set_status(if spec.is_empty() {
                    "Hotkey turned off".into()
                } else {
                    format!("{spec} now records from any app").into()
                });
            }
        }
    });

    ui.on_hotkey_captured({
        let weak = ui.as_weak();
        move |text, ctrl, alt, shift, meta| {
            let ui = weak.unwrap();
            match hotkey_spec(&text, ctrl, alt, shift, meta) {
                Spec::Wait => false,
                Spec::NeedsModifier => {
                    ui.set_status("Hold Ctrl, Alt or Win with the key".into());
                    false
                }
                Spec::Done(spec) => {
                    ui.invoke_hotkey_accepted(spec.into());
                    true
                }
            }
        }
    });

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

    ui.on_play_clip({
        let (app, weak, ensure) = (app.clone(), ui.as_weak(), ensure_timer.clone());
        move |i| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let Some(name) = a.clips.get(i as usize).map(|c| c.name.clone()) else {
                return;
            };
            if ui.get_selected() != i {
                ui.set_selected(i);
            }
            if a.toggle(&ui, &name) {
                ensure();
            }
        }
    });

    ui.on_open_clip({
        let weak = ui.as_weak();
        move |i| {
            let ui = weak.unwrap();
            if ui.get_selected() != i {
                ui.set_selected(i);
            }
        }
    });

    ui.on_toggle_play({
        let (app, weak, ensure) = (app.clone(), ui.as_weak(), ensure_timer.clone());
        move || {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let Some(name) = a.selected(&ui).map(|c| c.name.clone()) else {
                return;
            };
            if a.toggle(&ui, &name) {
                ensure();
            }
        }
    });

    ui.on_seek_fraction({
        let (app, weak, ensure) = (app.clone(), ui.as_weak(), ensure_timer.clone());
        move |f| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let Some(c) = a.selected(&ui).cloned() else {
                return;
            };
            let to = f.clamp(0.0, 1.0) as f64 * c.duration;
            if a.active.as_deref() == Some(c.name.as_str()) && !a.running {
                a.paused_at = to;
                a.show_position(&ui);
            } else if a.play_clip(&ui, &c.name, to) {
                ensure();
            }
        }
    });

    ui.on_skip({
        let (app, weak, ensure) = (app.clone(), ui.as_weak(), ensure_timer.clone());
        move |secs| {
            if app.borrow_mut().skip(&weak.unwrap(), secs as f64) {
                ensure();
            }
        }
    });

    ui.on_speed_changed({
        let (app, weak, ensure) = (app.clone(), ui.as_weak(), ensure_timer.clone());
        move |i| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            a.speed = (i.max(0) as usize).min(SPEEDS.len() - 1);
            if a.running
                && let Some(name) = a.active.clone()
            {
                let at = a.position();
                if a.play_clip(&ui, &name, at) {
                    ensure();
                }
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
            let Some(name) = a.selected(&ui).map(|c| c.name.clone()) else {
                return;
            };
            if a.play_clip(&ui, &name, start) {
                ui.set_current_word(i);
                ensure();
            }
        }
    });

    ui.on_stop_playback({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || app.borrow_mut().stop(&weak.unwrap())
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
            if a.active.as_deref() == Some(name.as_str()) {
                a.stop(&ui);
            }
            match a.library.delete(&name) {
                Ok(()) => {
                    ui.set_status(format!("Moved {name} to the Recycle Bin").into());
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

    ui.on_rename_selected({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |new| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let Some(old) = a.selected(&ui).map(|c| c.name.clone()) else {
                return;
            };
            let new = new.trim();
            if new == old {
                return;
            }
            if a.active.as_deref() == Some(old.as_str()) {
                a.stop(&ui);
            }
            match a.library.rename(&old, new) {
                Ok(()) => {
                    a.refresh_clips(&ui);
                    a.select_name(&ui, new);
                    a.show_transcript(&ui);
                    ui.set_status(format!("Renamed to {new}").into());
                }
                Err(e) => ui.set_status(e.to_string().into()),
            }
        }
    });

    ui.on_mic_changed({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |idx| {
            let mut a = app.borrow_mut();
            a.settings.mic_device = usize::try_from(idx - 1)
                .ok()
                .and_then(|i| a.devices.get(i))
                .map(|d| d.id.clone());
            a.settings.save();
            weak.unwrap().set_device_label(a.device_label().into());
        }
    });

    ui.on_system_changed({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |on| {
            let mut a = app.borrow_mut();
            a.settings.record_system = on;
            a.settings.save();
            weak.unwrap().set_device_label(a.device_label().into());
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
            let size = if on { WIDGET_SIZE } else { FULL_SIZE };
            ui.window().set_size(size);
            // Windows keeps the framed window's minimum size until the frame is gone,
            // so set the size again once it is.
            let weak = weak.clone();
            Timer::single_shot(Duration::from_millis(80), move || {
                if let Some(ui) = weak.upgrade() {
                    ui.window().set_size(size);
                }
            });
        }
    });

    ui.on_selection_changed({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |_| {
            let ui = weak.unwrap();
            ui.set_renaming(ui.get_rename_pending());
            ui.set_rename_pending(false);
            app.borrow_mut().show_transcript(&ui);
            app.borrow().show_position(&ui);
        }
    });

    ui.on_waves_ready({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || app.borrow().apply_waves(&weak.unwrap())
    });

    ui.on_edit_markup(|text| {
        slint::StyledText::from_markdown(&edit_markup(&text))
            .unwrap_or_else(|_| slint::StyledText::from_plain_text(&text))
    });
    ui.on_plain_selection(|text, a, b| {
        let (a, b) = (a.min(b).max(0) as usize, a.max(b).max(0) as usize);
        let picked = text.get(a..b.min(text.len())).unwrap_or("");
        edit::without_tags(picked).into()
    });

    ui.on_edit_changed({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |text| app.borrow().preview_edit(&weak.unwrap(), &text)
    });

    // Save never changes the clip: the edit becomes a new clip, "name (edit)", whose
    // new words are spoken and whose audio is then rewritten as the edited result.
    ui.on_apply_transcript({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |text| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            if ui.get_speaking() {
                return ui.set_status("Wait until the new words are spoken, or cancel them".into());
            }
            let Some(name) = a.selected(&ui).map(|c| c.name.clone()) else {
                return;
            };
            let Ok(mut sc) = Sidecar::load(&a.library.sidecar_path(&name)) else {
                ui.set_status("Transcribe this clip first; older transcripts are read-only".into());
                return;
            };
            let changes = match edit::apply_text(&text, &mut sc) {
                Ok(c) => c,
                Err(e) => return ui.set_status(e.into()),
            };
            if changes.is_empty() {
                return ui.set_status("Nothing changed".into());
            }
            // New words need a voice sample from this clip. Without enough speech,
            // nothing is saved and the editor stays open.
            if changes.spoken > 0
                && let Err(e) = thrasher_core::voice::pick_reference(&sc)
            {
                return ui.set_status(e.to_string().into());
            }
            let new = a.library.edit_name(&name);
            if let Err(e) = a.library.copy_clip(&name, &new, &sc) {
                return ui.set_status(e.to_string().into());
            }
            a.stop(&ui);
            ui.set_page(1);
            voice::start(
                &ui,
                &a.library,
                new.clone(),
                a.settings.model.clone(),
                a.settings.language.clone(),
                true,
            );
            a.refresh_clips(&ui);
            a.select_name(&ui, &new);
            a.show_transcript(&ui);
            ui.set_status(format!("Saving as {new}: {}", changes.summary()).into());
        }
    });

    // A saved edit is finished: show its baked audio and transcript.
    ui.on_library_changed({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |name| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let keep = a.selected(&ui).map(|c| c.name.clone());
            a.refresh_clips(&ui);
            a.select_name(&ui, keep.as_deref().unwrap_or(&name));
            a.show_transcript(&ui);
        }
    });
    ui.on_cancel_speaking(voice::cancel);

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

    ui.on_export_as({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |kind| {
            let ui = weak.unwrap();
            let (library, name, clean_voice, default_format, folder) = {
                let a = app.borrow();
                let Some(name) = a.selected(&ui).map(|c| c.name.clone()) else {
                    return;
                };
                (
                    a.library.clone(),
                    name,
                    a.settings.clean_voice,
                    a.settings.export_format.clone(),
                    a.settings.export_dir.clone(),
                )
            };
            let mut kind = Export::ALL[(kind.max(0) as usize).min(Export::ALL.len() - 1)];
            if let Export::Project(_) = kind {
                kind = Export::Project(match default_format.as_str() {
                    "wav" => thrasher_core::export::Format::Wav,
                    "mp3" => thrasher_core::export::Format::Mp3,
                    _ => thrasher_core::export::Format::Flac,
                });
            }
            let start = folder
                .filter(|d| d.is_dir())
                .unwrap_or_else(|| library.dir().to_path_buf());
            let file = format!("{name}.{}", kind.extension());
            // The test window must never take focus, so it saves without asking.
            let test = std::env::var_os("THRASHER_TEST_WINDOW").is_some();
            let chosen = match kind {
                _ if test && matches!(kind, Export::Project(_)) => Some(start.clone()),
                _ if test => Some(start.join(&file)),
                // A project is a folder of files, so only the place for it is asked.
                Export::Project(_) => rfd::FileDialog::new()
                    .set_title("Export the whole project into a folder")
                    .set_directory(&start)
                    .pick_folder(),
                _ => rfd::FileDialog::new()
                    .set_title(format!("Export {}", kind.title()))
                    .set_directory(&start)
                    .set_file_name(&file)
                    .add_filter(kind.title(), &[kind.filter()])
                    .save_file(),
            };
            let Some(mut path) = chosen else {
                return;
            };
            let project = matches!(kind, Export::Project(_));
            if !project && path.extension().is_none() {
                path.set_extension(kind.filter());
            }
            let folder = if project {
                Some(path.as_path())
            } else {
                path.parent()
            };
            if let Some(dir) = folder {
                let mut a = app.borrow_mut();
                a.settings.export_dir = Some(dir.to_path_buf());
                a.settings.save();
            }
            ui.set_status(format!("Exporting {}...", short_path(&path)).into());
            let weak = ui.as_weak();
            std::thread::spawn(move || {
                let started = std::time::Instant::now();
                let result = kind.write(&library, &name, clean_voice, &path);
                crate::log!(
                    "export {} as {} took {} ms: {:?}",
                    name,
                    kind.title(),
                    crate::log::ms(started),
                    result.as_ref().err()
                );
                let status = match result {
                    Ok(true) if project => format!("Exported {name} into {}", short_path(&path)),
                    Ok(true) => format!("Exported {}", short_path(&path)),
                    Ok(false) => "Transcribe this clip first".to_string(),
                    Err(e) => format!("Export failed: {e}"),
                };
                let _ = weak.upgrade_in_event_loop(move |ui| ui.set_status(status.into()));
            });
        }
    });

    ui.on_language_changed({
        let app = app.clone();
        move |i| {
            let mut a = app.borrow_mut();
            if let Some((code, _)) = LANGUAGES.get(i as usize) {
                a.settings.language = (*code).into();
                a.settings.save();
            }
        }
    });

    ui.on_template_edited({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |text| {
            let ui = weak.unwrap();
            let a = app.borrow();
            ui.set_name_invalid(Template::parse(&text).is_err());
            let preview = match Template::parse(&text) {
                Ok(t) => {
                    let names = a.library.names().unwrap_or_default();
                    let next = t.next_name(
                        names.iter().map(String::as_str),
                        chrono::Local::now().naive_local(),
                    );
                    format!("Next: {next}.wav")
                }
                Err(e) => e.to_string(),
            };
            ui.set_name_preview(preview.into());
        }
    });
    ui.invoke_template_edited(app.borrow().settings.name_template.clone().into());

    ui.on_template_accepted({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |text| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            match Template::parse(&text) {
                Ok(t) => {
                    a.template = t;
                    a.settings.name_template = text.to_string();
                    a.settings.save();
                    ui.set_name_template(text.clone());
                    ui.set_status("File name pattern saved".into());
                }
                Err(e) => ui.set_status(e.to_string().into()),
            }
        }
    });

    ui.on_recording_limit_changed({
        let app = app.clone();
        move |i| {
            if let Some((minutes, _)) = settings::RECORDING_LIMITS.get(i as usize) {
                let mut a = app.borrow_mut();
                a.settings.max_recording_minutes = *minutes;
                a.settings.save();
            }
        }
    });

    ui.on_close_to_tray_changed({
        let app = app.clone();
        move |on| {
            let mut a = app.borrow_mut();
            a.settings.close_to_tray = on;
            a.settings.save();
        }
    });

    ui.on_start_with_windows_changed({
        let weak = ui.as_weak();
        move |on| {
            let ui = weak.unwrap();
            if !startup::set(on) {
                ui.set_status("Windows did not accept the change".into());
            }
            ui.set_start_with_windows(startup::enabled());
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
        ui.on_models_filter_changed({
            let (app, weak) = (app.clone(), ui.as_weak());
            move || transcription::show_models(&weak.unwrap(), &app.borrow().settings.model)
        });
        ui.on_use_model({
            let (app, weak) = (app.clone(), ui.as_weak());
            move |i| {
                let Some(m) = thrasher_core::models::CATALOG.get(i as usize) else {
                    return;
                };
                let ui = weak.unwrap();
                let mut a = app.borrow_mut();
                a.settings.model = m.id.into();
                if !m.language.is_empty() {
                    a.settings.language = m.language.into();
                    if let Some(p) = LANGUAGES.iter().position(|(c, _)| *c == m.language) {
                        ui.set_language_index(p as i32);
                    }
                }
                a.settings.save();
                transcription::show_models(&ui, m.id);
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

    ui.on_change_folder({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            let ui = weak.unwrap();
            let start = app.borrow().library.dir().to_path_buf();
            let Some(dir) = rfd::FileDialog::new()
                .set_title("Folder for new clips")
                .set_directory(&start)
                .pick_folder()
            else {
                return;
            };
            let mut a = app.borrow_mut();
            match Library::open(&dir) {
                Ok(lib) => {
                    a.stop(&ui);
                    a.library = lib;
                    a.settings.output_dir = dir.clone();
                    a.settings.save();
                    ui.set_folder(short_path(&dir).into());
                    a.refresh_clips(&ui);
                    ui.set_selected(-1);
                    a.show_transcript(&ui);
                }
                Err(e) => ui.set_status(format!("Cannot use {}: {e}", dir.display()).into()),
            }
        }
    });

    // Tray icon. With it, closing the window can hide it instead of quitting.
    let tray = Tray::new().ok();
    if let Some(tray) = &tray {
        tray.set_image(dot_image(32, false));
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
                tray.set_image(dot_image(32, rec));
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
    ui.on_resize_window({
        let weak = ui.as_weak();
        move |d| {
            use slint::winit_030::{WinitWindowAccessor, winit::window::ResizeDirection};
            let direction = match d {
                0 => ResizeDirection::North,
                1 => ResizeDirection::South,
                2 => ResizeDirection::East,
                3 => ResizeDirection::West,
                4 => ResizeDirection::NorthEast,
                5 => ResizeDirection::NorthWest,
                6 => ResizeDirection::SouthEast,
                _ => ResizeDirection::SouthWest,
            };
            weak.unwrap().window().with_winit_window(|w| {
                if let Err(e) = w.drag_resize_window(direction) {
                    crate::log!("resize failed: {e}");
                }
            });
        }
    });
    ui.on_minimize_window({
        let weak = ui.as_weak();
        move || weak.unwrap().window().set_minimized(true)
    });
    ui.on_close_window({
        let has_tray = tray.is_some();
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            if !has_tray || !app.borrow().settings.close_to_tray {
                let _ = slint::quit_event_loop();
            } else {
                let _ = weak.unwrap().hide();
            }
        }
    });

    ui.window().on_close_requested({
        let has_tray = tray.is_some();
        let app = app.clone();
        move || {
            if !has_tray || !app.borrow().settings.close_to_tray {
                let _ = slint::quit_event_loop();
            }
            slint::CloseRequestResponse::HideWindow
        }
    });

    ui.show()?;
    // Runs once the event loop has drawn the window.
    Timer::single_shot(Duration::ZERO, || {
        crate::log!("ready: window shown {} ms after start", log::uptime_ms())
    });
    let _watchdog = freeze_watchdog();
    slint::run_event_loop_until_quit()?;
    crate::log!("quit");
    drop(tray);

    // Close files properly if the window is closed mid-recording.
    let mut a = app.borrow_mut();
    a.player.stop();
    if let Some(rec) = a.recording.take() {
        let _ = rec.stop();
    }
    Ok(())
}

/// Logs when the window stops responding for a noticeable time. Checks four times a
/// second, so an idle window stays nearly free.
fn freeze_watchdog() -> Timer {
    let timer = Timer::default();
    let last = std::cell::Cell::new(std::time::Instant::now());
    timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
        let gap = last.get().elapsed().as_millis();
        if gap > 500 {
            crate::log!("the window froze for about {} ms", gap - 250);
        }
        last.set(std::time::Instant::now());
    });
    timer
}

/// With `THRASHER_TEST_WINDOW` set, the window opens without taking focus, so
/// automated checks do not get in the way of whoever is at the desk.
fn test_window() {
    if std::env::var_os("THRASHER_TEST_WINDOW").is_none() {
        return;
    }
    let selected = slint::BackendSelector::new()
        .with_winit_window_attributes_hook(|a| a.with_active(false))
        .select();
    if let Err(e) = selected {
        crate::log!("test window: {e}");
    }
}

/// Shows the record shortcut as key caps: "Ctrl+Alt+R" becomes Ctrl, Alt, R.
fn show_hotkey(ui: &AppWindow, spec: &str) {
    ui.set_hotkey(spec.into());
    let parts: Vec<SharedString> = spec
        .split('+')
        .filter(|p| !p.is_empty())
        .map(|p| if p == "Super" { "Win".into() } else { p.into() })
        .collect();
    ui.set_hotkey_parts(ModelRc::new(VecModel::from(parts)));
}

enum Spec {
    /// Only a modifier so far: keep listening.
    Wait,
    NeedsModifier,
    Done(String),
}

/// Turns a key pressed in the shortcut box into a shortcut such as "Ctrl+Alt+R".
fn hotkey_spec(text: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> Spec {
    use slint::platform::Key;
    let mut chars = text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return Spec::Wait;
    };
    let f_key = (1..=24).find(|n| {
        let k = match n {
            1 => Key::F1,
            2 => Key::F2,
            3 => Key::F3,
            4 => Key::F4,
            5 => Key::F5,
            6 => Key::F6,
            7 => Key::F7,
            8 => Key::F8,
            9 => Key::F9,
            10 => Key::F10,
            11 => Key::F11,
            12 => Key::F12,
            13 => Key::F13,
            14 => Key::F14,
            15 => Key::F15,
            16 => Key::F16,
            17 => Key::F17,
            18 => Key::F18,
            19 => Key::F19,
            20 => Key::F20,
            21 => Key::F21,
            22 => Key::F22,
            23 => Key::F23,
            _ => Key::F24,
        };
        char::from(k) == c
    });
    let key = if let Some(n) = f_key {
        format!("F{n}")
    } else if c == ' ' {
        "Space".into()
    } else if c.is_ascii_alphanumeric() {
        c.to_ascii_uppercase().to_string()
    } else {
        // A modifier key on its own (Slint sends those as control characters), or a
        // key a shortcut cannot use.
        return Spec::Wait;
    };
    if !(ctrl || alt || meta) && f_key.is_none() {
        return Spec::NeedsModifier;
    }
    let mut spec = String::new();
    for (on, name) in [
        (ctrl, "Ctrl"),
        (alt, "Alt"),
        (shift, "Shift"),
        (meta, "Super"),
    ] {
        if on {
            spec.push_str(name);
            spec.push('+');
        }
    }
    spec.push_str(&key);
    Spec::Done(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(s: Spec) -> Option<String> {
        match s {
            Spec::Done(x) => Some(x),
            _ => None,
        }
    }

    #[test]
    fn markup_colours_tags_and_escapes_the_rest() {
        assert_eq!(
            edit_markup("Hei* [silence:2s] der"),
            "Hei\\* <font color=\"#ff6a1a\">\\[silence\\:2s\\]</font> der"
        );
        for text in [
            "1. ord",
            "# x",
            "a <b> c",
            "_x_ `y`",
            "[silence:3",
            "linje\nto",
        ] {
            let styled = slint::StyledText::from_markdown(&edit_markup(text));
            assert!(styled.is_ok(), "{text}");
        }
    }

    #[test]
    fn export_menu_order_matches_the_settings_formats() {
        for (i, f) in EXPORT_FORMATS.iter().enumerate() {
            assert_eq!(Export::ALL[i].extension(), *f);
        }
        assert_eq!(Export::ALL[5].extension(), "words.json");
        assert_eq!(Export::ALL[5].filter(), "json");
        assert_eq!(Export::ALL[6].extension(), "txt");
        assert!(matches!(Export::ALL[7], Export::Project(_)));
    }

    #[test]
    fn shortcuts_from_key_presses() {
        assert_eq!(
            done(hotkey_spec("r", true, true, false, false)).as_deref(),
            Some("Ctrl+Alt+R")
        );
        assert_eq!(
            done(hotkey_spec("R", true, false, true, false)).as_deref(),
            Some("Ctrl+Shift+R")
        );
        let f9: String = char::from(slint::platform::Key::F9).into();
        assert_eq!(
            done(hotkey_spec(&f9, false, false, false, false)).as_deref(),
            Some("F9")
        );
        assert!(matches!(
            hotkey_spec("r", false, false, true, false),
            Spec::NeedsModifier
        ));
        use slint::platform::Key;
        for k in [
            Key::Control,
            Key::Shift,
            Key::Alt,
            Key::Meta,
            Key::Tab,
            Key::Return,
        ] {
            let k: String = char::from(k).into();
            assert!(matches!(
                hotkey_spec(&k, true, false, false, false),
                Spec::Wait
            ));
        }
        assert_eq!(
            done(hotkey_spec("5", false, false, false, true)).as_deref(),
            Some("Super+5")
        );
    }

    #[test]
    fn every_shortcut_made_here_parses() {
        for spec in ["Ctrl+Alt+R", "Ctrl+Shift+R", "F9", "Super+5", "Alt+Space"] {
            assert!(
                spec.parse::<global_hotkey::hotkey::HotKey>().is_ok(),
                "{spec}"
            );
        }
    }
}
