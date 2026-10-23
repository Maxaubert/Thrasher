//! Thrasher's audio engine: capture, recording, playback and the clip library.
//! No UI code lives here.

pub mod capture;
pub mod edit;
pub mod export;
pub mod library;
pub mod meter;
pub mod models;
pub mod naming;
pub mod player;
pub mod recorder;
pub mod sidecar;
pub mod transcribe;
#[cfg(windows)]
pub mod wasapi_io;
pub mod wav;
#[cfg(feature = "whisper")]
pub mod whisper;
