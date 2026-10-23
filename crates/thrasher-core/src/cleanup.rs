//! Voice cleanup for exports: high-pass filter, noise suppression and automatic gain,
//! using sonora (a Rust port of WebRTC's audio processing). The recorded file is never
//! changed; cleanup is applied to the mic track when exporting.

use sonora::config::{AdaptiveDigital, GainController2, HighPassFilter, NoiseSuppression};
use sonora::{AudioProcessing, Config, StreamConfig};

/// Cleans a mono or multichannel recording, 10 ms at a time. Each channel is
/// processed on its own.
pub fn clean_voice(samples: &[f32], sample_rate: u32, channels: u16) -> Result<Vec<f32>, String> {
    let ch = channels.max(1) as usize;
    let frame = (sample_rate / 100) as usize;
    let frames = samples.len() / ch;
    let mut out = vec![0f32; samples.len()];
    for c in 0..ch {
        let config = Config {
            high_pass_filter: Some(HighPassFilter::default()),
            noise_suppression: Some(NoiseSuppression::default()),
            gain_controller2: Some(GainController2 {
                adaptive_digital: Some(AdaptiveDigital::default()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut apm = AudioProcessing::builder()
            .config(config)
            .capture_config(StreamConfig::new(sample_rate, 1))
            .build();
        let mut src = vec![0f32; frame];
        let mut dst = vec![0f32; frame];
        let mut pos = 0;
        while pos < frames {
            let n = frame.min(frames - pos);
            src.fill(0.0);
            for i in 0..n {
                src[i] = samples[(pos + i) * ch + c];
            }
            apm.process_capture_f32(&[&src], &mut [&mut dst])
                .map_err(|e| format!("voice cleanup failed: {e:?}"))?;
            for i in 0..n {
                out[(pos + i) * ch + c] = dst[i];
            }
            pos += n;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduces_steady_noise_and_keeps_length() {
        let rate = 48_000u32;
        // Deterministic white-ish noise, 3 s.
        let mut seed = 1u32;
        let noise: Vec<f32> = (0..rate as usize * 3)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((seed >> 8) as f32 / (1 << 24) as f32 - 0.5) * 0.05
            })
            .collect();
        let out = clean_voice(&noise, rate, 1).unwrap();
        assert_eq!(out.len(), noise.len());
        let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();
        // Compare the last second, after the suppressor has learned the noise.
        let tail = rate as usize * 2..;
        assert!(rms(&out[tail.clone()]) < rms(&noise[tail]) * 0.7);
    }
}
