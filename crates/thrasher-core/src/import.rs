//! Import an audio file (WAV, MP3, FLAC, M4A/AAC, ALAC, OGG Vorbis, AIFF) as a new
//! clip. The file is decoded with Symphonia and stored as the clip's WAV track, so it
//! plays, transcribes and exports like a recording. The original file is left alone.

use crate::library::{Library, valid_name};
use crate::sidecar::{Meta, RecordingState, Sidecar, Track, TrackKind};
use crate::wav::WavWriter;
use std::io;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// File extensions offered in the import dialog.
pub const EXTENSIONS: [&str; 10] = [
    "wav", "mp3", "flac", "m4a", "aac", "mp4", "ogg", "oga", "aif", "aiff",
];

/// Imports `path` as a new clip named after the file, and returns the clip's name.
pub fn import_file(library: &Library, path: &Path) -> io::Result<String> {
    let name = free_name(library, path);
    let track = library.track_path(&name, TrackKind::Mic);
    let (rate, channels, frames) = match decode_into(path, &track) {
        Ok(r) => r,
        Err(e) => {
            let _ = std::fs::remove_file(&track);
            return Err(e);
        }
    };
    let file = track
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let sidecar = Sidecar {
        file: file.clone(),
        duration: frames as f64 / rate as f64,
        thrasher: Meta {
            version: 1,
            state: RecordingState::Complete,
            created: chrono::Local::now().to_rfc3339(),
            tracks: vec![Track {
                kind: TrackKind::Mic,
                file,
                sample_rate: rate,
                channels,
                device: Some(format!("imported from {}", path.display())),
            }],
        },
        ..Default::default()
    };
    sidecar.save(&library.sidecar_path(&name))?;
    Ok(name)
}

/// The file name without its extension, made safe and unique in the library.
fn free_name(library: &Library, path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let cleaned: String = stem
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let base = cleaned.trim().trim_matches('.').to_string();
    let base = if valid_name(&base) {
        base
    } else {
        "imported".into()
    };
    let mut name = base.clone();
    let mut n = 2;
    while library.exists(&name) {
        name = format!("{base} ({n})");
        n += 1;
    }
    name
}

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

/// Decodes the first audio track of `path` into a WAV at `out`. More than two channels
/// are mixed down to mono. Returns the sample rate, channels and frames written.
fn decode_into(path: &Path, out: &Path) -> io::Result<(u32, u16, u64)> {
    let is_flac = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("flac"));
    if is_flac {
        // Symphonia rejects some valid FLAC files (Thrasher's own exports among them).
        return decode_flac(path, out);
    }
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| invalid(format!("not an audio file Thrasher can read ({e})")))?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| invalid("the file has no audio"))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| invalid(format!("cannot decode this audio: {e}")))?;

    let mut writer: Option<WavWriter> = None;
    let mut mixed = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(e)) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(Error::ResetRequired) => break,
            Err(e) => return Err(invalid(e.to_string())),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // A damaged packet is skipped, as players do.
            Err(Error::DecodeError(_)) => continue,
            Err(e) => return Err(invalid(e.to_string())),
        };
        let spec = *decoded.spec();
        let ch = spec.channels.count().max(1);
        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buf.copy_interleaved_ref(decoded);
        let w = match &mut writer {
            Some(w) => w,
            None => {
                let out_ch = if ch > 2 { 1 } else { ch as u16 };
                writer.insert(WavWriter::create(out, spec.rate, out_ch)?)
            }
        };
        if ch <= 2 {
            w.write(buf.samples())?;
        } else {
            mixed.clear();
            mixed.extend(
                buf.samples()
                    .chunks(ch)
                    .map(|f| f.iter().sum::<f32>() / ch as f32),
            );
            w.write(&mixed)?;
        }
    }
    let w = writer.ok_or_else(|| invalid("the file has no audio"))?;
    let spec = w.spec();
    let frames = w.frames();
    w.finalize()?;
    Ok((spec.sample_rate, spec.channels, frames))
}

fn decode_flac(path: &Path, out: &Path) -> io::Result<(u32, u16, u64)> {
    let mut reader = claxon::FlacReader::open(path)
        .map_err(|e| invalid(format!("not a FLAC file Thrasher can read ({e})")))?;
    let info = reader.streaminfo();
    let ch = info.channels as usize;
    let scale = 1.0 / (1u64 << (info.bits_per_sample - 1)) as f32;
    let out_ch = if ch > 2 { 1 } else { ch as u16 };
    let mut w = WavWriter::create(out, info.sample_rate, out_ch)?;
    let mut frame = Vec::with_capacity(ch);
    let mut chunk = Vec::with_capacity(1 << 16);
    for s in reader.samples() {
        let s = s.map_err(|e| invalid(e.to_string()))? as f32 * scale;
        if ch <= 2 {
            chunk.push(s);
        } else {
            frame.push(s);
            if frame.len() == ch {
                chunk.push(frame.iter().sum::<f32>() / ch as f32);
                frame.clear();
            }
        }
        if chunk.len() >= 1 << 16 {
            w.write(&chunk)?;
            chunk.clear();
        }
    }
    w.write(&chunk)?;
    let (spec, frames) = (w.spec(), w.frames());
    w.finalize()?;
    Ok((spec.sample_rate, spec.channels, frames))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{ExportOptions, Format, export_buffer};
    use crate::player::Buffer;

    fn tone(rate: u32, channels: u16, secs: f32) -> Buffer {
        let frames = (rate as f32 * secs) as usize;
        let samples = (0..frames * channels as usize)
            .map(|i| ((i / channels as usize) as f32 * 0.05).sin() * 0.4)
            .collect();
        Buffer {
            samples,
            sample_rate: rate,
            channels,
        }
    }

    #[test]
    fn imports_wav_and_flac_as_clips() {
        let src = tempfile::tempdir().unwrap();
        let lib_dir = tempfile::tempdir().unwrap();
        let lib = Library::open(lib_dir.path()).unwrap();
        for (file, format) in [("talk.wav", Format::Wav), ("talk.flac", Format::Flac)] {
            let p = src.path().join(file);
            let o = ExportOptions {
                format,
                bits: 16,
                clean_voice: false,
            };
            export_buffer(&tone(44_100, 2, 1.5), &p, o).unwrap();
            let name = import_file(&lib, &p).unwrap_or_else(|e| panic!("{file}: {e}"));
            let sc = Sidecar::load(&lib.sidecar_path(&name)).unwrap();
            assert!((sc.duration - 1.5).abs() < 0.01, "{file}: {}", sc.duration);
            assert_eq!(sc.thrasher.tracks[0].sample_rate, 44_100);
            assert_eq!(sc.thrasher.tracks[0].channels, 2);
        }
        assert_eq!(lib.names().unwrap().len(), 2);
        assert!(lib.exists("talk") && lib.exists("talk (2)"));
    }

    #[test]
    fn rejects_files_that_are_not_audio() {
        let src = tempfile::tempdir().unwrap();
        let lib_dir = tempfile::tempdir().unwrap();
        let lib = Library::open(lib_dir.path()).unwrap();
        let p = src.path().join("notes.mp3");
        std::fs::write(&p, "not audio at all").unwrap();
        assert!(import_file(&lib, &p).is_err());
        assert!(lib.names().unwrap().is_empty());
    }
}
