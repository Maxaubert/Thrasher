//! Settings stored as JSON in the user's config folder.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Where the window was and how big (size in logical pixels, position in screen
/// pixels), so it opens the same way next time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowPlace {
    pub x: i32,
    pub y: i32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub output_dir: PathBuf,
    pub name_template: String,
    /// WASAPI device id; `None` follows the Windows default.
    pub mic_device: Option<String>,
    pub record_system: bool,
    pub pinned: bool,
    /// Starts and stops recording from any app, for example "Ctrl+Alt+R". Empty (the
    /// default) turns it off.
    pub hotkey: String,
    /// Id of the transcription model, from `thrasher_core::models::CATALOG`.
    pub model: String,
    /// Transcription language code such as "no"; empty means detect.
    pub language: String,
    /// Export format: "flac", "wav" or "mp3".
    pub export_format: String,
    /// The folder the last export was saved in; the save dialog starts there.
    pub export_dir: Option<PathBuf>,
    /// Clean up the voice (noise suppression, automatic gain) when exporting.
    pub clean_voice: bool,
    /// The window's close button hides Thrasher to the tray instead of quitting.
    pub close_to_tray: bool,
    /// A recording stops by itself after this many minutes, so one left running by
    /// mistake cannot fill the disk. 0 means no limit.
    pub max_recording_minutes: u32,
    /// The full window's last size and place; none until it has been moved or resized.
    pub window: Option<WindowPlace>,
    /// Name of the colour theme, one of `Theme.names` in the UI.
    pub theme: String,
    /// The project new clips go into; empty is Inbox, the clip folder itself.
    pub project: String,
    /// New clips (recordings, imports, dropped files) are transcribed right away.
    pub auto_transcribe: bool,
    /// After a transcription, longer pauses are shortened to this many seconds. 0 is off.
    pub max_pause: f64,
    /// Id of the local AI model, from `thrasher_core::ai::AI_MODELS`.
    pub ai_model: String,
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
            hotkey: String::new(),
            model: "nb-whisper-large".into(),
            language: "no".into(),
            export_format: "flac".into(),
            export_dir: None,
            clean_voice: false,
            close_to_tray: true,
            max_recording_minutes: 120,
            window: None,
            theme: "Ember".into(),
            project: String::new(),
            auto_transcribe: false,
            max_pause: 0.0,
            ai_model: "borealis-open-4b".into(),
        }
    }
}

/// `THRASHER_SETTINGS` points tests at their own settings file, so they never change
/// the user's.
fn path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("THRASHER_SETTINGS") {
        return Some(PathBuf::from(p));
    }
    directories::ProjectDirs::from("", "", "Thrasher").map(|d| d.config_dir().join("settings.json"))
}

/// Choices for the recording limit in settings: minutes and label. 0 is no limit.
pub const RECORDING_LIMITS: [(u32, &str); 6] = [
    (30, "30 minutes"),
    (60, "1 hour"),
    (120, "2 hours"),
    (240, "4 hours"),
    (480, "8 hours"),
    (0, "No limit"),
];

/// Choices for the longest pause in settings: seconds and label. 0 is off.
pub const MAX_PAUSES: [(f64, &str); 5] = [
    (0.0, "Off"),
    (0.3, "0.3 s"),
    (0.5, "0.5 s"),
    (1.0, "1 s"),
    (2.0, "2 s"),
];

/// True when a recording of `elapsed` seconds has reached the limit.
pub fn over_limit(elapsed: f64, max_minutes: u32) -> bool {
    max_minutes > 0 && elapsed >= max_minutes as f64 * 60.0
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_limit() {
        assert!(!over_limit(7199.9, 120));
        assert!(over_limit(7200.0, 120));
        assert!(!over_limit(1e9, 0));
        assert_eq!(Settings::default().max_recording_minutes, 120);
    }

    #[test]
    fn old_settings_files_get_the_default_limit() {
        let s: Settings = serde_json::from_str(r#"{"pinned": true}"#).unwrap();
        assert!(s.pinned);
        assert_eq!(s.max_recording_minutes, 120);
    }
}
