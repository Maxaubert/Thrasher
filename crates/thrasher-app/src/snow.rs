//! The snow resting on a card for the Snow and Daylight themes, as SVG path commands
//! for a Slint `Path`. Slint cannot build a path in a loop, so the outline is worked
//! out here and only again when the card changes size.
//!
//! The snow is built from the card's own outline: up the left side, round the left
//! corner, along the top, round the right corner and down the right side. It grows
//! outward from that line by a lumpy thickness, and the part on the top hangs a little
//! over the card face, so snow and card meet with no gap.

/// Distance between outline points, in px.
const STEP: f32 = 2.0;
/// How much there is: the "Settled" look from the mockups (thickness, overhang).
const AMOUNT: f32 = 1.35;
const HANG: f32 = 1.1;

/// The pieces of the cap, each a closed path in card coordinates (px, origin at the
/// card's top left; the snow reaches above and beside it).
pub struct Cap {
    /// The snow itself.
    pub body: String,
    /// A sliver along the edge that hangs over the face, a touch cooler, so the front
    /// reads as snow curling over rather than a cut.
    pub under: String,
}

/// A small fixed random sequence, so a card keeps the same snow every time.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0 * 16807 % 2147483647;
        self.0 as f32 / 2147483647.0
    }
}

/// Smooth one-dimensional noise from a few sines with random phases, 0 to 1.
struct Noise(Vec<(f32, f32, f32)>);

impl Noise {
    fn new(rng: &mut Rng, octaves: &[(f32, f32)]) -> Self {
        Noise(
            octaves
                .iter()
                .map(|&(f, a)| {
                    (
                        f * (0.8 + rng.next() * 0.4),
                        a,
                        rng.next() * std::f32::consts::TAU,
                    )
                })
                .collect(),
        )
    }

    fn at(&self, x: f32) -> f32 {
        let total: f32 = self.0.iter().map(|o| o.1).sum();
        let sum: f32 = self.0.iter().map(|&(f, a, p)| a * (x * f + p).sin()).sum();
        0.5 + sum / (2.0 * total)
    }
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

enum Part {
    /// Down a side; 0 at the far end, 1 where the corner starts.
    Side(f32),
    /// Round a corner; 0 at the side, 1 at the top.
    Corner(f32),
    Top,
}

/// The cap for a card `w` wide with corner radius `r`. `scale` is 1 for the small
/// recorder and grows with a bigger card.
pub fn cap(w: f32, r: f32, scale: f32, seed: u64) -> Cap {
    let s = scale;
    let w = w.max(2.0 * r + 1.0);
    let mut rng = Rng(seed.max(1) % 2147483647);
    let top_n = Noise::new(
        &mut rng,
        &[(0.018 / s, 1.0), (0.045 / s, 0.6), (0.11 / s, 0.25)],
    );
    let lump_n = Noise::new(
        &mut rng,
        &[(0.03 / s, 1.0), (0.075 / s, 0.6), (0.17 / s, 0.2)],
    );
    // How far down each side the snow reaches, different per side.
    let reach_l = 0.3 + rng.next() * 0.7;
    let reach_r = 0.3 + rng.next() * 0.7;

    let side = 9.0 * s;
    let arc = std::f32::consts::FRAC_PI_2 * r;
    let straight = w - 2.0 * r;
    let len = 2.0 * side + 2.0 * arc + straight;
    let pi = std::f32::consts::PI;
    // A point on the outline, its outward normal, and which part it is on.
    let at = |u: f32| -> (f32, f32, f32, f32, Part, bool) {
        if u < side {
            return (0.0, r + side - u, -1.0, 0.0, Part::Side(u / side), true);
        }
        let u = u - side;
        if u < arc {
            let a = pi + u / arc * pi / 2.0;
            return (
                r + r * a.cos(),
                r + r * a.sin(),
                a.cos(),
                a.sin(),
                Part::Corner(u / arc),
                true,
            );
        }
        let u = u - arc;
        if u < straight {
            return (r + u, 0.0, 0.0, -1.0, Part::Top, true);
        }
        let u = u - straight;
        if u < arc {
            let a = 1.5 * pi + u / arc * pi / 2.0;
            let (x, y) = (w - r + r * a.cos(), r + r * a.sin());
            return (x, y, a.cos(), a.sin(), Part::Corner(1.0 - u / arc), false);
        }
        let u = (u - arc).min(side);
        (w, r + u, 1.0, 0.0, Part::Side(1.0 - u / side), false)
    };

    let mut outer = Vec::new();
    let mut inner = Vec::new();
    let mut rim = Vec::new();
    let mut u: f32 = 0.0;
    loop {
        let (x, y, nx, ny, part, left) = at(u.min(len - 0.001));
        // How much snow there is here: all of it on the top, easing off round the
        // corner, a short thin tail down the side that ends in a rounded tip.
        let (k, hk) = match part {
            Part::Top => (1.0, 1.0),
            Part::Corner(q) => (
                0.3 + 0.7 * smoothstep(q * 1.1),
                smoothstep((q - 0.35) / 0.65),
            ),
            Part::Side(q) => {
                let reach = if left { reach_l } else { reach_r };
                (0.3 * (1.0 - (1.0 - q) / reach).max(0.0).sqrt(), 0.0)
            }
        };
        let t = (1.6 + 9.0 * top_n.at(u).powf(1.7)) * s * AMOUNT * k;
        let h = (1.0 + 7.0 * lump_n.at(u).powf(1.9)) * s * HANG * hk;
        outer.push((x + nx * t, y + ny * t));
        inner.push((x - nx * h, y - ny * h));
        let d = 1.3 * s;
        rim.push((x - nx * (h - d).max(0.0), y - ny * (h - d).max(0.0)));
        if u >= len {
            break;
        }
        u = (u + STEP).min(len);
    }

    let mut body = polyline(&outer, true);
    for p in inner.iter().rev() {
        body.push_str(&format!("L{:.1} {:.1}", p.0, p.1));
    }
    body.push('Z');
    let mut under = polyline(&inner, true);
    for p in rim.iter().rev() {
        under.push_str(&format!("L{:.1} {:.1}", p.0, p.1));
    }
    under.push('Z');
    Cap { body, under }
}

fn polyline(points: &[(f32, f32)], start: bool) -> String {
    let mut out = String::with_capacity(points.len() * 14);
    for (i, p) in points.iter().enumerate() {
        let cmd = if i == 0 && start { 'M' } else { 'L' };
        out.push_str(&format!("{cmd}{:.1} {:.1}", p.0, p.1));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_card_same_snow() {
        let a = cap(400.0, 14.0, 1.0, 7);
        let b = cap(400.0, 14.0, 1.0, 7);
        assert_eq!(a.body, b.body);
        assert!(a.body.starts_with('M') && a.body.ends_with('Z'));
        assert!(a.under.starts_with('M') && a.under.ends_with('Z'));
    }

    #[test]
    fn stays_near_the_card() {
        // Every point within the snow's reach of the card: above by at most the
        // thickest snow, beside by at most a few px, below by at most the side.
        let c = cap(300.0, 14.0, 1.0, 3);
        let nums: Vec<f32> = c
            .body
            .split(['M', 'L', 'Z', ' '])
            .filter(|t| !t.is_empty())
            .map(|t| t.parse().unwrap())
            .collect();
        for xy in nums.chunks(2) {
            assert!(xy[0] > -6.0 && xy[0] < 306.0, "x {}", xy[0]);
            assert!(xy[1] > -18.0 && xy[1] < 40.0, "y {}", xy[1]);
        }
    }

    #[test]
    fn tiny_card_does_not_panic() {
        let c = cap(10.0, 14.0, 0.6, 1);
        assert!(c.body.len() > 10);
    }
}
