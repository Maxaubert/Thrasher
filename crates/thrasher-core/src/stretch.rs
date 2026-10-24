//! Playback speed without changing the pitch, by WSOLA (waveform-similarity
//! overlap-add): short windowed frames are taken from the input at the new speed and
//! each is nudged to where it lines up best with the previous one.

use crate::player::Buffer;

/// Plays `buffer` at `speed` (0.5 to 2.0): 1.5 is 50 % faster at the same pitch.
pub fn stretch(buffer: &Buffer, speed: f64) -> Buffer {
    let ch = buffer.channels.max(1) as usize;
    let Some(mut s) = Stretcher::new(&buffer.samples, ch, buffer.sample_rate, 0, speed) else {
        return buffer.clone();
    };
    let mut samples = Vec::new();
    while s.read(&buffer.samples, 4096, &mut samples) > 0 {}
    Buffer {
        sample_rate: buffer.sample_rate,
        channels: buffer.channels,
        samples,
    }
}

/// Stretches a piece at a time, so playback can start at once on a long clip.
/// Each `read` passes the same interleaved samples the stretcher was made for.
pub struct Stretcher {
    ch: usize,
    frames: usize,
    from: usize,
    hs: usize,
    n: usize,
    ha: f64,
    tol: usize,
    step: usize,
    window: Vec<f32>,
    /// The next frame to place, and where the previous one was taken from.
    k: usize,
    prev: usize,
    /// Output not yet handed out, starting at output frame `base`.
    pending: Vec<f32>,
    base: usize,
    /// Output length: the input from `from` on, divided by the speed.
    end: usize,
    done: bool,
}

impl Stretcher {
    /// None when `speed` is normal or the audio after `from` is too short to stretch.
    pub fn new(samples: &[f32], ch: usize, rate: u32, from: usize, speed: f64) -> Option<Self> {
        let speed = speed.clamp(0.5, 2.0);
        let ch = ch.max(1);
        let frames = samples.len() / ch;
        let rate = rate.max(1) as usize;
        // 40 ms frames, half overlap; a periodic Hann window then sums to one.
        let hs = (rate / 50).max(2);
        let n = hs * 2;
        if (speed - 1.0).abs() < 1e-3 || frames < from + n * 2 {
            return None;
        }
        let window = (0..n)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos())
            .collect();
        Some(Self {
            ch,
            frames,
            from,
            hs,
            n,
            ha: hs as f64 * speed,
            tol: rate * 6 / 1000,
            // Correlation runs on a mono mix at about 6 kHz, which is plenty for speech.
            step: (rate / 6000).max(1),
            window,
            k: 0,
            prev: from,
            pending: Vec::new(),
            base: 0,
            end: ((frames - from) as f64 / speed) as usize,
            done: false,
        })
    }

    /// Appends up to `frames` stretched frames to `out`; returns how many, 0 at the end.
    pub fn read(&mut self, samples: &[f32], frames: usize, out: &mut Vec<f32>) -> usize {
        loop {
            // Output before frame k's start gets nothing more added.
            let ready = if self.done {
                self.end
            } else {
                (self.k * self.hs).min(self.end)
            };
            let have = ready.saturating_sub(self.base);
            if have >= frames || self.done {
                let m = have.min(frames);
                let len = m * self.ch;
                if self.pending.len() < len {
                    self.pending.resize(len, 0.0);
                }
                out.extend(self.pending.drain(..len));
                self.base += m;
                return m;
            }
            self.place(samples);
        }
    }

    fn place(&mut self, samples: &[f32]) {
        let (ch, n) = (self.ch, self.n);
        let target = self.from + (self.k as f64 * self.ha) as usize;
        if target + n > self.frames || self.k * self.hs >= self.end {
            self.done = true;
            return;
        }
        let pos = if self.k == 0 {
            self.from
        } else {
            self.best_match(samples, self.prev + self.hs, target)
        };
        if pos + n > self.frames {
            self.done = true;
            return;
        }
        let o = (self.k * self.hs - self.base) * ch;
        if self.pending.len() < o + n * ch {
            self.pending.resize(o + n * ch, 0.0);
        }
        for i in 0..n {
            let w = self.window[i];
            for c in 0..ch {
                self.pending[o + i * ch + c] += samples[(pos + i) * ch + c] * w;
            }
        }
        self.prev = pos;
        self.k += 1;
    }

    /// The start near `target` whose first frames best match those at `natural`,
    /// the point that would continue the previous frame seamlessly.
    fn best_match(&self, samples: &[f32], natural: usize, target: usize) -> usize {
        let (ch, len, step) = (self.ch, self.hs, self.step);
        let max = self.frames - self.n;
        let lo = target.saturating_sub(self.tol);
        let hi = (target + self.tol).min(max);
        if natural + len > self.frames || lo > hi {
            return target.min(max);
        }
        let mono = |f: usize| samples[f * ch..f * ch + ch].iter().sum::<f32>();
        let reference: Vec<f32> = (0..len).step_by(step).map(|j| mono(natural + j)).collect();
        let mut best = (f32::MIN, target.min(max));
        let mut c = lo;
        while c <= hi {
            let score: f32 = (0..len)
                .step_by(step)
                .zip(&reference)
                .map(|(j, r)| r * mono(c + j))
                .sum();
            if score > best.0 {
                best = (score, c);
            }
            c += step;
        }
        best.1
    }
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
    fn reading_in_pieces_matches_reading_at_once() {
        let b = sine(220.0, 1.0, 48_000, 2);
        let mut s = Stretcher::new(&b.samples, 2, 48_000, 4_800, 1.5).unwrap();
        let mut pieces = Vec::new();
        while s.read(&b.samples, 333, &mut pieces) > 0 {}
        let mut s = Stretcher::new(&b.samples, 2, 48_000, 4_800, 1.5).unwrap();
        let mut whole = Vec::new();
        s.read(&b.samples, usize::MAX / 4, &mut whole);
        assert_eq!(pieces, whole);
        assert_eq!(pieces.len() / 2, (43_200.0 / 1.5) as usize);
    }

    #[test]
    fn short_audio_is_left_alone() {
        let b = sine(220.0, 0.05, 48_000, 1);
        assert_eq!(stretch(&b, 1.5), b);
    }
}
