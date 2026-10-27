// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ask;
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
use thrasher_core::projects;
use thrasher_core::recorder::{RecorderOptions, Recording};
use thrasher_core::search;
use thrasher_core::sidecar::Sidecar;
use thrasher_core::sidecar::TrackKind;
use thrasher_core::wasapi_io::{DeviceInfo, input_devices};

slint::include_modules!();

const FULL_SIZE: LogicalSize = LogicalSize::new(520.0, 600.0);
const WIDGET_SIZE: LogicalSize = LogicalSize::new(240.0, 124.0);
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
fn edit_markup(text: &str, accent: slint::Color) -> String {
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
        out.push_str(&format!(
            "<font color=\"#{:02x}{:02x}{:02x}\">",
            accent.red(),
            accent.green(),
            accent.blue()
        ));
        escape(&mut out, &rest[i..end]);
        out.push_str("</font>");
        rest = &rest[end..];
    }
    escape(&mut out, rest);
    out
}

/// Clicks clip `i` in the list the way File Explorer does: a plain click picks just
/// it, Ctrl adds or removes it, Shift picks the range from the anchor (the last plain
/// or Ctrl click) to it, and Ctrl+Shift adds that range. `sel` is the highlighted
/// clip; returns the new one. A single picked clip is just the highlighted one, so
/// `picked` only ever holds two or more.
fn pick_clips(
    picked: &mut [bool],
    anchor: &mut Option<(usize, i32)>,
    sel: i32,
    i: i32,
    ctrl: bool,
    shift: bool,
) -> i32 {
    let n = picked.len();
    if i < 0 || i as usize >= n {
        picked.fill(false);
        *anchor = None;
        return -1;
    }
    let i = i as usize;
    let valid = |s: i32| (s >= 0 && (s as usize) < n).then_some(s as usize);
    // What is picked now, counting a lone highlighted clip.
    if !picked.contains(&true)
        && let Some(s) = valid(sel)
    {
        picked[s] = true;
    }
    let from = match *anchor {
        Some((at, was)) if was == sel && at < n => at,
        _ => valid(sel).unwrap_or(i),
    };
    let mut sel = i as i32;
    if shift {
        if !ctrl {
            picked.fill(false);
        }
        for p in &mut picked[from.min(i)..=from.max(i)] {
            *p = true;
        }
        *anchor = Some((from, sel));
    } else if ctrl {
        picked[i] = !picked[i];
        if !picked[i] {
            // The highlight stays on a picked clip.
            sel = picked.iter().position(|p| *p).map_or(-1, |p| p as i32);
        }
        *anchor = Some((i, sel));
    } else {
        picked.fill(false);
        *anchor = Some((i, sel));
    }
    if picked.iter().filter(|p| **p).count() < 2 {
        picked.fill(false);
    }
    sel
}

/// Work on a merged clip's parts, run on a worker thread: gets the library and the
/// clip's name, returns the status line (or an error for it).
type PartsJob = Box<dyn FnOnce(&Library, &str) -> Result<String, String> + Send>;

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
            Export::Project(_) => "all files",
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
    /// `english`: subtitles come from the English translation.
    fn write(
        self,
        library: &Library,
        name: &str,
        clean_voice: bool,
        english: bool,
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
            Export::Subtitles(kind) if english => {
                thrasher_core::export::export_translation_to(library, name, kind, path)
            }
            Export::Subtitles(kind) => {
                thrasher_core::export::export_subtitles_to(library, name, kind, path)
            }
        }
    }
}

struct App {
    settings: Settings,
    /// The clip folder, which is also the Inbox project.
    root: std::path::PathBuf,
    /// The project shown and recorded into; empty is Inbox.
    project: String,
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
    /// Clips picked with Ctrl or Shift, one flag per clip in the list.
    picked: Vec<bool>,
    /// Where a Shift click range starts, and the highlighted clip when it was set
    /// (moving the highlight another way, such as with the arrow keys, moves it).
    anchor: Option<(usize, i32)>,
    /// The clips on the merge page, in merge order.
    merge_names: Vec<String>,
    /// The transcription running now: its folder and clips. Clips asked for meanwhile
    /// wait in `queue`, with their folder (the project may change).
    /// The last flag is true for a translation.
    job: Option<(std::path::PathBuf, Vec<String>, bool)>,
    queue: VecDeque<(std::path::PathBuf, String, bool)>,
    /// A question is being answered.
    asking: bool,
    /// The widget is showing because a recording window was covered or minimised; the
    /// window comes back when the user switches to Thrasher.
    auto_widget: bool,
    /// The clip the local AI is translating, and into which language.
    ai_translation: Option<(std::path::PathBuf, String, String)>,
    /// What the translate menu's search field holds.
    lang_filter: String,
    /// The highlighted clip has Whisper's English with times, for subtitles.
    english_words: bool,
    /// New recordings to name with the AI once they are transcribed.
    to_name: Vec<(std::path::PathBuf, String)>,
    /// The clip the local AI is summarising, with its folder.
    ai_job: Option<(std::path::PathBuf, String)>,
    /// Transcripts read for search, kept while their sidecar is unchanged.
    search_docs: HashMap<std::path::PathBuf, (Option<SystemTime>, search::Doc)>,
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
        || !sc.inserts.is_empty()
        || sc.trim.is_some();
    if !edited || (sc.duration - c.duration).abs() >= 0.5 {
        return c.duration;
    }
    edit::edited_length(sc, &voice_lengths(library, &c.name, sc))
}

impl App {
    fn refresh_clips(&mut self, ui: &AppWindow) {
        self.clips = self.library.list().unwrap_or_default();
        let rows: Vec<ClipRow> = self
            .clips
            .iter()
            .map(|c| {
                let sc = Sidecar::load(&self.library.sidecar_path(&c.name)).ok();
                let secs = sc
                    .as_ref()
                    .map_or(c.duration, |sc| edited_secs(&self.library, c, sc));
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
        self.picked = vec![false; self.clips.len()];
        self.anchor = None;
        self.show_picked(ui);
        let n = self.clips.len();
        let sel = ui.get_selected();
        if sel >= n as i32 {
            ui.set_selected(n as i32 - 1);
        }
        self.sync_active(ui);
        let (library, clips) = (self.library.clone(), self.clips.clone());
        self.waves.request(ui, &library, &clips);
    }

    /// Transcribes `names` in the shown project, now or after the job that runs.
    /// `auto` (a new clip, with auto transcribe on) never leaves the page.
    fn transcribe(&mut self, ui: &AppWindow, names: Vec<String>, auto: bool) {
        self.enqueue(ui, names, auto, false);
    }

    /// Translates the clip into English, after whatever runs now, with a model that
    /// can (see `models::translator`).
    fn translate(&mut self, ui: &AppWindow, name: String) {
        if thrasher_core::models::translator(&self.settings.model).is_none() {
            ui.set_status("Download a Whisper model to translate".into());
            ui.invoke_visit(4);
            return;
        }
        self.enqueue(ui, vec![name], false, true);
    }

    fn enqueue(&mut self, ui: &AppWindow, names: Vec<String>, auto: bool, translate: bool) {
        if transcription::worker_path().is_none() {
            if !auto {
                ui.set_status("Transcription is not part of this build".into());
            }
            return;
        }
        if !translate && !transcription::model_ready(&self.settings.model) {
            ui.set_status("Download a transcription model first".into());
            if !auto {
                ui.invoke_visit(3);
            }
            return;
        }
        let dir = self.library.dir().to_path_buf();
        for name in names {
            let item = (dir.clone(), name, translate);
            let running = self
                .job
                .as_ref()
                .is_some_and(|(d, ns, t)| *d == item.0 && ns.contains(&item.1) && *t == item.2);
            if !running && !self.queue.contains(&item) {
                self.queue.push_back(item);
            }
        }
        if self.job.is_some() && !auto {
            ui.set_status("Queued".into());
        }
        self.next_job(ui);
    }

    /// Starts the next queued clips: all those waiting in the same folder as the
    /// first, in one worker run.
    fn next_job(&mut self, ui: &AppWindow) {
        if self.job.is_some() {
            return;
        }
        let Some((dir, _, translate)) = self.queue.front().cloned() else {
            return;
        };
        let (now, later): (VecDeque<_>, VecDeque<_>) = self
            .queue
            .drain(..)
            .partition(|(d, _, t)| *d == dir && *t == translate);
        self.queue = later;
        let names: Vec<String> = now.into_iter().map(|(_, n, _)| n).collect();
        self.job = Some((dir.clone(), names.clone(), translate));
        let model = if translate {
            thrasher_core::models::translator(&self.settings.model).map_or("", |m| m.id)
        } else {
            &self.settings.model
        };
        transcription::start(
            ui,
            dir,
            names,
            model,
            self.settings.language.clone(),
            translate,
        );
        self.show_transcribing(ui);
    }

    /// The transcript card shows progress when the highlighted clip is being transcribed.
    fn show_transcribing(&self, ui: &AppWindow) {
        let job = self.selected(ui).and_then(|c| {
            self.job
                .as_ref()
                .filter(|(d, ns, _)| d == self.library.dir() && ns.contains(&c.name))
        });
        ui.set_transcribing(job.is_some());
        ui.set_translating(job.is_some_and(|j| j.2));
    }

    /// The clip's transcript, prepared for search; read again only when it changed.
    fn search_doc(&mut self, path: &std::path::Path) -> search::Doc {
        let stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if let Some((s, d)) = self.search_docs.get(path)
            && *s == stamp
        {
            return d.clone();
        }
        let d = Sidecar::load(path)
            .map(|sc| search::doc(&sc))
            .unwrap_or_default();
        self.search_docs
            .insert(path.to_path_buf(), (stamp, d.clone()));
        d
    }

    /// Fills the search results: clips of this project (or of all of them) whose
    /// transcript or name holds `query`, newest first.
    fn search(&mut self, ui: &AppWindow, query: &str, all: bool) {
        const MAX_HITS: usize = 200;
        let mut found: Vec<(SystemTime, SearchHit)> = Vec::new();
        if !query.trim().is_empty() {
            let current = ui.get_project();
            let names: Vec<SharedString> = ui.get_projects().iter().collect();
            let places: Vec<i32> = if all {
                (0..names.len() as i32).collect()
            } else {
                vec![current]
            };
            for p in places {
                let Some(name) = App::project_at(ui, p) else {
                    continue;
                };
                let (library, clips) = if p == current {
                    (self.library.clone(), self.clips.clone())
                } else {
                    let Ok(lib) = Library::open(projects::dir(&self.root, &name)) else {
                        continue;
                    };
                    let clips = lib.list().unwrap_or_default();
                    (lib, clips)
                };
                let shown = names.get(p as usize).cloned().unwrap_or_default();
                for c in clips {
                    let d = self.search_doc(&library.sidecar_path(&c.name));
                    if let Some(h) = search::find(&c.name, &d, query) {
                        found.push((
                            c.created,
                            SearchHit {
                                clip: c.name.as_str().into(),
                                project: shown.clone(),
                                project_index: p,
                                before: h.before.into(),
                                matched: h.matched.into(),
                                after: h.after.into(),
                                more: h.more as i32,
                                at: h.at as f32,
                            },
                        ));
                    }
                }
            }
        }
        found.sort_by_key(|f| std::cmp::Reverse(f.0));
        if found.len() > MAX_HITS {
            crate::log!(
                "search for {query:?}: showing {MAX_HITS} of {}",
                found.len()
            );
            found.truncate(MAX_HITS);
        }
        let hits: Vec<SearchHit> = found.into_iter().map(|(_, h)| h).collect();
        ui.set_hits(ModelRc::new(VecModel::from(hits)));
    }

    /// Fills the translate menu for the highlighted clip: every language but its own,
    /// matching the search, with whether it is made already and whether it can be.
    fn show_langs(&self, ui: &AppWindow, sc: Option<&Sidecar>) {
        let Some(sc) = sc.filter(|sc| !sc.words.is_empty()) else {
            ui.set_langs(ModelRc::default());
            return;
        };
        let whisper = thrasher_core::models::translator(&self.settings.model).is_some()
            && transcription::worker_path().is_some();
        let ai = ask::worker_path().is_some()
            && thrasher_core::ai::find(thrasher_core::ai::TRANSLATOR)
                .is_some_and(|m| m.is_downloaded());
        let own = match sc.language.as_deref() {
            Some("nb" | "nn") => "no",
            Some(code) => code,
            None => "",
        };
        let q = self.lang_filter.trim().to_lowercase();
        let rows: Vec<LangRow> = thrasher_core::ai::LANGUAGES
            .iter()
            .filter(|l| l.code != own)
            .filter(|l| {
                q.is_empty()
                    || l.name.to_lowercase().contains(&q)
                    || l.native.to_lowercase().contains(&q)
                    || l.code == q
            })
            .map(|l| {
                let done = (l.code == "en" && !sc.translation.is_empty())
                    || sc.translations.contains_key(l.code);
                let ready = done || ai || (l.code == "en" && whisper);
                // English can come from Whisper (with times); the rest need the AI.
                let install = if ready {
                    0
                } else if l.code == "en" && ask::worker_path().is_none() {
                    1
                } else {
                    2
                };
                LangRow {
                    code: l.code.into(),
                    name: l.name.into(),
                    native: l.native.into(),
                    ready,
                    done,
                    install,
                }
            })
            .collect();
        ui.set_langs(ModelRc::new(VecModel::from(rows)));
    }

    /// Shows the clip in `code`, translating it first when it has no such translation
    /// yet: English with Whisper when a model can (it keeps word times), the rest with
    /// the local AI.
    fn choose_lang(&mut self, ui: &AppWindow, code: &str) {
        let Some(c) = self.selected(ui).cloned() else {
            return;
        };
        let path = self.library.sidecar_path(&c.name);
        let Ok(sc) = Sidecar::load(&path) else {
            return;
        };
        ui.set_shown_lang(code.into());
        ui.set_show_summary(false);
        let done =
            (code == "en" && !sc.translation.is_empty()) || sc.translations.contains_key(code);
        if done {
            self.show_transcript(ui);
            return;
        }
        if code == "en" && thrasher_core::models::translator(&self.settings.model).is_some() {
            ui.set_translation("".into());
            self.translate(ui, c.name.clone());
            return;
        }
        let Some(worker) = ask::worker_path() else {
            return;
        };
        let model = thrasher_core::ai::find(thrasher_core::ai::TRANSLATOR)
            .filter(|m| m.is_downloaded())
            .and_then(|m| m.path());
        let Some(model) = model else {
            ui.set_shown_lang("".into());
            ui.set_status("Download the translation model first".into());
            ui.invoke_visit(3);
            return;
        };
        if self.ai_job.is_some() || self.asking || self.ai_translation.is_some() {
            ui.set_shown_lang("".into());
            ui.set_status("The AI is busy".into());
            return;
        }
        let Some(language) = thrasher_core::ai::LANGUAGES.iter().find(|l| l.code == code) else {
            return;
        };
        let Some(job) = thrasher_core::ai::translate_job(&sc, language, &model) else {
            return;
        };
        crate::log!(
            "translating {} into {code} with the AI in {} part(s)",
            c.name,
            job.steps.len()
        );
        self.ai_translation = Some((
            self.library.dir().to_path_buf(),
            c.name.clone(),
            code.to_string(),
        ));
        ui.set_translation("".into());
        let (weak, name, code) = (ui.as_weak(), c.name.clone(), code.to_string());
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let live = |text: String| {
                let (weak, name, code) = (weak.clone(), name.clone(), code.clone());
                move || {
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        let shown = ui
                            .get_clips()
                            .row_data(ui.get_selected() as usize)
                            .is_some_and(|r| r.name == name)
                            && ui.get_shown_lang() == code.as_str();
                        if shown {
                            ui.set_translation(text.into());
                        }
                    });
                }
            };
            let result = ask::run(&worker, &job, &mut |_| {}, &mut |text| {
                live(thrasher_core::ai::tidy(text))()
            });
            crate::log!(
                "translation of {name} into {code} took {} ms: {:?}",
                crate::log::ms(started),
                result.as_ref().err()
            );
            let result = result.and_then(|text| {
                let text = thrasher_core::ai::tidy(&text);
                let mut sc = Sidecar::load(&path).map_err(|e| e.to_string())?;
                sc.translations.insert(code.clone(), text.clone());
                sc.save(&path).map_err(|e| e.to_string())?;
                Ok(text)
            });
            let ok = result.is_ok();
            let text = result.unwrap_or_else(|e| e);
            let _ = weak.upgrade_in_event_loop(move |ui| {
                ui.invoke_translated(ok, name.into(), code.into(), text.into())
            });
        });
    }

    /// Fills the project dropdown: Inbox, then the projects by name.
    fn show_projects(&self, ui: &AppWindow) {
        let list = projects::list(&self.root).unwrap_or_default();
        let current = list
            .iter()
            .position(|p| *p == self.project)
            .map_or(0, |i| i as i32 + 1);
        let mut names: Vec<SharedString> = vec![projects::INBOX.into()];
        names.extend(list.into_iter().map(SharedString::from));
        ui.set_projects(ModelRc::new(VecModel::from(names)));
        ui.set_project(current);
    }

    /// The name of entry `i` of the project dropdown; empty for Inbox.
    fn project_at(ui: &AppWindow, i: i32) -> Option<String> {
        if i == 0 {
            return Some(String::new());
        }
        ui.get_projects()
            .row_data(usize::try_from(i).ok()?)
            .map(|s| s.to_string())
    }

    /// Shows project `name` (empty for Inbox) and records into it from now on.
    fn open_project(&mut self, ui: &AppWindow, name: &str) {
        match Library::open(projects::dir(&self.root, name)) {
            Ok(lib) => {
                self.stop(ui);
                self.active = None;
                self.library = lib;
                self.project = name.to_string();
                self.settings.project = self.project.clone();
                self.settings.save();
                // Clips in different projects can share a name.
                self.waves = waves::Waves::default();
                self.refresh_clips(ui);
                ui.set_selected(-1);
                self.show_transcript(ui);
                self.show_projects(ui);
            }
            Err(e) => ui.set_status(format!("Cannot open {name}: {e}").into()),
        }
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

    fn show_picked(&self, ui: &AppWindow) {
        ui.set_picked(ModelRc::new(VecModel::from(self.picked.clone())));
        ui.set_picked_count(self.picked.iter().filter(|p| **p).count() as i32);
    }

    /// Lists the parts of the selected clip when it was merged from several.
    fn show_parts(&self, ui: &AppWindow) {
        let parts = self
            .selected(ui)
            .and_then(|c| Sidecar::load(&self.library.sidecar_path(&c.name)).ok())
            .map(|sc| sc.parts)
            .unwrap_or_default();
        let rows: Vec<OrderRow> = if parts.len() < 2 {
            Vec::new()
        } else {
            parts
                .iter()
                .map(|p| OrderRow {
                    name: p.name.clone().into(),
                    note: format!(
                        "{} · starts at {}",
                        fmt_time(p.end - p.start),
                        fmt_time(p.start)
                    )
                    .into(),
                })
                .collect()
        };
        ui.set_parts(ModelRc::new(VecModel::from(rows)));
    }

    /// Shows the selected clip on the clip page: details, waveform and transcript.
    /// Puts the trim handles where the highlighted clip's trim is.
    fn show_trim(&self, ui: &AppWindow) {
        let (mut start, mut end) = (0.0, 1.0);
        if let Some(c) = self.selected(ui)
            && let Ok(sc) = Sidecar::load(&self.library.sidecar_path(&c.name))
            && let Some(t) = sc.trim
            && c.duration > 0.0
        {
            start = (t.start / c.duration).clamp(0.0, 1.0) as f32;
            end = (t.end / c.duration).clamp(0.0, 1.0) as f32;
        }
        ui.set_trim_start(start);
        ui.set_trim_end(end);
    }

    fn show_transcript(&mut self, ui: &AppWindow) {
        self.show_transcribing(ui);
        self.show_trim(ui);
        self.update_length(ui);
        self.show_parts(ui);
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
            ui.set_translation("".into());
            self.english_words = false;
            self.show_langs(ui, None);
            ui.set_summary("".into());
            ui.set_summarising(false);
            ui.set_words(ModelRc::default());
            self.transcript_starts.clear();
            return;
        };
        ui.set_transcript(edit::text(&sc).into());
        let english: Vec<&str> = sc
            .translation
            .iter()
            .filter(|w| sc.in_trim(w.start))
            .map(|w| w.text.trim())
            .collect();
        self.english_words = !english.is_empty();
        self.show_langs(ui, Some(&sc));
        let shown = ui.get_shown_lang().to_string();
        let busy = self.ai_translation.as_ref().is_some_and(|(d, n, code)| {
            d == self.library.dir()
                && Some(n) == self.selected(ui).map(|c| &c.name)
                && *code == shown
        }) || (shown == "en" && ui.get_translating() && ui.get_transcribing());
        let text = match shown.as_str() {
            "" => String::new(),
            "en" if !english.is_empty() => english.join(" "),
            code => sc.translations.get(code).cloned().unwrap_or_default(),
        };
        if !shown.is_empty() && text.is_empty() && !busy {
            ui.set_shown_lang("".into());
        }
        ui.set_translation(text.into());
        ui.set_summary(sc.summary.clone().unwrap_or_default().into());
        let name = self
            .selected(ui)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        ui.set_summarising(
            self.ai_job
                .as_ref()
                .is_some_and(|(d, n)| d == self.library.dir() && *n == name),
        );
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

    /// Puts `name` in the player at `at` seconds without starting it, so Play starts
    /// there. A clip that is playing jumps there and keeps playing.
    fn cue(&mut self, ui: &AppWindow, name: &str, at: f64) -> bool {
        if self.active.as_deref() == Some(name) && (self.running || self.loading.is_some()) {
            return self.play_clip(ui, name, at);
        }
        if self.active.as_deref() != Some(name) {
            self.player.stop();
            self.loading = None;
            self.running = false;
            // The clip's edits, when it was read before, so the time shown is the
            // edited time Play will count in.
            self.timeline = self
                .loaded
                .as_ref()
                .filter(|l| l.name == name)
                .map(|l| l.timeline.clone())
                .unwrap_or_default();
            self.active = Some(name.to_string());
            ui.set_playing(false);
            self.sync_active(ui);
        }
        self.paused_at = at;
        self.show_position(ui);
        false
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
                // A saved clip shows up in the list; only problems get a line.
                if !done.warnings.is_empty() {
                    ui.set_status(done.warnings.join(". ").into());
                }
                self.refresh_clips(ui);
                // Nothing highlighted, so Space records again.
                ui.set_selected(-1);
                if self.settings.auto_transcribe {
                    if self.settings.ai_names {
                        let item = (self.library.dir().to_path_buf(), done.name.clone());
                        self.to_name.push(item);
                    }
                    self.transcribe(ui, vec![done.name.clone()], true);
                }
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
    let project = if projects::list(&dir)
        .unwrap_or_default()
        .contains(&settings.project)
    {
        settings.project.clone()
    } else {
        String::new()
    };
    let library = match Library::open(projects::dir(&dir, &project)) {
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
    thrasher_core::merge::clean_leftovers(&library);
    let live_model = Rc::new(VecModel::from(vec![0.06; LIVE_BARS]));
    ui.set_live(ModelRc::from(live_model.clone()));
    ui.set_window_icon(dot_image(64, true));

    let app = Rc::new(RefCell::new(App {
        settings,
        root: dir.clone(),
        project,
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
        picked: Vec::new(),
        anchor: None,
        merge_names: Vec::new(),
        job: None,
        queue: VecDeque::new(),
        search_docs: HashMap::new(),
        ai_job: None,
        asking: false,
        auto_widget: false,
        ai_translation: None,
        lang_filter: String::new(),
        english_words: false,
        to_name: Vec::new(),
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
        ui.set_folder(short_path(&a.root).into());
        a.show_projects(&ui);
        ui.set_name_template(a.settings.name_template.clone().into());
        ui.set_close_to_tray(a.settings.close_to_tray);
        ui.set_auto_transcribe(a.settings.auto_transcribe);
        ui.set_ai_names(a.settings.ai_names);
        ui.set_can_ai(ask::worker_path().is_some());
        show_ai_model(&ui, &a.settings.ai_model);
        let pauses: Vec<SharedString> = settings::MAX_PAUSES
            .iter()
            .map(|(_, l)| (*l).into())
            .collect();
        ui.set_max_pauses(ModelRc::new(VecModel::from(pauses)));
        ui.set_max_pause_index(
            settings::MAX_PAUSES
                .iter()
                .position(|(s, _)| (s - a.settings.max_pause).abs() < 1e-6)
                .unwrap_or(0) as i32,
        );
        let theme = ui.global::<Theme>();
        if let Some(i) = theme.get_names().iter().position(|n| n == a.settings.theme) {
            theme.set_current(i as i32);
        }
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
                    ui.set_status("Hold Ctrl Alt or Win too".into());
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

    ui.on_cue_clip({
        let (app, weak, ensure) = (app.clone(), ui.as_weak(), ensure_timer.clone());
        move |i, f| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let Some(c) = a.clips.get(i as usize).cloned() else {
                return;
            };
            ui.set_selected(i);
            if a.cue(&ui, &c.name, f.clamp(0.0, 1.0) as f64 * c.duration) {
                ensure();
            }
        }
    });
    ui.on_play_clip_at({
        let (app, weak, ensure) = (app.clone(), ui.as_weak(), ensure_timer.clone());
        move |i, f| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let Some(c) = a.clips.get(i as usize).cloned() else {
                return;
            };
            ui.set_selected(i);
            if a.play_clip(&ui, &c.name, f.clamp(0.0, 1.0) as f64 * c.duration) {
                ensure();
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

    ui.on_trim_changed({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |start, end| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let Some(c) = a.selected(&ui).cloned() else {
                return;
            };
            let path = a.library.sidecar_path(&c.name);
            // A clip without a sidecar gets one, so the trim has somewhere to live.
            let mut sc = Sidecar::load(&path).unwrap_or_else(|_| Sidecar {
                file: format!("{}.wav", c.name),
                duration: c.duration,
                ..Default::default()
            });
            let (s, e) = (start.clamp(0.0, 1.0) as f64, end.clamp(0.0, 1.0) as f64);
            sc.trim = (s > 0.0005 || e < 0.9995).then_some(thrasher_core::sidecar::Trim {
                start: s * c.duration,
                end: e * c.duration,
            });
            if let Err(e) = sc.save(&path) {
                ui.set_status(format!("Cannot save the trim: {e}").into());
                return;
            }
            crate::log!("trim of {} set to {:?}", c.name, sc.trim);
            // The edited audio changed; it is read again on the next play.
            if a.active.as_deref() == Some(c.name.as_str()) {
                a.stop(&ui);
            }
            let sel = ui.get_selected();
            a.refresh_clips(&ui);
            ui.set_selected(sel);
            a.show_transcript(&ui);
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
            // The highlighted clip among several picked: all of them go.
            let sel = ui.get_selected();
            let names: Vec<String> = if !last
                && a.picked.iter().filter(|p| **p).count() >= 2
                && a.picked.get(sel as usize).copied().unwrap_or(false)
            {
                a.clips
                    .iter()
                    .zip(&a.picked)
                    .filter(|(_, p)| **p)
                    .map(|(c, _)| c.name.clone())
                    .collect()
            } else {
                vec![name]
            };
            let recording = a.recording.as_ref().map(|r| r.name().to_string());
            let (mut gone, mut failed) = (Vec::new(), None);
            for name in names.iter().filter(|n| recording.as_ref() != Some(*n)) {
                if a.active.as_deref() == Some(name.as_str()) {
                    a.stop(&ui);
                }
                match a.library.delete(name) {
                    Ok(()) => gone.push(name.clone()),
                    Err(e) => failed = Some(e.to_string()),
                }
            }
            if !gone.is_empty() {
                a.refresh_clips(&ui);
                ui.set_selected(-1);
                a.show_transcript(&ui);
            }
            let status = match (gone.as_slice(), failed) {
                (_, Some(e)) => e,
                ([one], None) => format!("Moved {one} to the Recycle Bin"),
                (many, None) => format!("Moved {} clips to the Recycle Bin", many.len()),
            };
            ui.set_status(status.into());
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

    ui.on_compact_changed({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |on| {
            let ui = weak.unwrap();
            ui.set_compact(on);
            app.borrow_mut().auto_widget = false;
            let place = app.borrow().settings.window;
            let size = if on {
                WIDGET_SIZE
            } else {
                place.map_or(FULL_SIZE, |p| LogicalSize::new(p.width, p.height))
            };
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
            ui.set_renaming(false);
            app.borrow_mut().show_transcript(&ui);
            app.borrow().show_position(&ui);
        }
    });

    ui.on_import_files({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            let ui = weak.unwrap();
            let library = app.borrow().library.clone();
            // The test window must never take focus, so it imports the files named in
            // THRASHER_TEST_IMPORT instead of asking.
            let files = match std::env::var("THRASHER_TEST_IMPORT") {
                Ok(list) if std::env::var_os("THRASHER_TEST_WINDOW").is_some() => {
                    list.split(';').map(std::path::PathBuf::from).collect()
                }
                _ => rfd::FileDialog::new()
                    .set_title("Import")
                    .add_filter("Audio and video", &thrasher_core::import::EXTENSIONS)
                    .pick_files()
                    .unwrap_or_default(),
            };
            import_paths(&ui, library, files);
        }
    });
    ui.on_imported({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |status, last, names| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            a.refresh_clips(&ui);
            if !last.is_empty() {
                a.select_name(&ui, &last);
            }
            ui.set_status(status);
            if a.settings.auto_transcribe && !names.is_empty() {
                let names = names.split('\n').map(str::to_string).collect();
                a.transcribe(&ui, names, true);
            }
        }
    });

    ui.on_pick({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |i, ctrl, shift| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let n = a.clips.len();
            a.picked.resize(n, false);
            let App { picked, anchor, .. } = &mut *a;
            let sel = pick_clips(picked, anchor, ui.get_selected(), i, ctrl, shift);
            ui.set_selected(sel);
            a.show_picked(&ui);
        }
    });

    let merge_rows = |a: &App, ui: &AppWindow| {
        let rows: Vec<OrderRow> = a
            .merge_names
            .iter()
            .map(|n| {
                let note = a
                    .clips
                    .iter()
                    .find(|c| &c.name == n)
                    .map(|c| fmt_time(c.duration))
                    .unwrap_or_default();
                OrderRow {
                    name: n.into(),
                    note: note.into(),
                }
            })
            .collect();
        ui.set_merge_rows(ModelRc::new(VecModel::from(rows)));
    };
    ui.on_start_merge({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let names: Vec<String> = a
                .clips
                .iter()
                .zip(&a.picked)
                .filter(|(_, p)| **p)
                .map(|(c, _)| c.name.clone())
                .collect();
            let base = format!("{} merged", names.first().cloned().unwrap_or_default());
            let mut name = base.clone();
            let mut n = 2;
            while a.library.exists(&name) {
                name = format!("{base} {n}");
                n += 1;
            }
            a.merge_names = names;
            ui.set_merge_name(name.into());
            merge_rows(&a, &ui);
        }
    });
    ui.on_merge_move({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |from, to| {
            let mut a = app.borrow_mut();
            let n = a.merge_names.len() as i32;
            if from < 0 || to < 0 || from >= n || to >= n {
                return;
            }
            let item = a.merge_names.remove(from as usize);
            a.merge_names.insert(to as usize, item);
            merge_rows(&a, &weak.unwrap());
        }
    });
    ui.on_merge_remove({
        let (app, weak) = (app.clone(), ui.as_weak());
        // The clip leaves the merge untouched and is no longer picked. With fewer than
        // two left there is nothing to merge, so the list comes back.
        move |i| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let Some(name) = a.merge_names.get(i as usize).cloned() else {
                return;
            };
            a.merge_names.remove(i as usize);
            if let Some(k) = a.clips.iter().position(|c| c.name == name)
                && let Some(p) = a.picked.get_mut(k)
            {
                *p = false;
            }
            if a.merge_names.len() < 2 {
                a.merge_names.clear();
                a.picked.fill(false);
                ui.set_page(0);
            }
            a.show_picked(&ui);
            merge_rows(&a, &ui);
        }
    });
    ui.on_merge_play({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |i| {
            let ui = weak.unwrap();
            let index = {
                let a = app.borrow();
                let Some(name) = a.merge_names.get(i as usize) else {
                    return;
                };
                a.clips.iter().position(|c| &c.name == name)
            };
            if let Some(k) = index {
                ui.invoke_play_clip(k as i32);
            }
        }
    });
    ui.on_merge_run({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |name| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let name = name.trim().to_string();
            let names = a.merge_names.clone();
            if !thrasher_core::library::valid_name(&name) {
                return ui.set_status(format!("'{name}' cannot be a file name").into());
            }
            if a.library.exists(&name) && !names.contains(&name) {
                return ui.set_status(format!("A clip named '{name}' already exists").into());
            }
            if a.recording
                .as_ref()
                .is_some_and(|r| names.iter().any(|n| n == r.name()))
            {
                return ui.set_status("Stop the recording first".into());
            }
            if a.active.as_ref().is_some_and(|n| names.contains(n)) {
                a.stop(&ui);
            }
            ui.set_merging(true);
            ui.set_status(format!("Merging {} clips...", names.len()).into());
            let library = a.library.clone();
            let weak = ui.as_weak();
            std::thread::spawn(move || {
                let started = std::time::Instant::now();
                let result = thrasher_core::merge::merge(&library, &names, &name);
                crate::log!(
                    "merged {} clips into {name} in {} ms: {:?}",
                    names.len(),
                    crate::log::ms(started),
                    result.as_ref().err()
                );
                let (status, made) = match result {
                    Ok(()) => (format!("Merged {} clips into {name}", names.len()), name),
                    Err(e) => (format!("Merge failed: {e}"), String::new()),
                };
                let _ = weak
                    .upgrade_in_event_loop(move |ui| ui.invoke_merged(status.into(), made.into()));
            });
        }
    });
    ui.on_merged({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |status, name| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            ui.set_merging(false);
            a.waves.forget(&name);
            a.refresh_clips(&ui);
            if !name.is_empty() {
                a.merge_names.clear();
                a.select_name(&ui, &name);
                ui.set_page(0);
            }
            ui.set_status(status);
        }
    });

    // Reordering parts and taking one out rewrite the clip on a worker thread.
    let parts_job = {
        let (app, weak) = (app.clone(), ui.as_weak());
        move |job: PartsJob| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let Some(name) = a.selected(&ui).map(|c| c.name.clone()) else {
                return;
            };
            if a.active.as_deref() == Some(name.as_str()) {
                a.stop(&ui);
            }
            ui.set_parts_busy(true);
            let library = a.library.clone();
            let weak = ui.as_weak();
            std::thread::spawn(move || {
                let started = std::time::Instant::now();
                let status = job(&library, &name).unwrap_or_else(|e| e);
                crate::log!("parts of {name}: {status} ({} ms)", crate::log::ms(started));
                let _ = weak.upgrade_in_event_loop(move |ui| ui.invoke_parts_done(status.into()));
            });
        }
    };
    ui.on_parts_move({
        let job = parts_job.clone();
        move |from, to| {
            job(Box::new(move |lib: &Library, name: &str| {
                let n = thrasher_core::merge::parts(lib, name)
                    .map_err(|e| e.to_string())?
                    .len();
                let mut order: Vec<usize> = (0..n).collect();
                let (from, to) = (from as usize, to as usize);
                if from >= n || to >= n {
                    return Err("No such part".into());
                }
                let i = order.remove(from);
                order.insert(to, i);
                thrasher_core::merge::reorder(lib, name, &order)
                    .map(|()| String::new())
                    .map_err(|e| format!("Could not reorder: {e}"))
            }))
        }
    });
    ui.on_part_take_out(move |i| {
        parts_job(Box::new(move |lib: &Library, name: &str| {
            thrasher_core::merge::extract(lib, name, i as usize)
                .map(|new| format!("{new} is a clip again"))
                .map_err(|e| format!("Could not remove it: {e}"))
        }))
    });
    // A part's name is only a label in the merged clip's sidecar; it names the clip
    // the part becomes when it is removed.
    ui.on_rename_part({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |i, new| {
            let ui = weak.unwrap();
            let a = app.borrow();
            let new = new.trim();
            let Some(name) = a.selected(&ui).map(|c| c.name.clone()) else {
                return;
            };
            if new.is_empty() {
                return;
            }
            let path = a.library.sidecar_path(&name);
            let result = Sidecar::load(&path).and_then(|mut sc| {
                let part = sc
                    .parts
                    .get_mut(i as usize)
                    .ok_or_else(|| std::io::Error::other("no such part"))?;
                part.name = new.to_string();
                sc.save(&path)
            });
            match result {
                Ok(()) => a.show_parts(&ui),
                Err(e) => ui.set_status(format!("Could not rename it: {e}").into()),
            }
        }
    });
    ui.on_parts_done({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |status| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let current = a.selected(&ui).map(|c| c.name.clone());
            ui.set_parts_busy(false);
            if let Some(name) = &current {
                a.waves.forget(name);
            }
            a.refresh_clips(&ui);
            if let Some(name) = current {
                a.select_name(&ui, &name);
            }
            a.show_transcript(&ui);
            ui.set_status(status);
        }
    });

    ui.on_waves_ready({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || app.borrow().apply_waves(&weak.unwrap())
    });

    ui.on_edit_markup({
        let weak = ui.as_weak();
        move |text| {
            {
                let accent = weak.unwrap().global::<Theme>().get_accent();
                slint::StyledText::from_markdown(&edit_markup(&text, accent))
            }
            .unwrap_or_else(|_| slint::StyledText::from_plain_text(&text))
        }
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

    // Save overwrites the clip: its new words are spoken and its audio is rewritten as
    // the edited result. A copy of the clip as it was goes to the Recycle Bin first.
    ui.on_apply_transcript({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |text| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            if ui.get_speaking() {
                return ui.set_status("Wait for the new words".into());
            }
            let Some(name) = a.selected(&ui).map(|c| c.name.clone()) else {
                return;
            };
            let Ok(mut sc) = Sidecar::load(&a.library.sidecar_path(&name)) else {
                ui.set_status("Transcribe this clip first".into());
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
            a.stop(&ui);
            if let Err(e) = a.library.keep_copy(&name) {
                return ui.set_status(
                    format!("Not saved: no copy of the clip could be kept ({e})").into(),
                );
            }
            if let Err(e) = sc.save(&a.library.sidecar_path(&name)) {
                return ui.set_status(e.to_string().into());
            }
            ui.set_editing(false);
            voice::start(
                &ui,
                &a.library,
                name.clone(),
                a.settings.model.clone(),
                a.settings.language.clone(),
                true,
            );
            a.waves.forget(&name);
            a.refresh_clips(&ui);
            a.select_name(&ui, &name);
            a.show_transcript(&ui);
            ui.set_status(format!("Saving {name}: {}", changes.summary()).into());
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
            let english = ui.get_shown_lang() == "en" && app.borrow().english_words;
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
                    .set_title("Export all the clip's files into a folder")
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
                let result = kind.write(&library, &name, clean_voice, english, &path);
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
                    format!("Next {next}.wav")
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

    ui.on_theme_changed({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |i| {
            let ui = weak.unwrap();
            let theme = ui.global::<Theme>();
            let Some(name) = theme.get_names().row_data(i as usize) else {
                return;
            };
            theme.set_current(i);
            let mut a = app.borrow_mut();
            a.settings.theme = name.into();
            a.settings.save();
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
                let mut a = app.borrow_mut();
                let Some(name) = a.selected(&ui).map(|c| c.name.clone()) else {
                    return;
                };
                a.transcribe(&ui, vec![name], false);
            }
        });
        ui.on_transcribed({
            let (app, weak) = (app.clone(), ui.as_weak());
            move |ok, line| {
                let ui = weak.unwrap();
                let here = {
                    let mut a = app.borrow_mut();
                    let job = a.job.take();
                    let max = a.settings.max_pause;
                    if ok
                        && max > 0.0
                        && let Some((dir, names, false)) = &job
                    {
                        cap_pauses(dir, names, max);
                    }
                    if ok && let Some((dir, names, false)) = &job {
                        let wanted: Vec<String> = names
                            .iter()
                            .filter(|n| a.to_name.iter().any(|(d, m)| d == dir && m == *n))
                            .cloned()
                            .collect();
                        a.to_name.retain(|(d, m)| !(d == dir && wanted.contains(m)));
                        if a.settings.ai_names && a.settings.auto_transcribe && !wanted.is_empty() {
                            name_with_ai(&ui, &a.settings.ai_model, dir.clone(), wanted);
                        }
                    }
                    if ok && job.as_ref().is_some_and(|j| j.2) {
                        // Show what was just made.
                        ui.set_shown_lang("en".into());
                    }
                    job.is_some_and(|(d, _, _)| d == a.library.dir())
                };
                ui.set_status(line);
                if ok && here {
                    // The list marks the clips transcribed, and the card reloads.
                    app.borrow_mut().refresh_clips(&ui);
                    ui.invoke_selection_changed(ui.get_selected());
                }
                let mut a = app.borrow_mut();
                a.next_job(&ui);
                a.show_transcribing(&ui);
            }
        });
        ui.on_choose_lang({
            let (app, weak) = (app.clone(), ui.as_weak());
            move |code| app.borrow_mut().choose_lang(&weak.unwrap(), &code)
        });
        ui.on_lang_filter({
            let (app, weak) = (app.clone(), ui.as_weak());
            move |text| {
                let ui = weak.unwrap();
                let mut a = app.borrow_mut();
                a.lang_filter = text.to_string();
                let sc = a
                    .selected(&ui)
                    .and_then(|c| Sidecar::load(&a.library.sidecar_path(&c.name)).ok());
                a.show_langs(&ui, sc.as_ref());
            }
        });
        ui.on_install_for({
            let weak = ui.as_weak();
            move |kind| {
                let ui = weak.unwrap();
                if kind == 1 {
                    ui.set_status("Download a Whisper model to translate".into());
                    ui.invoke_visit(4);
                } else {
                    ui.set_status("Download the translation model".into());
                    ui.invoke_visit(3);
                }
            }
        });
        ui.on_translated({
            let (app, weak) = (app.clone(), ui.as_weak());
            move |ok, name, _code, text| {
                let ui = weak.unwrap();
                app.borrow_mut().ai_translation = None;
                if ok {
                    ui.set_status(format!("Translated {name}").into());
                } else {
                    ui.set_status(format!("Translation failed: {text}").into());
                }
                app.borrow_mut().show_transcript(&ui);
            }
        });
        ui.on_max_pause_changed({
            let app = app.clone();
            move |i| {
                if let Some((secs, _)) = settings::MAX_PAUSES.get(i as usize) {
                    let mut a = app.borrow_mut();
                    a.settings.max_pause = *secs;
                    a.settings.save();
                }
            }
        });
        ui.on_ai_names_changed({
            let app = app.clone();
            move |on| {
                let mut a = app.borrow_mut();
                a.settings.ai_names = on;
                a.settings.save();
            }
        });
        ui.on_ai_named({
            let (app, weak) = (app.clone(), ui.as_weak());
            move |dir, old, title| {
                let ui = weak.unwrap();
                let mut a = app.borrow_mut();
                // Only in the project shown, and never under the player's feet.
                if a.library.dir() != std::path::Path::new(dir.as_str())
                    || a.active.as_deref() == Some(old.as_str())
                {
                    return;
                }
                let mut to = title.to_string();
                let mut n = 2;
                while a.library.exists(&to) {
                    to = format!("{title} ({n})");
                    n += 1;
                }
                let was = a.selected(&ui).map(|c| c.name.clone());
                match a.library.rename(&old, &to) {
                    Ok(()) => {
                        crate::log!("AI named {old} {to:?}");
                        a.refresh_clips(&ui);
                        if was.as_deref() == Some(old.as_str()) {
                            a.select_name(&ui, &to);
                        }
                        ui.set_status(format!("Named {to}").into());
                    }
                    Err(e) => crate::log!("AI name for {old} failed: {e}"),
                }
            }
        });
        ui.on_auto_transcribe_changed({
            let app = app.clone();
            move |on| {
                let mut a = app.borrow_mut();
                a.settings.auto_transcribe = on;
                a.settings.save();
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

    ui.on_summarise_selected({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let Some(c) = a.selected(&ui).cloned() else {
                return;
            };
            let Some(worker) = ask::worker_path() else {
                return;
            };
            if a.ai_job.is_some() || a.asking || a.ai_translation.is_some() {
                ui.set_status("The AI is busy".into());
                return;
            }
            let model = thrasher_core::ai::find(&a.settings.ai_model)
                .filter(|m| m.is_downloaded())
                .and_then(|m| m.path());
            let Some(model) = model else {
                ui.set_status("Download the AI model first".into());
                ui.invoke_visit(3);
                return;
            };
            let path = a.library.sidecar_path(&c.name);
            let Some(job) = Sidecar::load(&path)
                .ok()
                .and_then(|sc| thrasher_core::ai::summary_job(&sc, &model))
            else {
                ui.set_status("Transcribe this clip first".into());
                return;
            };
            a.ai_job = Some((a.library.dir().to_path_buf(), c.name.clone()));
            ui.set_summarising(true);
            ui.set_show_summary(true);
            crate::log!("summarising {} in {} step(s)", c.name, job.steps.len());
            let (weak, name) = (ui.as_weak(), c.name.clone());
            std::thread::spawn(move || {
                let started = std::time::Instant::now();
                // Shows the summary as it is written, while its clip is the one shown.
                let live = |text: String| {
                    let (weak, name) = (weak.clone(), name.clone());
                    move || {
                        let _ = weak.upgrade_in_event_loop(move |ui| {
                            let shown = ui
                                .get_clips()
                                .row_data(ui.get_selected() as usize)
                                .is_some_and(|r| r.name == name);
                            if shown {
                                ui.set_summary(text.into());
                            }
                        });
                    }
                };
                let result = ask::run(&worker, &job, &mut |_| {}, &mut |text| {
                    live(thrasher_core::ai::tidy(text))()
                });
                crate::log!(
                    "summary of {name} took {} ms: {:?}",
                    crate::log::ms(started),
                    result.as_ref().err()
                );
                let result = result.and_then(|text| {
                    let text = thrasher_core::ai::tidy(&text);
                    let mut sc = Sidecar::load(&path).map_err(|e| e.to_string())?;
                    sc.summary = Some(text.clone());
                    sc.save(&path).map_err(|e| e.to_string())?;
                    Ok(text)
                });
                let ok = result.is_ok();
                let text = result.unwrap_or_else(|e| e);
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    ui.invoke_summarised(ok, name.into(), text.into())
                });
            });
        }
    });

    ui.on_summarised({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |ok, name, text| {
            let ui = weak.unwrap();
            app.borrow_mut().ai_job = None;
            if ok {
                ui.set_status(format!("Summary of {name} ready").into());
            } else {
                ui.set_status(format!("Summary failed: {text}").into());
            }
            app.borrow_mut().show_transcript(&ui);
        }
    });

    ui.on_ask({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |question, all| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let Some(worker) = ask::worker_path() else {
                return;
            };
            if a.ai_job.is_some() || a.asking || a.ai_translation.is_some() {
                ui.set_status("The AI is busy".into());
                return;
            }
            let model = thrasher_core::ai::find(&a.settings.ai_model)
                .filter(|m| m.is_downloaded())
                .and_then(|m| m.path());
            let Some(model) = model else {
                ui.set_status("Download the AI model first".into());
                ui.invoke_visit(3);
                return;
            };
            // Every transcript in this project, or in all of them.
            let current = ui.get_project();
            let places: Vec<i32> = if all {
                (0..ui.get_projects().row_count() as i32).collect()
            } else {
                vec![current]
            };
            let mut all_passages = Vec::new();
            for p in places {
                let Some(name) = App::project_at(&ui, p) else {
                    continue;
                };
                let (library, clips) = if p == current {
                    (a.library.clone(), a.clips.clone())
                } else {
                    let Ok(lib) = Library::open(projects::dir(&a.root, &name)) else {
                        continue;
                    };
                    let clips = lib.list().unwrap_or_default();
                    (lib, clips)
                };
                for c in clips {
                    let Ok(sc) = Sidecar::load(&library.sidecar_path(&c.name)) else {
                        continue;
                    };
                    let created = c
                        .created
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs());
                    all_passages.extend(thrasher_core::ai::passages(&sc, &c.name, p, created));
                }
            }
            let picked = thrasher_core::ai::retrieve(&question, &all_passages);
            let sources: Vec<AskSource> = picked
                .iter()
                .enumerate()
                .map(|(i, p)| AskSource {
                    n: i as i32 + 1,
                    clip: p.clip.as_str().into(),
                    place: p.place,
                    at: p.at as f32,
                    time: fmt_time(p.at).into(),
                    text: p.text.as_str().into(),
                })
                .collect();
            ui.set_sources(ModelRc::new(VecModel::from(sources)));
            if picked.is_empty() {
                ui.set_answer("Nothing in your transcripts matches".into());
                return;
            }
            crate::log!(
                "asking with {} passage(s) from {} in all",
                picked.len(),
                all_passages.len()
            );
            let job = thrasher_core::ai::ask_job(&question, &picked, &model);
            a.asking = true;
            ui.set_asking(true);
            ui.set_answer("".into());
            let weak = ui.as_weak();
            std::thread::spawn(move || {
                let started = std::time::Instant::now();
                let w = weak.clone();
                let result = ask::run(&worker, &job, &mut |_| {}, &mut |text| {
                    let text = thrasher_core::ai::tidy(text);
                    let _ = w.upgrade_in_event_loop(move |ui| ui.set_answer(text.into()));
                });
                crate::log!(
                    "answer took {} ms: {:?}",
                    crate::log::ms(started),
                    result.as_ref().err()
                );
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    match result {
                        Ok(text) => ui.set_answer(thrasher_core::ai::tidy(&text).into()),
                        Err(e) => ui.set_answer(format!("The AI failed: {e}").into()),
                    }
                    ui.invoke_answered();
                });
            });
        }
    });

    ui.on_answered({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            app.borrow_mut().asking = false;
            weak.unwrap().set_asking(false);
        }
    });

    ui.on_open_source({
        let (app, weak, ensure) = (app.clone(), ui.as_weak(), ensure_timer.clone());
        move |i| {
            let ui = weak.unwrap();
            let Some(s) = ui.get_sources().row_data(i as usize) else {
                return;
            };
            let mut a = app.borrow_mut();
            if s.place != ui.get_project() {
                if a.recording.is_some() {
                    return;
                }
                let Some(name) = App::project_at(&ui, s.place) else {
                    return;
                };
                a.open_project(&ui, &name);
            }
            a.select_name(&ui, &s.clip);
            if a.play_clip(&ui, &s.clip, s.at as f64) {
                ensure();
            }
        }
    });

    ui.on_ai_model_changed({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |i| {
            let Some(m) = thrasher_core::ai::AI_MODELS.get(i as usize) else {
                return;
            };
            let mut a = app.borrow_mut();
            a.settings.ai_model = m.id.to_string();
            a.settings.save();
            show_ai_model(&weak.unwrap(), m.id);
        }
    });

    ui.on_download_ai({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |which| {
            let ui = weak.unwrap();
            let id = if which == 1 {
                thrasher_core::ai::TRANSLATOR.to_string()
            } else {
                app.borrow().settings.ai_model.clone()
            };
            let Some(model) = thrasher_core::ai::find(&id) else {
                return;
            };
            let summary_model = app.borrow().settings.ai_model.clone();
            let set = move |ui: &AppWindow, p: f32| {
                if which == 1 {
                    ui.set_translator_download(p)
                } else {
                    ui.set_ai_download(p)
                }
            };
            set(&ui, 0.0);
            let weak = ui.as_weak();
            std::thread::spawn(move || {
                let cancel = std::sync::atomic::AtomicBool::new(false);
                let mut last = 0.0f32;
                let w = weak.clone();
                let result = thrasher_core::models::download(
                    model,
                    &mut |done, total| {
                        let p = done as f32 / total.max(1) as f32;
                        if p - last >= 0.005 {
                            last = p;
                            let _ = w.upgrade_in_event_loop(move |ui| set(&ui, p));
                        }
                    },
                    &cancel,
                );
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    set(&ui, -1.0);
                    match result {
                        Ok(_) => ui.set_status(format!("Downloaded {}", model.name).into()),
                        Err(e) => ui.set_status(format!("{}: {e}", model.name).into()),
                    }
                    show_ai_model(&ui, &summary_model);
                });
            });
        }
    });

    ui.on_search({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |query, all| app.borrow_mut().search(&weak.unwrap(), &query, all)
    });

    ui.on_open_hit({
        let (app, weak, ensure) = (app.clone(), ui.as_weak(), ensure_timer.clone());
        move |i| {
            let ui = weak.unwrap();
            let Some(hit) = ui.get_hits().row_data(i as usize) else {
                return;
            };
            let mut a = app.borrow_mut();
            if hit.project_index != ui.get_project() {
                if a.recording.is_some() {
                    return;
                }
                let Some(name) = App::project_at(&ui, hit.project_index) else {
                    return;
                };
                a.open_project(&ui, &name);
            }
            a.select_name(&ui, &hit.clip);
            if a.play_clip(&ui, &hit.clip, hit.at as f64) {
                ensure();
            }
        }
    });

    ui.on_choose_project({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |i| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            if a.recording.is_some() {
                return;
            }
            if let Some(name) = App::project_at(&ui, i)
                && name != a.project
            {
                a.open_project(&ui, &name);
            }
        }
    });

    ui.on_create_project({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |name| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let name = name.trim().to_string();
            match projects::create(&a.root, &name) {
                Ok(_) if a.recording.is_none() => a.open_project(&ui, &name),
                Ok(_) => a.show_projects(&ui),
                Err(e) => ui.set_status(format!("Cannot make {name}: {e}").into()),
            }
        }
    });

    ui.on_rename_project({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |name| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let (from, to) = (a.project.clone(), name.trim().to_string());
            if a.recording.is_some() {
                return;
            }
            match projects::rename(&a.root, &from, &to) {
                Ok(()) => a.open_project(&ui, &to),
                Err(e) => ui.set_status(format!("Cannot rename {from}: {e}").into()),
            }
        }
    });

    ui.on_delete_project({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let name = a.project.clone();
            if a.recording.is_some() {
                return;
            }
            match projects::remove(&a.root, &name) {
                Ok(()) => {
                    a.open_project(&ui, "");
                    ui.set_status(format!("Deleted {name}").into());
                }
                Err(e) => ui.set_status(format!("Cannot delete {name}: {e}").into()),
            }
        }
    });

    ui.on_move_clip({
        let (app, weak) = (app.clone(), ui.as_weak());
        move |i, p| {
            let ui = weak.unwrap();
            let mut a = app.borrow_mut();
            let (Some(c), Some(target)) =
                (a.clips.get(i as usize).cloned(), App::project_at(&ui, p))
            else {
                return;
            };
            if target == a.project || a.recording.as_ref().is_some_and(|r| r.name() == c.name) {
                return;
            }
            if a.active.as_deref() == Some(c.name.as_str()) {
                a.stop(&ui);
                a.active = None;
            }
            let to = Library::open(projects::dir(&a.root, &target)).map_err(|e| e.to_string());
            match to.and_then(|to| a.library.move_to(&c.name, &to).map_err(|e| e.to_string())) {
                Ok(_) => {
                    let shown = if target.is_empty() {
                        projects::INBOX
                    } else {
                        target.as_str()
                    };
                    ui.set_status(format!("Moved {} to {shown}", c.name).into());
                    a.refresh_clips(&ui);
                    ui.set_selected(-1);
                    a.show_transcript(&ui);
                }
                Err(e) => ui.set_status(format!("Cannot move {}: {e}", c.name).into()),
            }
        }
    });

    ui.on_export_project({
        let (app, weak) = (app.clone(), ui.as_weak());
        move || {
            let ui = weak.unwrap();
            let (library, project, clean_voice, format, folder) = {
                let a = app.borrow();
                let shown = if a.project.is_empty() {
                    projects::INBOX
                } else {
                    a.project.as_str()
                };
                (
                    a.library.clone(),
                    shown.to_string(),
                    a.settings.clean_voice,
                    a.settings.export_format.clone(),
                    a.settings.export_dir.clone(),
                )
            };
            use thrasher_core::export::{ExportOptions, Format};
            let format = match format.as_str() {
                "wav" => Format::Wav,
                "mp3" => Format::Mp3,
                _ => Format::Flac,
            };
            let options = ExportOptions {
                format,
                bits: if format == Format::Mp3 { 16 } else { 24 },
                clean_voice,
            };
            let start = folder
                .filter(|d| d.is_dir())
                .unwrap_or_else(|| library.dir().to_path_buf());
            // The test window must never take focus, so it exports without asking.
            let chosen = if std::env::var_os("THRASHER_TEST_WINDOW").is_some() {
                Some(start)
            } else {
                rfd::FileDialog::new()
                    .set_title(format!("Export {project} into a folder"))
                    .set_directory(&start)
                    .pick_folder()
            };
            let Some(parent) = chosen else {
                return;
            };
            {
                let mut a = app.borrow_mut();
                a.settings.export_dir = Some(parent.clone());
                a.settings.save();
            }
            ui.set_status(format!("Exporting {project}...").into());
            let weak = ui.as_weak();
            std::thread::spawn(move || {
                let started = std::time::Instant::now();
                let result = projects::export_all(&library, &project, options, &parent);
                crate::log!(
                    "export of project {project} took {} ms: {:?}",
                    crate::log::ms(started),
                    result.as_ref().err()
                );
                let status = match result {
                    Ok(dir) => format!("Exported {project} into {}", short_path(&dir)),
                    Err(e) => format!("Export failed: {e}"),
                };
                let _ = weak.upgrade_in_event_loop(move |ui| ui.set_status(status.into()));
            });
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
                Ok(_) => {
                    a.root = dir.clone();
                    a.settings.output_dir = dir.clone();
                    ui.set_folder(short_path(&dir).into());
                    // A new folder starts in its Inbox.
                    a.open_project(&ui, "");
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

    if !ui.get_compact()
        && let Some(place) = app.borrow().settings.window
    {
        restore_window(&ui, place);
    }
    ui.show()?;
    let _remember = remember_window(&ui, &app);
    let _drops = accept_drops(&ui, &app);
    let _cover = widget_when_hidden(&ui, &app);
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
/// Imports `files` into `library` on a worker thread; the window's `imported`
/// callback shows the result.
fn import_paths(ui: &AppWindow, library: Library, files: Vec<std::path::PathBuf>) {
    if files.is_empty() {
        return;
    }
    ui.set_status(format!("Importing {} file(s)...", files.len()).into());
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let (mut names, mut failed) = (Vec::new(), Vec::new());
        for f in &files {
            match thrasher_core::import::import_file(&library, f) {
                Ok(name) => names.push(name),
                Err(e) => {
                    crate::log!("import {} failed: {e}", f.display());
                    let file = f.file_name().unwrap_or_default().to_string_lossy();
                    failed.push(format!("{file}: {e}"));
                }
            }
        }
        crate::log!(
            "imported {} of {} file(s) in {} ms",
            names.len(),
            files.len(),
            crate::log::ms(started)
        );
        let status = match (names.len(), failed.first()) {
            (_, Some(f)) if failed.len() == 1 => format!("Could not import {f}"),
            (_, Some(_)) => format!("{} file(s) could not be imported", failed.len()),
            (1, None) => format!("Imported {}", names[0]),
            (n, None) => format!("Imported {n} clips"),
        };
        let last = names.last().cloned().unwrap_or_default();
        let names = names.join("\n");
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.invoke_imported(status.into(), last.into(), names.into())
        });
    });
}

/// Asks the local AI for a name for each of `names` (just transcribed, in `dir`), one
/// after another on a worker thread; the window's `ai-named` callback renames them.
/// Does nothing without the AI worker or its model.
fn name_with_ai(ui: &AppWindow, model_id: &str, dir: std::path::PathBuf, names: Vec<String>) {
    let (Some(worker), Some(model)) = (
        ask::worker_path(),
        thrasher_core::ai::find(model_id)
            .filter(|m| m.is_downloaded())
            .and_then(|m| m.path()),
    ) else {
        return;
    };
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let Ok(library) = Library::open(&dir) else {
            return;
        };
        for name in names {
            let Some(job) = Sidecar::load(&library.sidecar_path(&name))
                .ok()
                .and_then(|sc| thrasher_core::ai::name_job(&sc, &model))
            else {
                continue;
            };
            let title = ask::run(&worker, &job, &mut |_| {}, &mut |_| {})
                .ok()
                .and_then(|t| thrasher_core::ai::clean_title(&t));
            let Some(title) = title else { continue };
            let d = dir.to_string_lossy().into_owned();
            let _ = weak.upgrade_in_event_loop(move |ui| {
                ui.invoke_ai_named(d.into(), name.into(), title.into())
            });
        }
    });
}

/// Fills the AI model choice in settings and whether that model is downloaded.
fn show_ai_model(ui: &AppWindow, id: &str) {
    use thrasher_core::ai::AI_MODELS;
    let names: Vec<SharedString> = AI_MODELS
        .iter()
        .map(|m| format!("{} · {:.1} GB", m.name, m.bytes as f64 / 1e9).into())
        .collect();
    ui.set_ai_models(ModelRc::new(VecModel::from(names)));
    let i = AI_MODELS.iter().position(|m| m.id == id).unwrap_or(0);
    ui.set_ai_model_index(i as i32);
    ui.set_ai_ready(AI_MODELS[i].is_downloaded());
    ui.set_translator_ready(
        thrasher_core::ai::find(thrasher_core::ai::TRANSLATOR).is_some_and(|m| m.is_downloaded()),
    );
}

/// Shortens the pauses of freshly transcribed clips to `max` seconds (the Max pause
/// setting), as an edit the editor can change back.
fn cap_pauses(dir: &std::path::Path, names: &[String], max: f64) {
    let Ok(library) = Library::open(dir) else {
        return;
    };
    for name in names {
        let path = library.sidecar_path(name);
        let Ok(mut sc) = Sidecar::load(&path) else {
            continue;
        };
        let n = edit::cap_pauses(&mut sc, max);
        if n > 0 && sc.save(&path).is_ok() {
            crate::log!("shortened {n} pause(s) in {name} to {max} s");
        }
    }
}

/// True for a file the importer offers to read, judged by its extension.
fn importable(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        thrasher_core::import::EXTENSIONS
            .iter()
            .any(|x| x.eq_ignore_ascii_case(e))
    })
}

/// Files dropped on the window are imported into the current project, a moment after
/// the last one lands (Windows hands them over one at a time). While files hover, the
/// window says so.
fn accept_drops(ui: &AppWindow, app: &Rc<RefCell<App>>) -> Rc<Timer> {
    use slint::winit_030::winit::event::WindowEvent;
    use slint::winit_030::{EventResult, WinitWindowAccessor};
    let dropped: Rc<RefCell<Vec<std::path::PathBuf>>> = Rc::default();
    let timer = Rc::new(Timer::default());
    let (weak, app, t) = (ui.as_weak(), app.clone(), timer.clone());
    ui.window().on_winit_window_event(move |_, event| {
        let Some(ui) = weak.upgrade() else {
            return EventResult::Propagate;
        };
        match event {
            // Alt-Tab or the taskbar back to an automatic widget brings the window
            // back; a click on the widget itself (the mouse is over it) does not.
            WindowEvent::Focused(true)
                if ui.get_compact() && app.borrow().auto_widget && !cursor_over(&ui) =>
            {
                crate::log!("back to the window from the widget");
                ui.invoke_compact_changed(false);
            }
            WindowEvent::HoveredFile(_) => ui.set_dropping(true),
            WindowEvent::HoveredFileCancelled => ui.set_dropping(false),
            WindowEvent::DroppedFile(path) => {
                ui.set_dropping(false);
                dropped.borrow_mut().push(path.clone());
                let (weak, app, dropped) = (ui.as_weak(), app.clone(), dropped.clone());
                t.start(
                    TimerMode::SingleShot,
                    Duration::from_millis(150),
                    move || {
                        let files = std::mem::take(&mut *dropped.borrow_mut());
                        let Some(ui) = weak.upgrade() else { return };
                        let (files, skipped): (Vec<_>, Vec<_>) =
                            files.into_iter().partition(|f| importable(f));
                        crate::log!("dropped {} file(s), {} skipped", files.len(), skipped.len());
                        if files.is_empty() {
                            ui.set_status("Thrasher cannot import that".into());
                            return;
                        }
                        if !ui.get_editing() && ui.get_page() != 0 {
                            ui.set_page(0);
                        }
                        let library = app.borrow().library.clone();
                        import_paths(&ui, library, files);
                    },
                );
            }
            _ => {}
        }
        EventResult::Propagate
    });
    timer
}

/// While recording, the window turns into the widget when it is minimised or when
/// another window covers it (maximised, or over most of it), so the recording stays in
/// sight. A window beside another one, without focus, stays as it is.
fn widget_when_hidden(ui: &AppWindow, app: &Rc<RefCell<App>>) -> Timer {
    let timer = Timer::default();
    let (weak, app) = (ui.as_weak(), app.clone());
    timer.start(TimerMode::Repeated, Duration::from_millis(400), move || {
        let Some(ui) = weak.upgrade() else { return };
        if ui.get_compact() || !ui.get_recording() || app.borrow().recording.is_none() {
            return;
        }
        let minimised = ui.window().is_minimized();
        if !minimised && !covered(&ui) {
            return;
        }
        crate::log!(
            "recording window {}: showing the widget",
            if minimised { "minimised" } else { "covered" }
        );
        if minimised {
            ui.window().set_minimized(false);
        }
        ui.invoke_compact_changed(true);
        app.borrow_mut().auto_widget = true;
    });
    timer
}

#[cfg(windows)]
mod cover {
    #[repr(C)]
    #[derive(Default)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }
    #[repr(C)]
    #[derive(Default)]
    pub struct Point {
        pub x: i32,
        pub y: i32,
    }
    unsafe extern "system" {
        pub fn GetForegroundWindow() -> isize;
        pub fn GetWindowRect(hwnd: isize, rect: *mut Rect) -> i32;
        pub fn IsZoomed(hwnd: isize) -> i32;
        pub fn IsIconic(hwnd: isize) -> i32;
        pub fn MonitorFromWindow(hwnd: isize, flags: u32) -> isize;
        pub fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
        pub fn GetCursorPos(p: *mut Point) -> i32;
    }

    pub fn rect(hwnd: isize) -> Option<Rect> {
        let mut r = Rect::default();
        // SAFETY: `r` is a valid RECT to write into; a bad handle just returns 0.
        (unsafe { GetWindowRect(hwnd, &mut r) } != 0).then_some(r)
    }
}

/// Thrasher's own window handle.
#[cfg(windows)]
fn own_hwnd(ui: &AppWindow) -> Option<isize> {
    use slint::winit_030::WinitWindowAccessor;
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    ui.window()
        .with_winit_window(|w| match w.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
            _ => None,
        })
        .flatten()
}

/// True when the window in front is another app's and is maximised on Thrasher's
/// screen or covers most of Thrasher.
#[cfg(windows)]
fn covered(ui: &AppWindow) -> bool {
    use cover::*;
    let Some(own) = own_hwnd(ui) else {
        return false;
    };
    // SAFETY: plain Win32 queries on window handles; no memory is handed over.
    unsafe {
        let front = GetForegroundWindow();
        if front == 0 || front == own || IsIconic(front) != 0 {
            return false;
        }
        let (mut a, mut b) = (0u32, 0u32);
        GetWindowThreadProcessId(front, &mut a);
        GetWindowThreadProcessId(own, &mut b);
        if a == b {
            return false;
        }
        // MONITOR_DEFAULTTONEAREST.
        if IsZoomed(front) != 0 && MonitorFromWindow(front, 2) == MonitorFromWindow(own, 2) {
            return true;
        }
        let (Some(f), Some(o)) = (rect(front), rect(own)) else {
            return false;
        };
        let w = (f.right.min(o.right) - f.left.max(o.left)).max(0) as i64;
        let h = (f.bottom.min(o.bottom) - f.top.max(o.top)).max(0) as i64;
        let area = ((o.right - o.left) as i64 * (o.bottom - o.top) as i64).max(1);
        w * h * 10 >= area * 8
    }
}

#[cfg(not(windows))]
fn covered(_: &AppWindow) -> bool {
    false
}

/// True when the mouse is over Thrasher's window.
#[cfg(windows)]
fn cursor_over(ui: &AppWindow) -> bool {
    use cover::*;
    let Some(own) = own_hwnd(ui) else {
        return false;
    };
    let mut p = Point::default();
    // SAFETY: `p` is a valid POINT to write into.
    if unsafe { GetCursorPos(&mut p) } == 0 {
        return false;
    }
    rect(own).is_some_and(|r| p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom)
}

#[cfg(not(windows))]
fn cursor_over(_: &AppWindow) -> bool {
    false
}

/// Opens the window at its last size, and at its last place when that is still on a
/// screen (a monitor may have been unplugged since).
fn restore_window(ui: &AppWindow, place: settings::WindowPlace) {
    if place.width >= 300.0 && place.height >= 200.0 {
        ui.window()
            .set_size(LogicalSize::new(place.width, place.height));
    }
    if on_screen(place.x, place.y) {
        ui.window()
            .set_position(slint::PhysicalPosition::new(place.x, place.y));
    }
}

/// True when the point lies on the desktop, all monitors together.
#[cfg(windows)]
fn on_screen(x: i32, y: i32) -> bool {
    unsafe extern "system" {
        fn GetSystemMetrics(index: i32) -> i32;
    }
    // SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN.
    // SAFETY: GetSystemMetrics takes a plain index and has no other requirements.
    let [left, top, w, h] = [76, 77, 78, 79].map(|i| unsafe { GetSystemMetrics(i) });
    // Keep at least a strip of the title bar reachable.
    x >= left - 100 && x < left + w - 100 && y >= top && y < top + h - 40
}

#[cfg(not(windows))]
fn on_screen(_: i32, _: i32) -> bool {
    true
}

/// Saves the full window's size and place a moment after it changes. The widget and a
/// minimized window are left out, so they never become the size to come back to.
fn remember_window(ui: &AppWindow, app: &Rc<RefCell<App>>) -> Timer {
    let timer = Timer::default();
    let (weak, app) = (ui.as_weak(), app.clone());
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        let Some(ui) = weak.upgrade() else { return };
        let w = ui.window();
        if ui.get_compact() || w.is_minimized() || !w.is_visible() {
            return;
        }
        let scale = w.scale_factor();
        let (size, pos) = (w.size(), w.position());
        let place = settings::WindowPlace {
            x: pos.x,
            y: pos.y,
            width: (size.width as f32 / scale).round(),
            height: (size.height as f32 / scale).round(),
        };
        // Windows parks minimized windows far off screen.
        if pos.x <= -30000 || pos.y <= -30000 {
            return;
        }
        let mut a = app.borrow_mut();
        if a.settings.window != Some(place) {
            a.settings.window = Some(place);
            a.settings.save();
        }
    });
    timer
}

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

    fn picks(picked: &[bool]) -> Vec<usize> {
        (0..picked.len()).filter(|&i| picked[i]).collect()
    }

    #[test]
    fn picks_clips_like_file_explorer() {
        let mut p = vec![false; 8];
        let mut anchor = None;
        let mut sel = pick_clips(&mut p, &mut anchor, -1, 2, false, false);
        assert_eq!((sel, picks(&p)), (2, vec![]));
        // Shift: range from the anchor; a second Shift click keeps the anchor.
        sel = pick_clips(&mut p, &mut anchor, sel, 5, false, true);
        assert_eq!((sel, picks(&p)), (5, vec![2, 3, 4, 5]));
        sel = pick_clips(&mut p, &mut anchor, sel, 0, false, true);
        assert_eq!((sel, picks(&p)), (0, vec![0, 1, 2]));
        // Ctrl adds one and moves the anchor there.
        sel = pick_clips(&mut p, &mut anchor, sel, 6, true, false);
        assert_eq!((sel, picks(&p)), (6, vec![0, 1, 2, 6]));
        // Ctrl+Shift adds the range from the new anchor.
        sel = pick_clips(&mut p, &mut anchor, sel, 7, true, true);
        assert_eq!((sel, picks(&p)), (7, vec![0, 1, 2, 6, 7]));
        // Ctrl on a picked clip removes it; the highlight moves to a picked one.
        sel = pick_clips(&mut p, &mut anchor, sel, 7, true, false);
        assert_eq!((sel, picks(&p)), (0, vec![0, 1, 2, 6]));
        // A plain click picks just that one.
        sel = pick_clips(&mut p, &mut anchor, sel, 4, false, false);
        assert_eq!((sel, picks(&p)), (4, vec![]));
        // Ctrl from a lone highlighted clip makes two.
        sel = pick_clips(&mut p, &mut anchor, sel, 1, true, false);
        assert_eq!((sel, picks(&p)), (1, vec![1, 4]));
        // Moving the highlight with the keys moves the anchor too.
        sel = pick_clips(&mut p, &mut anchor, sel, 3, false, false);
        assert_eq!(sel, 3);
        sel = pick_clips(&mut p, &mut anchor, 6, 7, false, true);
        assert_eq!((sel, picks(&p)), (7, vec![6, 7]));
    }

    const ORANGE: slint::Color = slint::Color::from_rgb_u8(0xff, 0x6a, 0x1a);

    #[test]
    fn markup_colours_tags_and_escapes_the_rest() {
        assert_eq!(
            edit_markup("Hei* [silence:2s] der", ORANGE),
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
            let styled = slint::StyledText::from_markdown(&edit_markup(text, ORANGE));
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
