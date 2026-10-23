//! Level metering and waveform summaries.

use std::sync::atomic::{AtomicU32, Ordering};

/// Peak and RMS of the most recent block, shared between the writer thread and the UI
/// without locks.
#[derive(Debug, Default)]
pub struct SharedLevel {
    peak: AtomicU32,
    rms: AtomicU32,
    clipped: AtomicU32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Level {
    /// Highest absolute sample value, linear 0..=1 (can exceed 1 for float input).
    pub peak: f32,
    pub rms: f32,
    /// Number of blocks so far that touched full scale.
    pub clipped: u32,
}

impl SharedLevel {
    pub fn store(&self, block: &[f32]) {
        let (peak, rms) = peak_rms(block);
        self.peak.store(peak.to_bits(), Ordering::Relaxed);
        self.rms.store(rms.to_bits(), Ordering::Relaxed);
        if peak >= 0.999 {
            self.clipped.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn reset(&self) {
        self.peak.store(0, Ordering::Relaxed);
        self.rms.store(0, Ordering::Relaxed);
    }

    pub fn load(&self) -> Level {
        Level {
            peak: f32::from_bits(self.peak.load(Ordering::Relaxed)),
            rms: f32::from_bits(self.rms.load(Ordering::Relaxed)),
            clipped: self.clipped.load(Ordering::Relaxed),
        }
    }
}

pub fn peak_rms(block: &[f32]) -> (f32, f32) {
    if block.is_empty() {
        return (0.0, 0.0);
    }
    let mut peak = 0f32;
    let mut sum = 0f64;
    for &s in block {
        peak = peak.max(s.abs());
        sum += (s as f64) * (s as f64);
    }
    (peak, (sum / block.len() as f64).sqrt() as f32)
}

/// Converts a linear level to a 0..=1 meter position over a range of `floor_db` to 0 dBFS.
pub fn meter_position(linear: f32, floor_db: f32) -> f32 {
    if linear <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * linear.log10();
    ((db - floor_db) / -floor_db).clamp(0.0, 1.0)
}

/// Reduces interleaved audio to `buckets` peak values (0..=1), one per column of a
/// waveform drawing. All channels are folded together.
pub fn waveform(samples: &[f32], channels: usize, buckets: usize) -> Vec<f32> {
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    if frames == 0 || buckets == 0 {
        return vec![0.0; buckets];
    }
    (0..buckets)
        .map(|b| {
            let start = b * frames / buckets;
            let end = ((b + 1) * frames / buckets).max(start + 1).min(frames);
            samples[start * channels..end * channels]
                .iter()
                .fold(0f32, |m, s| m.max(s.abs()))
                .min(1.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_and_rms() {
        let (p, r) = peak_rms(&[0.5, -1.0, 0.5, 0.0]);
        assert_eq!(p, 1.0);
        assert!((r - (1.5f32 / 4.0).sqrt()).abs() < 1e-6);
    }

    #[test]
    fn shared_level_counts_clips() {
        let l = SharedLevel::default();
        l.store(&[0.1, 1.0]);
        l.store(&[0.1]);
        let v = l.load();
        assert_eq!(v.clipped, 1);
        assert!((v.peak - 0.1).abs() < 1e-6);
    }

    #[test]
    fn meter_scale() {
        assert_eq!(meter_position(1.0, -60.0), 1.0);
        assert_eq!(meter_position(0.0, -60.0), 0.0);
        assert!((meter_position(0.001, -60.0)).abs() < 1e-6);
        assert!((meter_position(0.031_622_8, -60.0) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn waveform_buckets() {
        let s = [0.1, 0.9, 0.2, 0.3, -0.5, 0.0];
        assert_eq!(waveform(&s, 1, 3), vec![0.9, 0.3, 0.5]);
        assert_eq!(waveform(&s, 2, 1), vec![0.9]);
        assert_eq!(waveform(&[], 1, 2), vec![0.0, 0.0]);
    }
}
