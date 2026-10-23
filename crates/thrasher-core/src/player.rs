//! Clip playback on a background thread, with stop and position.

use crate::library::Library;
use crate::sidecar::TrackKind;
use crate::wav;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Audio ready to play: interleaved f32.
#[derive(Clone, Debug, PartialEq)]
pub struct Buffer {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl Buffer {
    pub fn frames(&self) -> u64 {
        self.samples.len() as u64 / self.channels.max(1) as u64
    }

    pub fn duration_secs(&self) -> f64 {
        self.frames() as f64 / self.sample_rate as f64
    }
}

/// Loads a clip for playback. When the clip has a system-audio track at the same
/// sample rate, the two are mixed to stereo; otherwise only the mic track is used.
pub fn load_clip(library: &Library, name: &str) -> std::io::Result<Buffer> {
    load_clip_with(library, name, None)
}

/// Processes the mic track before mixing: takes samples, rate and channels.
pub type MicFilter<'a> = &'a dyn Fn(&[f32], u32, u16) -> Result<Vec<f32>, String>;

/// Like [`load_clip`], with an optional filter run on the mic track first.
pub fn load_clip_with(
    library: &Library,
    name: &str,
    mic_filter: Option<MicFilter>,
) -> std::io::Result<Buffer> {
    let (spec, mut mic) = wav::read_f32(&library.track_path(name, TrackKind::Mic))?;
    if let Some(f) = mic_filter {
        mic = f(&mic, spec.sample_rate, spec.channels).map_err(std::io::Error::other)?;
    }
    let sys_path = library.track_path(name, TrackKind::System);
    let system = sys_path
        .exists()
        .then(|| wav::read_f32(&sys_path).ok())
        .flatten()
        .filter(|(s, _)| s.sample_rate == spec.sample_rate);
    let Some((sys_spec, sys)) = system else {
        return Ok(Buffer {
            sample_rate: spec.sample_rate,
            channels: spec.channels,
            samples: mic,
        });
    };
    let mc = spec.channels as usize;
    let sc = sys_spec.channels as usize;
    let frames = (mic.len() / mc).max(sys.len() / sc);
    let mut out = vec![0f32; frames * 2];
    for f in 0..frames {
        for side in 0..2 {
            let m = mic.get(f * mc + side.min(mc - 1)).copied().unwrap_or(0.0);
            let s = sys.get(f * sc + side.min(sc - 1)).copied().unwrap_or(0.0);
            out[f * 2 + side] = (m + s).clamp(-1.0, 1.0);
        }
    }
    Ok(Buffer {
        sample_rate: spec.sample_rate,
        channels: 2,
        samples: out,
    })
}

struct Playing {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

/// Plays one clip at a time.
#[derive(Default)]
pub struct Player {
    current: Option<Playing>,
    position: Arc<AtomicU64>,
    done: Arc<AtomicBool>,
    rate: u32,
    error: Arc<Mutex<Option<String>>>,
}

impl Player {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn play(&mut self, buffer: Buffer, start_secs: f64) {
        self.stop();
        // Windows plays a mono stream on the left speaker only, so send it as stereo.
        let buffer = if buffer.channels == 1 {
            Buffer {
                channels: 2,
                samples: buffer.samples.iter().flat_map(|&s| [s, s]).collect(),
                ..buffer
            }
        } else {
            buffer
        };
        let stop = Arc::new(AtomicBool::new(false));
        let start = (start_secs.max(0.0) * buffer.sample_rate as f64) as u64;
        self.position.store(start, Ordering::Relaxed);
        self.done.store(false, Ordering::Relaxed);
        self.rate = buffer.sample_rate;
        *self.error.lock().unwrap() = None;
        let (st, pos, done, error) = (
            stop.clone(),
            self.position.clone(),
            self.done.clone(),
            self.error.clone(),
        );
        let handle = std::thread::Builder::new()
            .name("thrasher-playback".into())
            .spawn(move || {
                let r = output(&buffer, start, &st, &pos);
                if let Err(e) = r {
                    *error.lock().unwrap() = Some(e);
                }
                done.store(true, Ordering::Relaxed);
            })
            .expect("spawn playback thread");
        self.current = Some(Playing { stop, handle });
    }

    pub fn stop(&mut self) {
        if let Some(p) = self.current.take() {
            p.stop.store(true, Ordering::Relaxed);
            let _ = p.handle.join();
        }
    }

    pub fn is_playing(&self) -> bool {
        self.current.is_some() && !self.done.load(Ordering::Relaxed)
    }

    pub fn position_secs(&self) -> f64 {
        if self.rate == 0 {
            return 0.0;
        }
        self.position.load(Ordering::Relaxed) as f64 / self.rate as f64
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(windows)]
fn output(b: &Buffer, start: u64, stop: &AtomicBool, pos: &AtomicU64) -> Result<(), String> {
    crate::wasapi_io::play_blocking(&b.samples, b.sample_rate, b.channels, start, stop, pos)
}

#[cfg(not(windows))]
fn output(_: &Buffer, _: u64, _: &AtomicBool, _: &AtomicU64) -> Result<(), String> {
    Err("playback is only implemented on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::WavWriter;

    #[test]
    fn mixes_system_track_to_stereo() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        let mut m = WavWriter::create(&lib.track_path("c", TrackKind::Mic), 100, 1).unwrap();
        m.write(&[0.5, 0.5]).unwrap();
        m.finalize().unwrap();
        let mut s = WavWriter::create(&lib.track_path("c", TrackKind::System), 100, 2).unwrap();
        s.write(&[0.1, 0.2, 0.7, 0.7, 0.1, 0.1]).unwrap();
        s.finalize().unwrap();
        let b = load_clip(&lib, "c").unwrap();
        assert_eq!(b.channels, 2);
        assert_eq!(b.frames(), 3);
        assert_eq!(b.samples, vec![0.6, 0.7, 1.0, 1.0, 0.1, 0.1]);
    }

    #[test]
    fn mic_only() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        let mut m = WavWriter::create(&lib.track_path("c", TrackKind::Mic), 100, 1).unwrap();
        m.write(&[0.25; 50]).unwrap();
        m.finalize().unwrap();
        let b = load_clip(&lib, "c").unwrap();
        assert_eq!((b.channels, b.frames()), (1, 50));
        assert!((b.duration_secs() - 0.5).abs() < 1e-9);
    }
}
