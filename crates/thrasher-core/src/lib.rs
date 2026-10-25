//! Thrasher's audio engine: capture, recording, playback and the clip library.
//! No UI code lives here.

pub mod bake;
pub mod capture;
pub mod cleanup;
pub mod edit;
pub mod export;
pub mod import;
pub mod library;
pub mod merge;
pub mod meter;
pub mod models;
pub mod naming;
pub mod player;
pub mod recorder;
pub mod sidecar;
pub mod stretch;
pub mod subtitles;
pub mod transcribe;
pub mod voice;
#[cfg(windows)]
pub mod wasapi_io;
pub mod wav;
#[cfg(feature = "whisper")]
pub mod whisper;
