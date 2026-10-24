//! Crash-safe incremental WAV writing and a small WAV reader.
//!
//! The writer reserves a `JUNK` chunk after the RIFF header that is large enough to
//! hold an RF64 `ds64` chunk. While the file is below 4 GiB it is a plain WAV file.
//! When it grows past that, the header is rewritten as RF64. Header sizes are patched
//! periodically, so a killed process leaves a readable file, and [`repair`] fixes the
//! last few hundred milliseconds of size information on the next start.

use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Sample encoding of a WAV file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleFormat {
    /// IEEE float, 32 bit.
    F32,
    /// Signed integer PCM with this many bits per sample (8, 16, 24 or 32).
    Int(u16),
}

impl SampleFormat {
    fn bytes(self) -> u16 {
        match self {
            SampleFormat::F32 => 4,
            SampleFormat::Int(bits) => bits.div_ceil(8),
        }
    }
}

/// Basic shape of a WAV file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WavSpec {
    pub sample_rate: u32,
    pub channels: u16,
    pub format: SampleFormat,
}

impl WavSpec {
    pub fn f32(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            format: SampleFormat::F32,
        }
    }

    pub fn block_align(&self) -> u16 {
        self.channels * self.format.bytes()
    }
}

// Byte layout written by `WavWriter`:
//   0  "RIFF"/"RF64"  4  riff size  8  "WAVE"
//  12  "JUNK"/"ds64" 16  28 bytes (riff size u64, data size u64, sample count u64, table len u32)
//  48  "fmt "        52  16       56..72 fmt payload
//  72  "data"        76  data size
//  80  samples
const DATA_SIZE_OFFSET: u64 = 76;
const HEADER_LEN: u64 = 80;
const U32_LIMIT: u64 = u32::MAX as u64;

/// Writes 32-bit float samples to a WAV file, keeping the header valid as it goes.
pub struct WavWriter {
    out: BufWriter<File>,
    spec: WavSpec,
    data_bytes: u64,
    rf64: bool,
}

impl WavWriter {
    pub fn create(path: &Path, sample_rate: u32, channels: u16) -> io::Result<Self> {
        let spec = WavSpec::f32(sample_rate, channels);
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(path)?;
        let mut out = BufWriter::with_capacity(64 * 1024, file);
        write_header(&mut out, &spec, 0, false)?;
        Ok(Self {
            out,
            spec,
            data_bytes: 0,
            rf64: false,
        })
    }

    pub fn spec(&self) -> WavSpec {
        self.spec
    }

    /// Interleaved samples. The length should be a whole number of frames.
    pub fn write(&mut self, samples: &[f32]) -> io::Result<()> {
        for s in samples {
            self.out.write_all(&s.to_le_bytes())?;
        }
        self.data_bytes += samples.len() as u64 * 4;
        Ok(())
    }

    /// Writes `frames` frames of silence.
    pub fn write_silence(&mut self, frames: u64) -> io::Result<()> {
        let zeros = [0u8; 4096];
        let mut left = frames * self.spec.block_align() as u64;
        self.data_bytes += left;
        while left > 0 {
            let n = left.min(zeros.len() as u64) as usize;
            self.out.write_all(&zeros[..n])?;
            left -= n as u64;
        }
        Ok(())
    }

    pub fn frames(&self) -> u64 {
        self.data_bytes / self.spec.block_align() as u64
    }

    pub fn duration_secs(&self) -> f64 {
        self.frames() as f64 / self.spec.sample_rate as f64
    }

    /// Flushes buffered samples and patches the header sizes.
    pub fn update_header(&mut self) -> io::Result<()> {
        self.out.flush()?;
        if !self.rf64 && HEADER_LEN - 8 + self.data_bytes > U32_LIMIT {
            self.rf64 = true;
        }
        let file = self.out.get_mut();
        let end = file.stream_position()?;
        patch_sizes(file, &self.spec, self.data_bytes, self.rf64)?;
        file.seek(SeekFrom::Start(end))?;
        Ok(())
    }

    /// Patches the header and asks the OS to put the data on disk.
    pub fn sync(&mut self) -> io::Result<()> {
        self.update_header()?;
        self.out.get_ref().sync_data()
    }

    pub fn finalize(mut self) -> io::Result<()> {
        self.sync()
    }
}

fn write_header<W: Write>(w: &mut W, spec: &WavSpec, data: u64, rf64: bool) -> io::Result<()> {
    let riff = HEADER_LEN - 8 + data;
    let (tag, riff32, data32) = if rf64 {
        (b"RF64", u32::MAX, u32::MAX)
    } else {
        (b"RIFF", riff as u32, data as u32)
    };
    w.write_all(tag)?;
    w.write_all(&riff32.to_le_bytes())?;
    w.write_all(b"WAVE")?;
    w.write_all(if rf64 { b"ds64" } else { b"JUNK" })?;
    w.write_all(&28u32.to_le_bytes())?;
    if rf64 {
        let frames = data / spec.block_align() as u64;
        w.write_all(&riff.to_le_bytes())?;
        w.write_all(&data.to_le_bytes())?;
        w.write_all(&frames.to_le_bytes())?;
        w.write_all(&0u32.to_le_bytes())?;
    } else {
        w.write_all(&[0u8; 28])?;
    }
    let format_tag: u16 = match spec.format {
        SampleFormat::F32 => 3,
        SampleFormat::Int(_) => 1,
    };
    let bits = spec.format.bytes() * 8;
    w.write_all(b"fmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&format_tag.to_le_bytes())?;
    w.write_all(&spec.channels.to_le_bytes())?;
    w.write_all(&spec.sample_rate.to_le_bytes())?;
    w.write_all(&(spec.sample_rate * spec.block_align() as u32).to_le_bytes())?;
    w.write_all(&spec.block_align().to_le_bytes())?;
    w.write_all(&bits.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data32.to_le_bytes())?;
    Ok(())
}

fn patch_sizes(file: &mut File, spec: &WavSpec, data: u64, rf64: bool) -> io::Result<()> {
    if rf64 {
        file.seek(SeekFrom::Start(0))?;
        write_header(file, spec, data, true)?;
    } else {
        file.seek(SeekFrom::Start(4))?;
        file.write_all(&((HEADER_LEN - 8 + data) as u32).to_le_bytes())?;
        file.seek(SeekFrom::Start(DATA_SIZE_OFFSET))?;
        file.write_all(&(data as u32).to_le_bytes())?;
    }
    Ok(())
}

/// Where the sample data of a WAV file lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WavInfo {
    pub spec: WavSpec,
    pub data_offset: u64,
    pub data_bytes: u64,
}

impl WavInfo {
    pub fn frames(&self) -> u64 {
        self.data_bytes / self.spec.block_align() as u64
    }

    pub fn duration_secs(&self) -> f64 {
        self.frames() as f64 / self.spec.sample_rate as f64
    }
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// Reads the header of a RIFF or RF64 WAV file. The data size is clamped to the
/// bytes actually present, so a truncated file still reports what it holds.
pub fn read_info(path: &Path) -> io::Result<WavInfo> {
    let mut f = File::open(path)?;
    let file_len = f.metadata()?.len();
    let mut head = [0u8; 12];
    f.read_exact(&mut head)?;
    let rf64 = &head[0..4] == b"RF64";
    if !(rf64 || &head[0..4] == b"RIFF") || &head[8..12] != b"WAVE" {
        return Err(bad("not a WAV file"));
    }
    let mut ds64_data: Option<u64> = None;
    let mut spec: Option<WavSpec> = None;
    let mut pos = 12u64;
    loop {
        let mut ch = [0u8; 8];
        if f.read_exact(&mut ch).is_err() {
            return Err(bad("no data chunk"));
        }
        let id = &ch[0..4];
        let size = u32::from_le_bytes(ch[4..8].try_into().unwrap()) as u64;
        let body = pos + 8;
        match id {
            b"ds64" => {
                let mut d = [0u8; 16];
                f.read_exact(&mut d)?;
                ds64_data = Some(u64::from_le_bytes(d[8..16].try_into().unwrap()));
            }
            b"fmt " => {
                let mut d = [0u8; 16];
                f.read_exact(&mut d)?;
                let tag = u16::from_le_bytes([d[0], d[1]]);
                let channels = u16::from_le_bytes([d[2], d[3]]);
                let sample_rate = u32::from_le_bytes(d[4..8].try_into().unwrap());
                let mut bits = u16::from_le_bytes([d[14], d[15]]);
                let mut tag = tag;
                if tag == 0xFFFE && size >= 40 {
                    let mut ext = [0u8; 24];
                    f.read_exact(&mut ext)?;
                    bits = u16::from_le_bytes([ext[2], ext[3]]).max(bits);
                    tag = u16::from_le_bytes([ext[8], ext[9]]);
                }
                let format = match (tag, bits) {
                    (3, 32) => SampleFormat::F32,
                    (1, 8 | 16 | 24 | 32) => SampleFormat::Int(bits),
                    _ => return Err(bad("unsupported WAV sample format")),
                };
                spec = Some(WavSpec {
                    sample_rate,
                    channels,
                    format,
                });
            }
            b"data" => {
                let spec = spec.ok_or_else(|| bad("data before fmt"))?;
                let declared = match (rf64, size) {
                    (true, 0xFFFF_FFFF) => ds64_data.unwrap_or(u64::MAX),
                    _ => size,
                };
                let present = file_len.saturating_sub(body);
                let mut data_bytes = declared.min(present);
                data_bytes -= data_bytes % spec.block_align() as u64;
                return Ok(WavInfo {
                    spec,
                    data_offset: body,
                    data_bytes,
                });
            }
            _ => {}
        }
        pos = body + size + (size & 1);
        f.seek(SeekFrom::Start(pos))?;
    }
}

/// Reads a whole WAV file as interleaved f32 samples in -1.0..=1.0.
pub fn read_f32(path: &Path) -> io::Result<(WavSpec, Vec<f32>)> {
    let info = read_info(path)?;
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(info.data_offset))?;
    let mut raw = vec![0u8; info.data_bytes as usize];
    f.read_exact(&mut raw)?;
    Ok((info.spec, decode(info.spec.format, &raw)))
}

/// The loudest sample (0..=1) in each of `buckets` equal slices of the file, for
/// drawing its waveform. Reads the file in pieces, so long clips need little memory.
pub fn peaks(path: &Path, buckets: usize) -> io::Result<Vec<f32>> {
    let info = read_info(path)?;
    let mut out = vec![0f32; buckets];
    let frames = info.frames();
    if frames == 0 || buckets == 0 {
        return Ok(out);
    }
    let ch = info.spec.channels.max(1) as u64;
    let align = info.spec.block_align() as u64;
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(info.data_offset))?;
    let chunk = (1 << 20) / align * align;
    let (mut left, mut frame) = (info.data_bytes, 0u64);
    let mut raw = vec![0u8; chunk as usize];
    while left > 0 {
        let n = left.min(chunk) as usize;
        f.read_exact(&mut raw[..n])?;
        left -= n as u64;
        for (i, s) in decode(info.spec.format, &raw[..n]).into_iter().enumerate() {
            let fr = frame + i as u64 / ch;
            let b = (fr * buckets as u64 / frames) as usize;
            out[b.min(buckets - 1)] = out[b.min(buckets - 1)].max(s.abs().min(1.0));
        }
        frame += n as u64 / align;
    }
    Ok(out)
}

fn decode(format: SampleFormat, raw: &[u8]) -> Vec<f32> {
    match format {
        SampleFormat::F32 => raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect(),
        SampleFormat::Int(8) => raw.iter().map(|&b| (b as f32 - 128.0) / 128.0).collect(),
        SampleFormat::Int(16) => raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b) as f32 / 32768.0)
            .collect(),
        SampleFormat::Int(24) => raw
            .as_chunks::<3>()
            .0
            .iter()
            .map(|b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0)
            .collect(),
        SampleFormat::Int(_) => raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| i32::from_le_bytes(*b) as f32 / 2_147_483_648.0)
            .collect(),
    }
}

/// Fixes the size fields of a file written by [`WavWriter`] that was never finalized,
/// for example after a crash. Trailing partial frames are cut off.
pub fn repair(path: &Path) -> io::Result<WavInfo> {
    let info = read_info(path)?;
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let present = file.metadata()?.len().saturating_sub(info.data_offset);
    let data = present - present % info.spec.block_align() as u64;
    file.set_len(info.data_offset + data)?;
    if info.data_offset == HEADER_LEN {
        let rf64 = HEADER_LEN - 8 + data > U32_LIMIT;
        patch_sizes(&mut file, &info.spec, data, rf64)?;
    } else {
        // A file from another writer: only fix the plain RIFF fields when they fit.
        if info.data_offset - 8 + data <= U32_LIMIT {
            file.seek(SeekFrom::Start(4))?;
            file.write_all(&((info.data_offset - 8 + data) as u32).to_le_bytes())?;
            file.seek(SeekFrom::Start(info.data_offset - 4))?;
            file.write_all(&(data as u32).to_le_bytes())?;
        }
    }
    file.sync_data()?;
    read_info(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(frames: usize, channels: usize) -> Vec<f32> {
        (0..frames * channels)
            .map(|i| ((i / channels) as f32 * 0.05).sin() * 0.5)
            .collect()
    }

    #[test]
    fn round_trip_through_hound() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.wav");
        let samples = sine(4800, 2);
        let mut w = WavWriter::create(&path, 48_000, 2).unwrap();
        w.write(&samples).unwrap();
        w.write_silence(100).unwrap();
        w.finalize().unwrap();

        let mut r = hound::WavReader::open(&path).unwrap();
        let spec = r.spec();
        assert_eq!(spec.sample_rate, 48_000);
        assert_eq!(spec.channels, 2);
        assert_eq!(spec.sample_format, hound::SampleFormat::Float);
        let read: Vec<f32> = r.samples::<f32>().map(Result::unwrap).collect();
        assert_eq!(read.len(), samples.len() + 200);
        assert_eq!(&read[..samples.len()], &samples[..]);

        let (s, ours) = read_f32(&path).unwrap();
        assert_eq!(s, WavSpec::f32(48_000, 2));
        assert_eq!(ours, read);
    }

    #[test]
    fn peaks_find_the_loud_part() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.wav");
        let mut w = WavWriter::create(&path, 48_000, 2).unwrap();
        // One quiet second, then one loud second.
        w.write(&vec![0.1; 96_000]).unwrap();
        w.write(&vec![-0.8; 96_000]).unwrap();
        w.finalize().unwrap();
        let p = peaks(&path, 4).unwrap();
        assert_eq!(p.len(), 4);
        assert!((p[0] - 0.1).abs() < 1e-6 && (p[1] - 0.1).abs() < 1e-6);
        assert!((p[2] - 0.8).abs() < 1e-6 && (p[3] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn header_is_valid_while_recording() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live.wav");
        let mut w = WavWriter::create(&path, 44_100, 1).unwrap();
        w.write(&sine(44_100, 1)).unwrap();
        w.update_header().unwrap();
        let info = read_info(&path).unwrap();
        assert_eq!(info.frames(), 44_100);
        assert!((info.duration_secs() - 1.0).abs() < 1e-9);
        drop(w);
    }

    #[test]
    fn repair_after_crash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.wav");
        let mut w = WavWriter::create(&path, 48_000, 1).unwrap();
        w.write(&sine(1000, 1)).unwrap();
        w.update_header().unwrap();
        w.write(&sine(500, 1)).unwrap();
        // Simulate a crash: data reached the file but the header was not patched.
        w.out.flush().unwrap();
        std::mem::forget(w);
        // Add half a frame of garbage, as a torn write would.
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(&[1, 2]).unwrap();
        drop(f);

        let fixed = repair(&path).unwrap();
        assert_eq!(fixed.frames(), 1500);
        let r = hound::WavReader::open(&path).unwrap();
        assert_eq!(r.len(), 1500);
    }

    #[test]
    fn rf64_header_layout() {
        let spec = WavSpec::f32(48_000, 2);
        let mut buf = Vec::new();
        let data = 5_000_000_000u64;
        write_header(&mut buf, &spec, data, true).unwrap();
        assert_eq!(buf.len() as u64, HEADER_LEN);
        assert_eq!(&buf[0..4], b"RF64");
        assert_eq!(&buf[12..16], b"ds64");
        assert_eq!(u64::from_le_bytes(buf[28..36].try_into().unwrap()), data);
        assert_eq!(&buf[72..76], b"data");
        assert_eq!(
            u32::from_le_bytes(buf[76..80].try_into().unwrap()),
            u32::MAX
        );
    }

    #[test]
    fn reads_16_bit_pcm_from_old_recorder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip_001.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for v in [0i16, 16384, -32768, 32767] {
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();
        let (s, data) = read_f32(&path).unwrap();
        assert_eq!(s.format, SampleFormat::Int(16));
        assert_eq!(data, vec![0.0, 0.5, -1.0, 32767.0 / 32768.0]);
    }
}
