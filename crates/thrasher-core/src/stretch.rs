//! Playback speed without changing the pitch, by WSOLA (waveform-similarity
//! overlap-add): short windowed frames are taken from the input at the new speed and
//! each is nudged to where it lines up best with the previous one.

use crate::player::Buffer;

/// Plays `buffer` at `speed` (0.5 to 2.0): 1.5 is 50 % faster at the same pitch.
pub fn stretch(buffer: &Buffer, speed: f64) -> Buffer {
    let speed = speed.clamp(0.5, 2.0);
    let ch = buffer.channels.max(1) as usize;
    let frames = buffer.samples.len() / ch;
    let rate = buffer.sample_rate.max(1) as usize;
    // 40 ms frames, half overlap; a periodic Hann window then sums to one.
    let hs = (rate / 50).max(2);
    let n = hs * 2;
    if (speed - 1.0).abs() < 1e-3 || frames < n * 2 {
        return buffer.clone();
    }
    let ha = hs as f64 * speed;
    let tol = rate * 6 / 1000;
    // Correlation runs on a mono mix at about 6 kHz, which is plenty for speech.
    let step = (rate / 6000).max(1);
    let mono: Vec<f32> = buffer
        .samples
        .chunks_exact(ch)
        .map(|f| f.iter().sum::<f32>())
        .collect();
    let window: Vec<f32> = (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos())
        .collect();

    let out_frames = (frames as f64 / speed) as usize + n;
    let mut out = vec![0f32; out_frames * ch];
    let mut prev = 0usize;
    let mut k = 0usize;
    loop {
        let target = (k as f64 * ha) as usize;
        let pos = if k == 0 {
            0
        } else {
            best_match(&mono, prev + hs, target, tol, hs, step, frames - n)
        };
        if pos + n > frames || k * hs + n > out_frames {
            break;
        }
        let o = k * hs;
        for i in 0..n {
            let w = window[i];
            for c in 0..ch {
                out[(o + i) * ch + c] += buffer.samples[(pos + i) * ch + c] * w;
            }
        }
        prev = pos;
        k += 1;
    }
    out.truncate(((frames as f64 / speed) as usize).min(out_frames) * ch);
    Buffer {
        sample_rate: buffer.sample_rate,
        channels: buffer.channels,
        samples: out,
    }
}

/// The start near `target` whose first `len` samples best match those at `natural`,
/// the point that would continue the previous frame seamlessly.
fn best_match(
    mono: &[f32],
    natural: usize,
    target: usize,
    tol: usize,
    len: usize,
    step: usize,
    max: usize,
) -> usize {
    let lo = target.saturating_sub(tol);
    let hi = (target + tol).min(max);
    if natural + len > mono.len() || lo > hi {
        return target.min(max);
    }
    let reference = &mono[natural..natural + len];
    let mut best = (f32::MIN, target.min(max));
    let mut c = lo;
    while c <= hi {
        let cand = &mono[c..c + len];
        let score: f32 = (0..len).step_by(step).map(|j| reference[j] * cand[j]).sum();
        if score > best.0 {
            best = (score, c);
        }
        c += step;
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, secs: f32, rate: u32, channels: u16) -> Buffer {
        let frames = (secs * rate as f32) as usize;
        let samples = (0..frames)
            .flat_map(|i| {
                let s = (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin() * 0.5;
                std::iter::repeat_n(s, channels as usize)
            })
            .collect();
        Buffer {
            sample_rate: rate,
            channels,
            samples,
        }
    }

    fn crossings(b: &Buffer) -> usize {
        let ch = b.channels as usize;
        let mono: Vec<f32> = b.samples.iter().step_by(ch).copied().collect();
        mono.windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count()
    }

    #[test]
    fn normal_speed_is_unchanged() {
        let b = sine(220.0, 1.0, 48_000, 1);
        assert_eq!(stretch(&b, 1.0), b);
    }

    #[test]
    fn length_follows_speed() {
        let b = sine(220.0, 2.0, 48_000, 2);
        for speed in [0.75, 1.5, 2.0] {
            let out = stretch(&b, speed);
            let want = b.duration_secs() / speed;
            assert!(
                (out.duration_secs() - want).abs() < 0.05,
                "{speed}: {}",
                out.duration_secs()
            );
            assert_eq!(out.channels, 2);
        }
    }

    #[test]
    fn pitch_stays_the_same() {
        let b = sine(300.0, 2.0, 48_000, 1);
        let out = stretch(&b, 1.5);
        // Same frequency means the same number of cycles per second of output.
        let per_sec_in = crossings(&b) as f64 / b.duration_secs();
        let per_sec_out = crossings(&out) as f64 / out.duration_secs();
        assert!(
            (per_sec_out - per_sec_in).abs() / per_sec_in < 0.05,
            "{per_sec_in} vs {per_sec_out}"
        );
    }

    #[test]
    fn short_audio_is_left_alone() {
        let b = sine(220.0, 0.05, 48_000, 1);
        assert_eq!(stretch(&b, 1.5), b);
    }
}
