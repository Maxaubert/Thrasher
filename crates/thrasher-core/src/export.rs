//! Export a clip to WAV or FLAC at 16 or 24 bit. Exports go into an `export` folder
//! inside the clip folder, so they never show up as clips themselves.

use crate::library::Library;
use crate::player::{Buffer, load_clip};
use crate::wav::{SampleFormat, WavSpec};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Wav,
    Flac,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Wav => "wav",
            Format::Flac => "flac",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportOptions {
    pub format: Format,
    /// 16 or 24.
    pub bits: u16,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            format: Format::Flac,
            bits: 24,
        }
    }
}

pub fn export_dir(library: &Library) -> PathBuf {
    library.dir().join("export")
}

/// Exports the clip (mic and system audio mixed, as heard in the player).
pub fn export_clip(library: &Library, name: &str, options: ExportOptions) -> io::Result<PathBuf> {
    let buffer = load_clip(library, name)?;
    let dir = export_dir(library);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{name}.{}", options.format.extension()));
    export_buffer(&buffer, &path, options)?;
    Ok(path)
}

pub fn export_buffer(buffer: &Buffer, path: &Path, options: ExportOptions) -> io::Result<()> {
    if options.bits != 16 && options.bits != 24 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "bits must be 16 or 24",
        ));
    }
    let ints = quantize(&buffer.samples, options.bits);
    let tmp = path.with_extension("part");
    match options.format {
        Format::Wav => write_pcm_wav(
            &tmp,
            WavSpec {
                sample_rate: buffer.sample_rate,
                channels: buffer.channels,
                format: SampleFormat::Int(options.bits),
            },
            &ints,
        )?,
        Format::Flac => write_flac(
            &tmp,
            buffer.sample_rate,
            buffer.channels,
            options.bits,
            &ints,
        )?,
    }
    std::fs::rename(&tmp, path)
}

/// Converts float samples to integers with triangular dither, which hides the
/// quantization distortion of quiet passages at 16 bit.
pub fn quantize(samples: &[f32], bits: u16) -> Vec<i32> {
    let max = ((1i64 << (bits - 1)) - 1) as f64;
    let min = -(1i64 << (bits - 1)) as f64;
    let mut seed: u32 = 0x2545_F491;
    let mut rand = move || {
        // xorshift32: fast, deterministic, plenty for dither.
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f64 / u32::MAX as f64
    };
    samples
        .iter()
        .map(|&s| {
            let dither = rand() - rand();
            (s as f64 * max + dither).round().clamp(min, max) as i32
        })
        .collect()
}

fn write_pcm_wav(path: &Path, spec: WavSpec, samples: &[i32]) -> io::Result<()> {
    let bytes_per = spec.format_bytes();
    let data_len = samples.len() as u64 * bytes_per as u64;
    if data_len + 36 > u32::MAX as u64 {
        return Err(io::Error::other("export is larger than 4 GiB; use FLAC"));
    }
    let mut out = io::BufWriter::new(std::fs::File::create(path)?);
    let block = spec.channels * bytes_per;
    out.write_all(b"RIFF")?;
    out.write_all(&((36 + data_len) as u32).to_le_bytes())?;
    out.write_all(b"WAVEfmt ")?;
    out.write_all(&16u32.to_le_bytes())?;
    out.write_all(&1u16.to_le_bytes())?;
    out.write_all(&spec.channels.to_le_bytes())?;
    out.write_all(&spec.sample_rate.to_le_bytes())?;
    out.write_all(&(spec.sample_rate * block as u32).to_le_bytes())?;
    out.write_all(&block.to_le_bytes())?;
    out.write_all(&(bytes_per * 8).to_le_bytes())?;
    out.write_all(b"data")?;
    out.write_all(&(data_len as u32).to_le_bytes())?;
    for &s in samples {
        out.write_all(&s.to_le_bytes()[..bytes_per as usize])?;
    }
    out.flush()
}

fn write_flac(path: &Path, rate: u32, channels: u16, bits: u16, samples: &[i32]) -> io::Result<()> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|e| io::Error::other(format!("{e:?}")))?;
    let source = flacenc::source::MemSource::from_samples(
        samples,
        channels as usize,
        bits as usize,
        rate as usize,
    );
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| io::Error::other(format!("{e:?}")))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| io::Error::other(format!("{e:?}")))?;
    std::fs::write(path, sink.as_slice())
}

impl WavSpec {
    fn format_bytes(&self) -> u16 {
        match self.format {
            SampleFormat::F32 => 4,
            SampleFormat::Int(b) => b.div_ceil(8),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::TrackKind;
    use crate::wav::WavWriter;

    fn clip(lib: &Library) {
        let mut w = WavWriter::create(&lib.track_path("c", TrackKind::Mic), 48_000, 1).unwrap();
        let tone: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        w.write(&tone).unwrap();
        w.finalize().unwrap();
    }

    #[test]
    fn quantize_range() {
        let q = quantize(&[0.0, 1.0, -1.0, 2.0], 16);
        assert!(q[0].abs() <= 1);
        assert_eq!(q[1], 32767);
        assert_eq!(q[2], -32767);
        assert_eq!(q[3], 32767);
    }

    #[test]
    fn wav_16_export_is_readable() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        clip(&lib);
        let p = export_clip(
            &lib,
            "c",
            ExportOptions {
                format: Format::Wav,
                bits: 16,
            },
        )
        .unwrap();
        let r = hound::WavReader::open(&p).unwrap();
        assert_eq!(r.spec().bits_per_sample, 16);
        assert_eq!(r.len(), 48_000);
        // Exports must not appear as clips.
        assert_eq!(lib.names().unwrap(), ["c"]);
    }

    #[test]
    fn flac_export_is_smaller_and_has_magic() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        clip(&lib);
        let p = export_clip(&lib, "c", ExportOptions::default()).unwrap();
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(&bytes[..4], b"fLaC");
        assert!(bytes.len() < 48_000 * 3);
    }
}
