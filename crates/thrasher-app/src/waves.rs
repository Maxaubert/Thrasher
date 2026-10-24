//! Clip waveforms for the list and the clip page, read from the WAV files on a
//! background thread so a long library never holds up the window.

use crate::AppWindow;
use slint::ComponentHandle;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use thrasher_core::library::{Clip, Library};
use thrasher_core::sidecar::TrackKind;

/// Columns of the large waveform on the clip page; the list uses fewer.
pub const COLUMNS: usize = 90;

/// Waveforms by clip name, with the clip length they were drawn for: an edited
/// clip is rewritten in place and must be drawn again.
type Done = HashMap<String, (f64, Vec<f32>)>;

#[derive(Default)]
pub struct Waves {
    done: Arc<Mutex<Done>>,
    pending: HashSet<String>,
}

fn key(clip: &Clip) -> String {
    format!("{}|{}", clip.name, clip.duration)
}

impl Waves {
    pub fn get(&self, clip: &Clip) -> Option<Vec<f32>> {
        self.done
            .lock()
            .unwrap()
            .get(&clip.name)
            .filter(|(d, _)| (d - clip.duration).abs() < 1e-6)
            .map(|(_, w)| w.clone())
    }

    /// Starts drawing the clips that have no waveform yet. The window's
    /// `waves-ready` callback runs after each one.
    pub fn request(&mut self, ui: &AppWindow, library: &Library, clips: &[Clip]) {
        let todo: Vec<(String, f64, Vec<PathBuf>)> = clips
            .iter()
            .filter(|c| self.get(c).is_none() && !self.pending.contains(&key(c)))
            .map(|c| {
                let tracks = [TrackKind::Mic, TrackKind::System]
                    .into_iter()
                    .map(|k| library.track_path(&c.name, k))
                    .filter(|p| p.exists())
                    .collect();
                (c.name.clone(), c.duration, tracks)
            })
            .collect();
        if todo.is_empty() {
            return;
        }
        for (name, duration, _) in &todo {
            self.pending.insert(format!("{name}|{duration}"));
        }
        let (done, weak) = (self.done.clone(), ui.as_weak());
        let _ = std::thread::Builder::new()
            .name("thrasher-waves".into())
            // Newest clips are at the end of the list and the most likely to be seen.
            .spawn(move || {
                for (name, duration, tracks) in todo.into_iter().rev() {
                    let mut peaks = vec![0f32; COLUMNS];
                    for path in tracks {
                        if let Ok(p) = thrasher_core::wav::peaks(&path, COLUMNS) {
                            for (a, b) in peaks.iter_mut().zip(p) {
                                *a = a.max(b);
                            }
                        }
                    }
                    done.lock().unwrap().insert(name, (duration, shape(peaks)));
                    let _ = weak.upgrade_in_event_loop(|ui| ui.invoke_waves_ready());
                }
            });
    }
}

/// Scales a waveform to its own loudest point, so quiet clips still show their
/// shape, and lifts the quiet parts a little the way the ear hears them.
fn shape(peaks: Vec<f32>) -> Vec<f32> {
    let max = peaks.iter().copied().fold(0f32, f32::max);
    if max <= 1e-6 {
        return peaks;
    }
    peaks.into_iter().map(|v| (v / max).powf(0.7)).collect()
}

/// Reduces a waveform to `n` columns, keeping the peak of each group.
pub fn columns(wave: &[f32], n: usize) -> Vec<f32> {
    if wave.is_empty() || n == 0 {
        return vec![0.0; n];
    }
    (0..n)
        .map(|i| {
            let a = i * wave.len() / n;
            let b = ((i + 1) * wave.len() / n).max(a + 1).min(wave.len());
            wave[a..b].iter().copied().fold(0f32, f32::max)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_keep_peaks() {
        let w = [0.1, 0.9, 0.2, 0.3, 0.5, 0.4];
        assert_eq!(columns(&w, 3), vec![0.9, 0.3, 0.5]);
        assert_eq!(columns(&[], 2), vec![0.0, 0.0]);
    }

    #[test]
    fn shape_scales_to_the_loudest_point() {
        let s = shape(vec![0.0, 0.25, 0.5]);
        assert_eq!(s[2], 1.0);
        assert!(s[1] > 0.5 && s[1] < 1.0);
        assert_eq!(shape(vec![0.0, 0.0]), vec![0.0, 0.0]);
    }
}
