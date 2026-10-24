//! Windows audio through WASAPI: microphone capture, system audio loopback, device
//! lists and the render stream used for playback.

use crate::capture::{Source, SourceCtx, SourceFormat};
use wasapi::{
    AudioClient, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat, initialize_mta,
};

/// 200 ms device buffer: plenty of slack for a non-realtime-priority thread.
const BUFFER_HNS: i64 = 2_000_000;
const WAIT_MS: u32 = 100;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Enumerates on a helper thread, so the caller's thread (often the UI thread, which
/// needs single-threaded COM) is never switched to multithreaded COM.
fn list(direction: Direction) -> Result<Vec<DeviceInfo>, String> {
    std::thread::spawn(move || list_here(direction))
        .join()
        .map_err(|_| "device enumeration crashed".to_string())?
}

fn list_here(direction: Direction) -> Result<Vec<DeviceInfo>, String> {
    let _ = initialize_mta();
    let en = DeviceEnumerator::new().map_err(err)?;
    let default_id = en
        .get_default_device(&direction)
        .and_then(|d| d.get_id())
        .ok();
    let coll = en.get_device_collection(&direction).map_err(err)?;
    let mut out = Vec::new();
    for dev in &coll {
        let dev = dev.map_err(err)?;
        let id = dev.get_id().map_err(err)?;
        out.push(DeviceInfo {
            name: dev.get_friendlyname().unwrap_or_else(|_| id.clone()),
            is_default: default_id.as_deref() == Some(id.as_str()),
            id,
        });
    }
    Ok(out)
}

/// Active microphones and other capture devices.
pub fn input_devices() -> Result<Vec<DeviceInfo>, String> {
    list(Direction::Capture)
}

/// Active speakers and headphones.
pub fn output_devices() -> Result<Vec<DeviceInfo>, String> {
    list(Direction::Render)
}

fn device(direction: Direction, id: Option<&str>) -> Result<wasapi::Device, String> {
    let en = DeviceEnumerator::new().map_err(err)?;
    match id {
        Some(id) => en.get_device(id).map_err(err),
        None => en.get_default_device(&direction).map_err(err),
    }
}

/// Runs an initialized capture client until stop, pushing f32 samples into the feed.
fn capture_loop(
    client: &AudioClient,
    format: SourceFormat,
    mut ctx: SourceCtx,
) -> Result<(), String> {
    let event = client.set_get_eventhandle().map_err(err)?;
    let capture = client.get_audiocaptureclient().map_err(err)?;
    let ch = format.channels as usize;
    // One second of room; shared-mode packets are about 10 ms.
    let frames_cap = format.sample_rate as usize;
    let mut bytes = vec![0u8; frames_cap * ch * 4];
    let mut samples = vec![0f32; frames_cap * ch];
    let mut feed = ctx.ready(format);
    client.start_stream().map_err(err)?;
    let result = (|| -> Result<(), String> {
        while !ctx.stopped() {
            // A timeout is normal for loopback while nothing plays.
            let _ = event.wait_for_event(WAIT_MS);
            while let Some(n) = capture.get_next_packet_size().map_err(err)? {
                if n == 0 {
                    break;
                }
                let (frames, info) = capture.read_from_device(&mut bytes).map_err(err)?;
                let len = frames as usize * ch;
                if info.flags.silent {
                    samples[..len].fill(0.0);
                } else {
                    for (s, b) in samples[..len].iter_mut().zip(bytes.as_chunks::<4>().0) {
                        *s = f32::from_le_bytes(*b);
                    }
                }
                let time = (!info.flags.timestamp_error).then_some(info.timestamp);
                feed.push(&samples[..len], time);
            }
        }
        Ok(())
    })();
    let _ = client.stop_stream();
    if let Err(e) = &result {
        ctx.failed(e.clone());
    }
    result
}

/// A microphone (or any capture device), recorded as mono at the device's own rate.
pub struct MicSource {
    pub device_id: Option<String>,
    pub channels: u16,
}

impl MicSource {
    pub fn new(device_id: Option<String>) -> Self {
        Self {
            device_id,
            channels: 1,
        }
    }
}

impl Source for MicSource {
    fn run(self: Box<Self>, ctx: SourceCtx) -> Result<(), String> {
        let _ = initialize_mta();
        let dev = device(Direction::Capture, self.device_id.as_deref())?;
        *ctx.shared.label.lock().unwrap() = dev.get_friendlyname().ok();
        let mut client = dev.get_iaudioclient().map_err(err)?;
        let rate = client.get_mixformat().map_err(err)?.get_samplespersec();
        let format = SourceFormat {
            sample_rate: rate,
            channels: self.channels,
        };
        let wf = WaveFormat::new(
            32,
            32,
            &SampleType::Float,
            rate as usize,
            self.channels as usize,
            None,
        );
        let mode = StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: BUFFER_HNS,
        };
        client
            .initialize_client(&wf, &Direction::Capture, &mode)
            .map_err(err)?;
        capture_loop(&client, format, ctx)
    }
}

/// How system audio is captured.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoopbackMode {
    /// Everything the computer plays except Thrasher itself (Windows 10 2004 or later).
    ExcludeSelf,
    /// Everything played on one output device (the default when `None`).
    Endpoint(Option<String>),
}

/// System audio, recorded as stereo.
pub struct LoopbackSource {
    pub mode: LoopbackMode,
}

impl Source for LoopbackSource {
    fn run(self: Box<Self>, ctx: SourceCtx) -> Result<(), String> {
        let _ = initialize_mta();
        *ctx.shared.label.lock().unwrap() = Some(match &self.mode {
            LoopbackMode::ExcludeSelf => "System audio".into(),
            LoopbackMode::Endpoint(_) => "System audio (device loopback)".into(),
        });
        let out_id = match &self.mode {
            LoopbackMode::Endpoint(id) => id.clone(),
            LoopbackMode::ExcludeSelf => None,
        };
        let out = device(Direction::Render, out_id.as_deref())?;
        // Process loopback has no mix format of its own; use the output device's rate.
        let rate = out
            .get_iaudioclient()
            .and_then(|c| c.get_mixformat())
            .map(|f| f.get_samplespersec())
            .unwrap_or(48_000);
        let format = SourceFormat {
            sample_rate: rate,
            channels: 2,
        };
        let wf = WaveFormat::new(32, 32, &SampleType::Float, rate as usize, 2, None);
        let mut client = match &self.mode {
            LoopbackMode::ExcludeSelf => {
                AudioClient::new_application_loopback_client(std::process::id(), false)
                    .map_err(err)?
            }
            LoopbackMode::Endpoint(_) => out.get_iaudioclient().map_err(err)?,
        };
        let mode = StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: match self.mode {
                // Process loopback rejects a fixed buffer size.
                LoopbackMode::ExcludeSelf => 0,
                LoopbackMode::Endpoint(_) => BUFFER_HNS,
            },
        };
        client
            .initialize_client(&wf, &Direction::Capture, &mode)
            .map_err(err)?;
        capture_loop(&client, format, ctx)
    }
}

/// Plays interleaved f32 audio on the default output device, asking `fill` for up to
/// the given number of frames at a time (it returns how many it added, 0 at the end).
/// Blocks until done or until `stop` is set. `played` counts the frames heard so far.
pub fn play_blocking(
    fill: &mut dyn FnMut(usize, &mut Vec<f32>) -> usize,
    sample_rate: u32,
    channels: u16,
    stop: &std::sync::atomic::AtomicBool,
    played: &std::sync::atomic::AtomicU64,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let _ = initialize_mta();
    let dev = device(Direction::Render, None)?;
    let mut client = dev.get_iaudioclient().map_err(err)?;
    let wf = WaveFormat::new(
        32,
        32,
        &SampleType::Float,
        sample_rate as usize,
        channels as usize,
        None,
    );
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: BUFFER_HNS / 2,
    };
    client
        .initialize_client(&wf, &Direction::Render, &mode)
        .map_err(err)?;
    let event = client.set_get_eventhandle().map_err(err)?;
    let render = client.get_audiorenderclient().map_err(err)?;
    let ch = channels as usize;
    let size = client.get_buffer_size().map_err(err)? as usize;
    let mut sent = 0u64;
    let mut ended = false;
    let mut chunk: Vec<f32> = Vec::with_capacity(size * ch);
    let mut bytes: Vec<u8> = Vec::with_capacity(size * ch * 4);
    client.start_stream().map_err(err)?;
    let result = (|| -> Result<(), String> {
        loop {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            let space = client.get_available_space_in_frames().map_err(err)? as usize;
            let padding = client.get_current_padding().map_err(err)? as u64;
            played.store(sent.saturating_sub(padding), Ordering::Relaxed);
            if ended {
                if padding == 0 {
                    return Ok(());
                }
            } else if space > 0 {
                chunk.clear();
                let n = fill(space, &mut chunk);
                if n == 0 {
                    ended = true;
                } else {
                    bytes.clear();
                    for s in &chunk[..n * ch] {
                        bytes.extend_from_slice(&s.to_le_bytes());
                    }
                    render.write_to_device(n, &bytes, None).map_err(err)?;
                    sent += n as u64;
                }
            }
            let _ = event.wait_for_event(WAIT_MS);
        }
    })();
    let _ = client.stop_stream();
    result
}
