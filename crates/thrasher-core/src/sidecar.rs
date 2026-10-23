//! The JSON file stored next to each clip (`clip_007.json`).
//!
//! The top-level `file`, `duration` and `words` fields match the transcript format
//! Max's video tooling already reads, so a sidecar can be used there directly.
//! Everything Thrasher adds is either optional or lives under `thrasher`.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    /// File name of the main (mic) track, without folder.
    pub file: String,
    /// Length in seconds.
    pub duration: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<Word>,
    /// Pauses between words, shown as `[silence:7s]` tags in the transcript.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub silences: Vec<Silence>,
    #[serde(default)]
    pub thrasher: Meta,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub id: String,
    pub text: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Silence {
    pub start: f64,
    pub end: f64,
    /// Seconds of this pause to keep when exporting; `None` keeps all of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub version: u32,
    pub state: RecordingState,
    /// Local start time, RFC 3339.
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub tracks: Vec<Track>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordingState {
    /// Still being written, or the app died while writing.
    Recording,
    #[default]
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Mic,
    System,
}

impl TrackKind {
    /// Suffix added to the clip name for this track's WAV file.
    pub fn file_suffix(self) -> &'static str {
        match self {
            TrackKind::Mic => "",
            TrackKind::System => ".system",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub kind: TrackKind,
    pub file: String,
    pub sample_rate: u32,
    pub channels: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

pub const VERSION: u32 = 1;

impl Sidecar {
    pub fn load(path: &Path) -> io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Writes to a temporary file first and renames it, so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_existing_transcript_format() {
        let json = r#"{"file": "clip_001.wav", "duration": 19.036,
            "words": [{"id": "w0", "text": "La", "start": 0.6, "end": 1.12}]}"#;
        let s: Sidecar = serde_json::from_str(json).unwrap();
        assert_eq!(s.file, "clip_001.wav");
        assert_eq!(s.words[0].text, "La");
        assert_eq!(s.thrasher.state, RecordingState::Complete);
    }

    #[test]
    fn save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.json");
        let s = Sidecar {
            file: "c.wav".into(),
            duration: 1.5,
            thrasher: Meta {
                version: VERSION,
                state: RecordingState::Recording,
                created: "2026-10-23T10:00:00+02:00".into(),
                tracks: vec![Track {
                    kind: TrackKind::Mic,
                    file: "c.wav".into(),
                    sample_rate: 48_000,
                    channels: 1,
                    device: None,
                }],
            },
            ..Default::default()
        };
        s.save(&p).unwrap();
        assert_eq!(Sidecar::load(&p).unwrap(), s);
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(!text.contains("words"));
    }
}
