//! Non-destructive edits: shortening pauses. The recording is never changed; the
//! sidecar stores how much of each pause to keep, and export renders the result.
//!
//! In the transcript text a pause shows as `[silence:7s]`. Editing that tag to
//! `[silence:2s]` and applying the text sets `keep` for that pause to 2 s.

use crate::player::Buffer;
use crate::sidecar::Silence;

/// Crossfade length at each cut.
const FADE_SECS: f64 = 0.015;

/// Length a pause will have after editing.
pub fn kept(s: &Silence) -> f64 {
    let full = s.end - s.start;
    s.keep.map_or(full, |k| k.clamp(0.0, full))
}

/// Formats a pause tag, for example `[silence:7s]` or `[silence:1.5s]`.
pub fn tag(secs: f64) -> String {
    if secs >= 10.0 {
        format!("[silence:{}s]", secs.round())
    } else {
        format!("[silence:{secs:.1}s]").replace(".0s", "s")
    }
}

/// Reads the pause tags in edited transcript text, in order, and sets `keep` on the
/// matching pauses. Returns how many pauses changed. Text that is not a tag is
/// ignored, so editing words does not break anything here.
pub fn apply_tags(text: &str, silences: &mut [Silence]) -> Result<usize, String> {
    let mut values = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("[silence:") {
        let after = &rest[i + "[silence:".len()..];
        let end = after.find(']').ok_or("a [silence: tag is not closed")?;
        let inner = after[..end].trim().trim_end_matches('s').trim();
        let v: f64 = inner
            .replace(',', ".")
            .parse()
            .map_err(|_| format!("'{}' is not a number of seconds", &after[..end]))?;
        if v < 0.0 {
            return Err("a pause cannot be negative".into());
        }
        values.push(v);
        rest = &after[end..];
    }
    if values.len() != silences.len() {
        return Err(format!(
            "found {} pause tags but the transcript has {}; delete a pause by setting it to 0s",
            values.len(),
            silences.len()
        ));
    }
    let mut changed = 0;
    for (s, v) in silences.iter_mut().zip(values) {
        let full = s.end - s.start;
        // Tags are rounded for display, so only a clear change counts as an edit.
        let shown: f64 = tag(kept(s))
            .trim_start_matches("[silence:")
            .trim_end_matches("s]")
            .parse()
            .unwrap_or(full);
        if (v - shown).abs() < 1e-9 {
            continue;
        }
        let new = if v >= full { None } else { Some(v) };
        if new != s.keep {
            s.keep = new;
            changed += 1;
        }
    }
    Ok(changed)
}

/// Renders the buffer with every shortened pause cut down. Half of the kept pause
/// stays on each side of the cut, joined with a short equal-power crossfade.
pub fn render(buffer: &Buffer, silences: &[Silence]) -> Buffer {
    let rate = buffer.sample_rate as f64;
    let ch = buffer.channels.max(1) as usize;
    let frames = buffer.frames() as usize;
    let fade = (FADE_SECS * rate) as usize;
    // Ranges of frames to remove, in order.
    let mut cuts: Vec<(usize, usize)> = silences
        .iter()
        .filter(|s| kept(s) < s.end - s.start - 1e-6)
        .map(|s| {
            let half = kept(s) / 2.0;
            let a = ((s.start + half) * rate) as usize;
            let b = ((s.end - half) * rate) as usize;
            (a.min(frames), b.min(frames))
        })
        .filter(|(a, b)| b > a)
        .collect();
    cuts.sort();
    let mut out: Vec<f32> = Vec::with_capacity(buffer.samples.len());
    let mut pos = 0usize;
    for (a, b) in cuts {
        if a < pos {
            continue;
        }
        out.extend_from_slice(&buffer.samples[pos * ch..a * ch]);
        // Crossfade the audio before the cut into the audio after it.
        let n = fade.min(a - pos).min(frames - b);
        let start = out.len() / ch - n;
        for i in 0..n {
            let t = (i as f64 + 0.5) / n as f64;
            let (g_out, g_in) = (
                (t * std::f64::consts::FRAC_PI_2).cos(),
                (t * std::f64::consts::FRAC_PI_2).sin(),
            );
            for c in 0..ch {
                let o = &mut out[(start + i) * ch + c];
                *o = (*o as f64 * g_out + buffer.samples[(b + i) * ch + c] as f64 * g_in) as f32;
            }
        }
        pos = b + n;
    }
    out.extend_from_slice(&buffer.samples[pos.min(frames) * ch..]);
    Buffer {
        sample_rate: buffer.sample_rate,
        channels: buffer.channels,
        samples: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sil(start: f64, end: f64) -> Silence {
        Silence {
            start,
            end,
            keep: None,
        }
    }

    #[test]
    fn tags_round_trip() {
        assert_eq!(tag(7.0), "[silence:7s]");
        assert_eq!(tag(1.5), "[silence:1.5s]");
        assert_eq!(tag(12.4), "[silence:12s]");
    }

    #[test]
    fn apply_edited_tags() {
        let mut s = vec![sil(1.0, 8.0), sil(10.0, 11.5)];
        let changed = apply_tags("Hei [silence:2s] der [silence:1.5s] ja", &mut s).unwrap();
        assert_eq!(changed, 1);
        assert_eq!(s[0].keep, Some(2.0));
        assert_eq!(s[1].keep, None);
        // Setting it back to the full length removes the edit.
        apply_tags("[silence:7s] [silence:1.5s]", &mut s).unwrap();
        assert_eq!(s[0].keep, None);
        assert!(apply_tags("[silence:2s]", &mut s).is_err());
        assert!(apply_tags("[silence:abc] [silence:1s]", &mut s).is_err());
        assert!(apply_tags("[silence:2,5s] [silence:1s]", &mut s).is_ok());
        assert_eq!(s[0].keep, Some(2.5));
    }

    #[test]
    fn render_shortens_pause() {
        let rate = 1000;
        // 1 s of 0.5, 4 s of silence, 1 s of -0.5.
        let mut samples = vec![0.5f32; rate];
        samples.extend(vec![0.0; 4 * rate]);
        samples.extend(vec![-0.5; rate]);
        let buf = Buffer {
            sample_rate: rate as u32,
            channels: 1,
            samples,
        };
        let mut s = sil(1.0, 5.0);
        s.keep = Some(1.0);
        let out = render(&buf, &[s]);
        // 6 s minus the 3 s removed, minus the overlapped crossfade.
        let expected = 3 * rate - (FADE_SECS * rate as f64) as usize;
        assert_eq!(out.samples.len(), expected);
        assert_eq!(out.samples[0], 0.5);
        assert_eq!(*out.samples.last().unwrap(), -0.5);
        // Unedited pauses leave the audio untouched.
        assert_eq!(render(&buf, &[sil(1.0, 5.0)]).samples, buf.samples);
    }
}
