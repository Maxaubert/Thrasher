//! Settings stored as JSON in the user's config folder.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub output_dir: PathBuf,
    pub name_template: String,
    /// WASAPI device id; `None` follows the Windows default.
    pub mic_device: Option<String>,
    pub record_system: bool,
    pub pinned: bool,
    /// Starts and stops recording from any app, for example "Ctrl+Alt+R". Empty turns it off.
    pub hotkey: String,
    /// Id of the transcription model, from `thrasher_core::models::CATALOG`.
    pub model: String,
    /// Transcription language code such as "no"; empty means detect.
    pub language: String,
    /// Export format: "flac", "wav" or "mp3".
    pub export_format: String,
    /// Clean up the voice (noise suppression, automatic gain) when exporting.
    pub clean_voice: bool,
    /// Voice model for typed-in words: "omnivoice" (non-commercial) or "chatterbox" (MIT).
    pub voice_engine: String,
}

impl Default for Settings {
    fn default() -> Self {
        let music = directories::UserDirs::new()
            .and_then(|u| u.audio_dir().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            output_dir: music.join("VoiceClips"),
            name_template: thrasher_core::naming::DEFAULT_TEMPLATE.into(),
            mic_device: None,
            record_system: false,
            pinned: false,
            hotkey: "Ctrl+Alt+R".into(),
            model: "nb-whisper-large".into(),
            language: "no".into(),
            export_format: "flac".into(),
            clean_voice: false,
            voice_engine: "omnivoice".into(),
        }
    }
}

fn path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "Thrasher").map(|d| d.config_dir().join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, text);
        }
    }
}
