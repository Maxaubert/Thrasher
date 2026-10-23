//! Audio sources. Each source runs on its own thread, opens its device there, reports
//! its format once, then only copies samples into a lock-free ring buffer.
//!
//! The capture loop never allocates, locks or logs: buffers are made before the
//! stream starts, and errors are reported once, after the loop has ended.

use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};

/// Sample rate and channel count a source delivers. Samples are always f32.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

/// State a source shares with the recorder.
#[derive(Debug, Default)]
pub struct SourceShared {
    /// Timestamp of the first frame in 100 ns units on the QPC clock, 0 until known.
    pub start_time: AtomicU64,
    /// Samples dropped because the ring buffer was full.
    pub overruns: AtomicU64,
    /// Silence frames inserted to cover gaps in the device stream.
    pub gap_frames: AtomicU64,
    pub error: Mutex<Option<String>>,
    /// Device name, set by the source once it knows it.
    pub label: Mutex<Option<String>>,
}

pub type ReadySender = SyncSender<Result<(SourceFormat, Consumer<f32>), String>>;

/// Hands a source what it needs to run.
pub struct SourceCtx {
    pub stop: Arc<AtomicBool>,
    pub shared: Arc<SourceShared>,
    ready: Option<ReadySender>,
    buffer_secs: f32,
}

impl SourceCtx {
    pub fn new(stop: Arc<AtomicBool>, shared: Arc<SourceShared>, ready: ReadySender) -> Self {
        Self {
            stop,
            shared,
            ready: Some(ready),
            buffer_secs: 4.0,
        }
    }

    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Call once the device is open. Returns the feed to push samples into.
    pub fn ready(&mut self, format: SourceFormat) -> Feed {
        let cap = (format.sample_rate as f32 * format.channels as f32 * self.buffer_secs) as usize;
        let (producer, consumer) = RingBuffer::new(cap.max(1024));
        if let Some(tx) = self.ready.take() {
            let _ = tx.send(Ok((format, consumer)));
        }
        Feed {
            producer,
            format,
            shared: self.shared.clone(),
            first_time: None,
            frames: 0,
        }
    }

    /// Call if the device could not be opened.
    pub fn failed(&mut self, msg: String) {
        if let Some(tx) = self.ready.take() {
            let _ = tx.send(Err(msg));
        }
    }
}

/// The producer side of a source's ring buffer, with gap filling.
pub struct Feed {
    producer: Producer<f32>,
    format: SourceFormat,
    shared: Arc<SourceShared>,
    first_time: Option<u64>,
    frames: u64,
}

/// Gaps shorter than this are treated as timestamp jitter.
const GAP_TOLERANCE_100NS: u64 = 200_000;

impl Feed {
    /// Pushes interleaved samples. `time` is the QPC time (100 ns) of the first frame,
    /// if the device gives one. A jump forward in time is filled with silence, so a
    /// track that stalls (loopback while nothing plays) stays aligned with the clock.
    pub fn push(&mut self, samples: &[f32], time: Option<u64>) {
        let ch = self.format.channels as u64;
        if let Some(t) = time {
            match self.first_time {
                None => {
                    self.first_time = Some(t);
                    self.shared.start_time.store(t, Ordering::Relaxed);
                }
                Some(first) => {
                    let expected =
                        first + self.frames * 10_000_000 / self.format.sample_rate as u64;
                    if t > expected + GAP_TOLERANCE_100NS {
                        let gap = (t - expected) * self.format.sample_rate as u64 / 10_000_000;
                        self.silence(gap);
                        self.shared.gap_frames.fetch_add(gap, Ordering::Relaxed);
                    }
                }
            }
        }
        self.write(samples.iter().copied(), samples.len());
        self.frames += samples.len() as u64 / ch;
    }

    pub fn silence(&mut self, frames: u64) {
        let n = (frames * self.format.channels as u64) as usize;
        self.write(std::iter::repeat_n(0.0, n), n);
        self.frames += frames;
    }

    fn write(&mut self, it: impl Iterator<Item = f32>, n: usize) {
        let free = self.producer.slots();
        let take = n.min(free);
        if take > 0
            && let Ok(chunk) = self.producer.write_chunk_uninit(take)
        {
            chunk.fill_from_iter(it.take(take));
        }
        if take < n {
            self.shared
                .overruns
                .fetch_add((n - take) as u64, Ordering::Relaxed);
        }
    }

    pub fn format(&self) -> SourceFormat {
        self.format
    }
}

/// Something that produces audio: a device, or a fake for tests.
pub trait Source: Send + 'static {
    /// Opens the device, calls `ctx.ready`, then pushes samples until `ctx.stop` is set.
    fn run(self: Box<Self>, ctx: SourceCtx) -> Result<(), String>;
}

/// A test source: a sine tone, delivered in real time or as fast as possible.
pub struct FakeSource {
    pub format: SourceFormat,
    pub frequency: f32,
    pub amplitude: f32,
    /// Stop producing after this many frames (then idle until stopped).
    pub total_frames: Option<u64>,
    pub realtime: bool,
    /// Timestamp of the first frame, to test alignment. `None` gives no timestamps.
    pub start_time: Option<u64>,
}

impl FakeSource {
    pub fn tone(sample_rate: u32, channels: u16) -> Self {
        Self {
            format: SourceFormat {
                sample_rate,
                channels,
            },
            frequency: 440.0,
            amplitude: 0.5,
            total_frames: None,
            realtime: true,
            start_time: None,
        }
    }
}

impl Source for FakeSource {
    fn run(self: Box<Self>, mut ctx: SourceCtx) -> Result<(), String> {
        *ctx.shared.label.lock().unwrap() = Some("Fake tone".into());
        let mut feed = ctx.ready(self.format);
        let rate = self.format.sample_rate as u64;
        let ch = self.format.channels as usize;
        let block = (rate / 100).max(1);
        let mut buf = vec![0f32; block as usize * ch];
        let mut produced = 0u64;
        let begin = std::time::Instant::now();
        while !ctx.stopped() {
            let left = self
                .total_frames
                .map_or(block, |t| t.saturating_sub(produced));
            if left == 0 {
                std::thread::sleep(std::time::Duration::from_millis(2));
                continue;
            }
            let n = left.min(block) as usize;
            for f in 0..n {
                let t = (produced + f as u64) as f32 / rate as f32;
                let v = (t * self.frequency * std::f32::consts::TAU).sin() * self.amplitude;
                buf[f * ch..(f + 1) * ch].fill(v);
            }
            if !self.realtime {
                // Wait for room rather than dropping, so tests get exact output.
                while feed.producer.slots() < n * ch && !ctx.stopped() {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
            let time = self.start_time.map(|s| s + produced * 10_000_000 / rate);
            feed.push(&buf[..n * ch], time);
            produced += n as u64;
            if self.realtime {
                let due = std::time::Duration::from_secs_f64(produced as f64 / rate as f64);
                if let Some(wait) = due.checked_sub(begin.elapsed()) {
                    std::thread::sleep(wait);
                }
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
pub use crate::wasapi_io::{LoopbackMode, LoopbackSource, MicSource};
