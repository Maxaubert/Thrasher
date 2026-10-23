//! Thrasher's audio engine: capture, recording, playback and the clip library.
//! No UI code lives here.

pub mod capture;
pub mod library;
pub mod meter;
pub mod naming;
pub mod player;
pub mod recorder;
pub mod sidecar;
#[cfg(windows)]
pub mod wasapi_io;
pub mod wav;
