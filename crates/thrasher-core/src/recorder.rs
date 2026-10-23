//! A recording session: one thread per source, one writer thread that drains the ring
//! buffers into WAV files, with pause, live levels and a sidecar.
//!
//! The first track is the master clock. Other tracks are lined up with it using the
//! device timestamps of their first frames, and padded to its length on stop.

use crate::capture::{Source, SourceCtx, SourceFormat, SourceShared};
use crate::library::Library;
use crate::meter::{Level, SharedLevel};
use crate::sidecar::{Meta, RecordingState, Sidecar, Track, TrackKind, VERSION};
use crate::wav::WavWriter;
use rtrb::Consumer;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const DRAIN_EVERY: Duration = Duration::from_millis(15);
/// How long a non-master track waits for the master's first timestamp before it is
/// written without alignment.
const ALIGN_WAIT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub struct RecorderOptions {
    /// How often the WAV headers are patched and data is flushed to disk.
    pub sync_every: Duration,
    /// How long to wait for a device to open.
    pub open_timeout: Duration,
}

impl Default for RecorderOptions {
    fn default() -> Self {
        Self {
            sync_every: Duration::from_secs(1),
            open_timeout: Duration::from_secs(5),
        }
    }
}

struct Status {
    paused: AtomicBool,
    stop_writer: AtomicBool,
    master_frames: AtomicU64,
    error: Mutex<Option<String>>,
}

pub struct TrackStatus {
    pub kind: TrackKind,
    pub level: Arc<SharedLevel>,
    pub shared: Arc<SourceShared>,
    pub format: SourceFormat,
}

/// A recording in progress.
pub struct Recording {
    name: String,
    master_rate: u32,
    stop_sources: Arc<AtomicBool>,
    sources: Vec<JoinHandle<()>>,
    writer: Option<JoinHandle<Result<f64, String>>>,
    status: Arc<Status>,
    tracks: Vec<TrackStatus>,
    started: Instant,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Finished {
    pub name: String,
    pub duration: f64,
    /// Problems worth showing, such as dropped samples or a device error.
    pub warnings: Vec<String>,
}

struct WriterTrack {
    consumer: Consumer<f32>,
    wav: WavWriter,
    format: SourceFormat,
    level: Arc<SharedLevel>,
    shared: Arc<SourceShared>,
    aligned: bool,
    skip: u64,
}

fn kind_name(k: TrackKind) -> &'static str {
    match k {
        TrackKind::Mic => "mic",
        TrackKind::System => "system",
    }
}

impl Recording {
    /// Opens every source, creates the files and starts writing.
    pub fn start(
        library: &Library,
        name: &str,
        sources: Vec<(TrackKind, Box<dyn Source>)>,
        options: RecorderOptions,
    ) -> Result<Self, String> {
        if sources.is_empty() {
            return Err("nothing to record: no source selected".into());
        }
        let stop_sources = Arc::new(AtomicBool::new(false));
        let mut handles = Vec::new();
        let mut pending = Vec::new();
        for (kind, source) in sources {
            let shared = Arc::new(SourceShared::default());
            let (tx, rx) = sync_channel(1);
            let ctx = SourceCtx::new(stop_sources.clone(), shared.clone(), tx.clone());
            let sh = shared.clone();
            let handle = std::thread::Builder::new()
                .name(format!("thrasher-capture-{}", kind_name(kind)))
                .spawn(move || {
                    if let Err(e) = source.run(ctx) {
                        let _ = tx.try_send(Err(e.clone()));
                        *sh.error.lock().unwrap() = Some(e);
                    }
                })
                .map_err(|e| e.to_string())?;
            handles.push(handle);
            pending.push((kind, shared, rx));
        }

        let fail = |handles: Vec<JoinHandle<()>>, msg: String| {
            stop_sources.store(true, Ordering::Relaxed);
            for h in handles {
                let _ = h.join();
            }
            Err(msg)
        };

        let mut opened = Vec::new();
        for (kind, shared, rx) in pending {
            match rx.recv_timeout(options.open_timeout) {
                Ok(Ok((format, consumer))) => opened.push((kind, shared, format, consumer)),
                Ok(Err(e)) => {
                    return fail(handles, format!("{} could not start: {e}", kind_name(kind)));
                }
                Err(_) => {
                    return fail(
                        handles,
                        format!("{} did not start in time", kind_name(kind)),
                    );
                }
            }
        }

        let mut writer_tracks = Vec::new();
        let mut track_status = Vec::new();
        let mut sidecar_tracks = Vec::new();
        for (i, (kind, shared, format, consumer)) in opened.into_iter().enumerate() {
            let path = library.track_path(name, kind);
            let wav = match WavWriter::create(&path, format.sample_rate, format.channels) {
                Ok(w) => w,
                Err(e) => return fail(handles, format!("cannot create {}: {e}", path.display())),
            };
            let level = Arc::new(SharedLevel::default());
            sidecar_tracks.push(Track {
                kind,
                file: path.file_name().unwrap().to_string_lossy().into_owned(),
                sample_rate: format.sample_rate,
                channels: format.channels,
                device: shared.label.lock().unwrap().clone(),
            });
            track_status.push(TrackStatus {
                kind,
                level: level.clone(),
                shared: shared.clone(),
                format,
            });
            writer_tracks.push(WriterTrack {
                consumer,
                wav,
                format,
                level,
                shared,
                aligned: i == 0,
                skip: 0,
            });
        }

        let sidecar_path = library.sidecar_path(name);
        let mut sidecar = Sidecar {
            file: sidecar_tracks[0].file.clone(),
            duration: 0.0,
            thrasher: Meta {
                version: VERSION,
                state: RecordingState::Recording,
                created: chrono::Local::now().to_rfc3339(),
                tracks: sidecar_tracks,
            },
            ..Default::default()
        };
        if let Err(e) = sidecar.save(&sidecar_path) {
            return fail(handles, format!("cannot write the sidecar: {e}"));
        }

        let status = Arc::new(Status {
            paused: AtomicBool::new(false),
            stop_writer: AtomicBool::new(false),
            master_frames: AtomicU64::new(0),
            error: Mutex::new(None),
        });
        let st = status.clone();
        let sync_every = options.sync_every;
        let writer = std::thread::Builder::new()
            .name("thrasher-writer".into())
            .spawn(move || {
                let result = write_loop(&mut writer_tracks, &st, sync_every);
                let duration = writer_tracks[0].wav.duration_secs();
                let finish = finish_tracks(writer_tracks);
                sidecar.duration = duration;
                sidecar.thrasher.state = RecordingState::Complete;
                let saved = sidecar.save(&sidecar_path).map_err(|e| e.to_string());
                result.and(finish).and(saved).map(|_| duration)
            })
            .map_err(|e| e.to_string())?;

        Ok(Self {
            name: name.to_string(),
            master_rate: track_status[0].format.sample_rate,
            stop_sources,
            sources: handles,
            writer: Some(writer),
            status,
            tracks: track_status,
            started: Instant::now(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn pause(&self) {
        self.status.paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.status.paused.store(false, Ordering::Relaxed);
    }

    pub fn is_paused(&self) -> bool {
        self.status.paused.load(Ordering::Relaxed)
    }

    /// Recorded length so far, not counting pauses.
    pub fn elapsed(&self) -> f64 {
        self.status.master_frames.load(Ordering::Relaxed) as f64 / self.master_rate as f64
    }

    /// Wall-clock time since start, including pauses.
    pub fn wall_time(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn tracks(&self) -> &[TrackStatus] {
        &self.tracks
    }

    pub fn levels(&self) -> Vec<(TrackKind, Level)> {
        self.tracks
            .iter()
            .map(|t| (t.kind, t.level.load()))
            .collect()
    }

    /// The first error from a device or the writer, if any. Recording continues on the
    /// other tracks when one device fails.
    pub fn error(&self) -> Option<String> {
        if let Some(e) = self.status.error.lock().unwrap().clone() {
            return Some(e);
        }
        self.tracks.iter().find_map(|t| {
            t.shared
                .error
                .lock()
                .unwrap()
                .clone()
                .map(|e| format!("{}: {e}", kind_name(t.kind)))
        })
    }

    /// Stops the devices, writes what is left and closes the files.
    pub fn stop(mut self) -> Result<Finished, String> {
        self.stop_sources.store(true, Ordering::Relaxed);
        for h in self.sources.drain(..) {
            let _ = h.join();
        }
        self.status.stop_writer.store(true, Ordering::Relaxed);
        let duration = self
            .writer
            .take()
            .unwrap()
            .join()
            .map_err(|_| "the writer thread crashed".to_string())??;
        let mut warnings = Vec::new();
        for t in &self.tracks {
            let lost = t.shared.overruns.load(Ordering::Relaxed);
            if lost > 0 {
                warnings.push(format!(
                    "{}: {:.2} s of audio was dropped because the disk could not keep up",
                    kind_name(t.kind),
                    lost as f64 / (t.format.sample_rate as f64 * t.format.channels as f64)
                ));
            }
            if let Some(e) = t.shared.error.lock().unwrap().clone() {
                warnings.push(format!("{}: {e}", kind_name(t.kind)));
            }
        }
        Ok(Finished {
            name: self.name.clone(),
            duration,
            warnings,
        })
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        // Dropped without stop(): still close the files properly.
        self.stop_sources.store(true, Ordering::Relaxed);
        self.status.stop_writer.store(true, Ordering::Relaxed);
        for h in self.sources.drain(..) {
            let _ = h.join();
        }
        if let Some(w) = self.writer.take() {
            let _ = w.join();
        }
    }
}

fn write_loop(tracks: &mut [WriterTrack], st: &Status, sync_every: Duration) -> Result<(), String> {
    let begin = Instant::now();
    let mut last_sync = Instant::now();
    loop {
        // Read the flag before draining, so the final drain sees everything the
        // sources pushed before they were joined.
        let stopping = st.stop_writer.load(Ordering::Relaxed);
        let paused = st.paused.load(Ordering::Relaxed);
        let master_start = tracks[0].shared.start_time.load(Ordering::Relaxed);
        let waited = begin.elapsed() > ALIGN_WAIT;
        for (i, t) in tracks.iter_mut().enumerate() {
            if !t.aligned {
                let own = t.shared.start_time.load(Ordering::Relaxed);
                if own == 0 && t.consumer.slots() > 0 {
                    // The source gives no timestamps: nothing to align by.
                    t.aligned = true;
                } else if own != 0 && (master_start != 0 || waited || stopping) {
                    if master_start != 0 {
                        let rate = t.format.sample_rate as i128;
                        let offset = (own as i128 - master_start as i128) * rate / 10_000_000;
                        if offset > 0 {
                            t.wav
                                .write_silence(offset as u64)
                                .map_err(|e| e.to_string())?;
                        } else {
                            t.skip = (-offset) as u64 * t.format.channels as u64;
                        }
                    }
                    t.aligned = true;
                }
                if !t.aligned {
                    continue;
                }
            }
            drain(t, paused).map_err(|e| e.to_string())?;
            if i == 0 {
                st.master_frames.store(t.wav.frames(), Ordering::Relaxed);
            }
        }
        if stopping {
            return Ok(());
        }
        if last_sync.elapsed() >= sync_every {
            for t in tracks.iter_mut() {
                if let Err(e) = t.wav.sync() {
                    *st.error.lock().unwrap() = Some(format!("disk write failed: {e}"));
                }
            }
            last_sync = Instant::now();
        }
        std::thread::sleep(DRAIN_EVERY);
    }
}

fn drain(t: &mut WriterTrack, paused: bool) -> std::io::Result<()> {
    let n = t.consumer.slots();
    if n == 0 {
        return Ok(());
    }
    let chunk = t.consumer.read_chunk(n).expect("slots were counted");
    let (a, b) = chunk.as_slices();
    for part in [a, b] {
        let mut part = part;
        if t.skip > 0 {
            let k = (t.skip as usize).min(part.len());
            part = &part[k..];
            t.skip -= k as u64;
        }
        if part.is_empty() {
            continue;
        }
        t.level.store(part);
        if !paused {
            t.wav.write(part)?;
        }
    }
    chunk.commit_all();
    Ok(())
}

fn finish_tracks(tracks: Vec<WriterTrack>) -> Result<(), String> {
    let master_secs = tracks[0].wav.duration_secs();
    let mut first_err = None;
    for (i, mut t) in tracks.into_iter().enumerate() {
        t.level.reset();
        let target = (master_secs * t.format.sample_rate as f64).round() as u64;
        if i > 0 && t.wav.frames() < target {
            let _ = t.wav.write_silence(target - t.wav.frames());
        }
        if let Err(e) = t.wav.finalize() {
            first_err.get_or_insert(e.to_string());
        }
    }
    first_err.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::FakeSource;
    use crate::wav;

    fn fake(rate: u32, ch: u16, frames: u64, start: Option<u64>) -> Box<dyn Source> {
        let mut f = FakeSource::tone(rate, ch);
        f.total_frames = Some(frames);
        f.realtime = false;
        f.start_time = start;
        Box::new(f)
    }

    fn wait_for(rec: &Recording, secs: f64) {
        let t = Instant::now();
        while rec.elapsed() < secs && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn records_one_track() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        let rec = Recording::start(
            &lib,
            "clip_001",
            vec![(TrackKind::Mic, fake(8000, 1, 8000, None))],
            RecorderOptions::default(),
        )
        .unwrap();
        wait_for(&rec, 1.0);
        let done = rec.stop().unwrap();
        assert!((done.duration - 1.0).abs() < 1e-9, "{}", done.duration);
        assert!(done.warnings.is_empty());

        let (spec, samples) = wav::read_f32(&lib.track_path("clip_001", TrackKind::Mic)).unwrap();
        assert_eq!(spec, wav::WavSpec::f32(8000, 1));
        assert_eq!(samples.len(), 8000);
        let sc = Sidecar::load(&lib.sidecar_path("clip_001")).unwrap();
        assert_eq!(sc.thrasher.state, RecordingState::Complete);
        assert_eq!(sc.file, "clip_001.wav");
        assert_eq!(sc.thrasher.tracks[0].device.as_deref(), Some("Fake tone"));
    }

    #[test]
    fn aligns_and_pads_second_track() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        // System audio starts 0.25 s after the mic and stops early.
        let rec = Recording::start(
            &lib,
            "meeting",
            vec![
                (TrackKind::Mic, fake(8000, 1, 8000, Some(1_000_000_000))),
                (TrackKind::System, fake(16000, 2, 4000, Some(1_002_500_000))),
            ],
            RecorderOptions::default(),
        )
        .unwrap();
        wait_for(&rec, 1.0);
        let done = rec.stop().unwrap();
        assert!((done.duration - 1.0).abs() < 1e-9);

        let (spec, sys) = wav::read_f32(&lib.track_path("meeting", TrackKind::System)).unwrap();
        assert_eq!(spec.channels, 2);
        // Padded to the mic's 1.0 s.
        assert_eq!(sys.len(), 16000 * 2);
        // The first 0.25 s is silence, then the tone begins.
        assert!(sys[..4000 * 2].iter().all(|&s| s == 0.0));
        assert!(sys[4001 * 2..4100 * 2].iter().any(|&s| s.abs() > 0.1));
    }

    #[test]
    fn pause_skips_audio() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        let mut src = FakeSource::tone(8000, 1);
        src.realtime = true;
        let rec = Recording::start(
            &lib,
            "p",
            vec![(TrackKind::Mic, Box::new(src))],
            RecorderOptions::default(),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        rec.pause();
        std::thread::sleep(Duration::from_millis(50));
        let at_pause = rec.elapsed();
        std::thread::sleep(Duration::from_millis(400));
        assert!(
            (rec.elapsed() - at_pause).abs() < 0.03,
            "advanced while paused"
        );
        assert!(rec.is_paused());
        rec.resume();
        std::thread::sleep(Duration::from_millis(200));
        let done = rec.stop().unwrap();
        assert!(
            done.duration > 0.4 && done.duration < 0.7,
            "{}",
            done.duration
        );
    }

    #[test]
    fn start_fails_cleanly() {
        struct Broken;
        impl Source for Broken {
            fn run(self: Box<Self>, _ctx: SourceCtx) -> Result<(), String> {
                Err("no such device".into())
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        let err = Recording::start(
            &lib,
            "x",
            vec![(TrackKind::Mic, Box::new(Broken))],
            RecorderOptions::default(),
        )
        .err()
        .unwrap();
        assert!(err.contains("no such device"), "{err}");
        assert!(!lib.exists("x"));
    }
}
