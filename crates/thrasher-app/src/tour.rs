//! The route the Cyberpunk snakes run on the main page: round the recorder card,
//! through the gap to the toolbar card, round the highlighted clip, out to the window
//! frame and back into the recorder. Each box is lapped once from where the snakes
//! come in to where they leave; the stretches across the gaps are tunnels, and they
//! only leave a box straight on from a side, never across the middle of an edge.
//!
//! Slint asks for it (`Tour.route`) with the boxes in window px, and only again when
//! one of them moves.

/// Distance between route points, in px.
const STEP: f32 = 2.0;

/// A rounded box in window px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub r: f32,
}

impl Rect {
    fn bottom(&self) -> f32 {
        self.y + self.h
    }
    fn right(&self) -> f32 {
        self.x + self.w
    }

    /// The outline, every STEP px, clockwise from the left end of the top edge.
    fn outline(&self) -> Vec<(f32, f32)> {
        let Rect { x, y, w, h, .. } = *self;
        let r = self.r.min(w / 2.0).min(h / 2.0).max(0.0);
        let mut pts = Vec::new();
        let seg = |a: (f32, f32), b: (f32, f32), pts: &mut Vec<(f32, f32)>| {
            let n = ((b.0 - a.0).hypot(b.1 - a.1) / STEP).round().max(1.0) as usize;
            for i in 0..n {
                let t = i as f32 / n as f32;
                pts.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
            }
        };
        let arc = |cx: f32, cy: f32, a0: f32, pts: &mut Vec<(f32, f32)>| {
            let n = (std::f32::consts::FRAC_PI_2 * r / STEP).round().max(2.0) as usize;
            for i in 0..n {
                let a = a0 + i as f32 / n as f32 * std::f32::consts::FRAC_PI_2;
                pts.push((cx + r * a.cos(), cy + r * a.sin()));
            }
        };
        use std::f32::consts::PI;
        seg((x + r, y), (x + w - r, y), &mut pts);
        arc(x + w - r, y + r, -PI / 2.0, &mut pts);
        seg((x + w, y + r), (x + w, y + h - r), &mut pts);
        arc(x + w - r, y + h - r, 0.0, &mut pts);
        seg((x + w - r, y + h), (x + r, y + h), &mut pts);
        arc(x + r, y + h - r, PI / 2.0, &mut pts);
        seg((x, y + h - r), (x, y + r), &mut pts);
        arc(x + r, y + r, PI, &mut pts);
        pts
    }
}

/// A tunnel: from box `.0` at point `.1` to box `.2` at point `.3`.
type Link = (usize, (f32, f32), usize, (f32, f32));

/// A point of the route; `tunnel` across a gap between boxes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    pub tunnel: bool,
}

fn nearest(pts: &[(f32, f32)], p: (f32, f32)) -> usize {
    let d = |q: &(f32, f32)| (q.0 - p.0).powi(2) + (q.1 - p.1).powi(2);
    (0..pts.len())
        .min_by(|&a, &b| d(&pts[a]).total_cmp(&d(&pts[b])))
        .unwrap_or(0)
}

/// The route. `sel` is the highlighted clip's row, `None` when there is none or it is
/// scrolled out of view; `wide` puts the toolbar beside the recorder instead of under it.
pub fn route(
    rec: Rect,
    tool: Rect,
    sel: Option<Rect>,
    win_w: f32,
    win_h: f32,
    wide: bool,
) -> Vec<Point> {
    if rec.w < 1.0 || tool.w < 1.0 || win_w < 40.0 || win_h < 40.0 {
        return Vec::new();
    }
    let frame = Rect {
        x: 4.0,
        y: 4.0,
        w: win_w - 8.0,
        h: win_h - 8.0,
        r: 8.0,
    };
    // 0 recorder, 1 toolbar, 2 highlighted row, 3 frame.
    let boxes = [rec, tool, sel.unwrap_or(tool), frame];
    // Tunnels: (from box, point on it) to (to box, point on it). Every box is run
    // clockwise, so a snake only hops where it is already heading that way: straight on
    // off the end of a side (down a right side, up a left side, along a top edge), into
    // the matching side of the next box.
    let mut links: Vec<Link> = Vec::new();
    if !wide {
        // Down the recorder's right side and on into the toolbar's.
        links.push((
            0,
            (rec.right(), rec.bottom() - rec.r),
            1,
            (tool.right(), tool.y + tool.r),
        ));
    } else {
        // Along the recorder's top and on along the toolbar's, beside it.
        links.push((
            0,
            (rec.right() - rec.r, rec.y),
            1,
            (tool.x + tool.r, tool.y),
        ));
    }
    // Down the right sides: toolbar, then the highlighted row, then a step across into
    // the window frame's right side, still heading down.
    let last = match sel {
        Some(s) => {
            links.push((
                1,
                (tool.right(), tool.bottom() - tool.r),
                2,
                (s.right(), s.y + s.r),
            ));
            (2, s)
        }
        None => (1, tool),
    };
    let (i, b) = last;
    let y = b.bottom() - b.r;
    links.push((i, (b.right(), y), 3, (frame.right(), y + 10.0)));
    // Round the frame and up its left side, then a step across into the recorder's
    // left side, still heading up.
    let y = if wide {
        rec.y + rec.h * 0.6
    } else {
        rec.bottom() - rec.r - 2.0
    };
    links.push((3, (frame.x, y + 10.0), 0, (rec.x, y)));

    let outlines: Vec<Vec<(f32, f32)>> = boxes.iter().map(Rect::outline).collect();
    let mut out = Vec::new();
    for i in 0..links.len() {
        let enter = links[(i + links.len() - 1) % links.len()];
        let leave = links[i];
        let pts = &outlines[leave.0];
        let n = pts.len();
        if n == 0 {
            continue;
        }
        let a = nearest(pts, enter.3);
        let b = nearest(pts, leave.1);
        // Once all the way round, then on to where the tunnel starts.
        let steps = (b + n - a) % n + n;
        for j in 0..steps {
            let p = pts[(a + j) % n];
            out.push(Point {
                x: p.0,
                y: p.1,
                tunnel: false,
            });
        }
        let (from, to) = (pts[b], leave.3);
        let m = ((to.0 - from.0).hypot(to.1 - from.1) / STEP)
            .round()
            .max(1.0) as usize;
        for j in 0..m {
            let t = j as f32 / m as f32;
            out.push(Point {
                x: from.0 + (to.0 - from.0) * t,
                y: from.1 + (to.1 - from.1) * t,
                tunnel: true,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect {
            x,
            y,
            w,
            h,
            r: 14.0,
        }
    }

    #[test]
    fn route_is_continuous() {
        // Portrait: recorder over toolbar over a highlighted row.
        let r = route(
            bx(12.0, 40.0, 560.0, 130.0),
            bx(12.0, 186.0, 560.0, 52.0),
            Some(bx(24.0, 300.0, 536.0, 52.0)),
            585.0,
            788.0,
            false,
        );
        assert!(r.len() > 1000);
        for w in r.windows(2) {
            let d = (w[1].x - w[0].x).hypot(w[1].y - w[0].y);
            assert!(d < 6.0, "jump of {d} at {:?}", w[0]);
        }
        let last = r[r.len() - 1];
        assert!(
            (last.x - r[0].x).hypot(last.y - r[0].y) < 6.0,
            "the route closes"
        );
        assert!(r.iter().any(|p| p.tunnel) && r.iter().any(|p| !p.tunnel));
    }

    #[test]
    fn hops_leave_from_the_sides() {
        // In portrait every tunnel starts on a left or right side of a box, where the
        // snake is already heading up or down, never across the middle of an edge.
        let (rec, tool, sel) = (
            bx(12.0, 40.0, 560.0, 130.0),
            bx(12.0, 186.0, 560.0, 52.0),
            bx(24.0, 300.0, 536.0, 52.0),
        );
        let r = route(rec, tool, Some(sel), 585.0, 788.0, false);
        let sides = [
            rec.x,
            rec.right(),
            tool.x,
            tool.right(),
            sel.x,
            sel.right(),
            4.0,
            581.0,
        ];
        let starts: Vec<&Point> = r
            .windows(2)
            .filter(|w| !w[0].tunnel && w[1].tunnel)
            .map(|w| &w[1])
            .collect();
        assert_eq!(starts.len(), 4);
        for p in starts {
            assert!(
                sides.iter().any(|x| (p.x - x).abs() < 2.5),
                "tunnel from the middle of an edge at {p:?}"
            );
        }
    }

    #[test]
    fn wide_without_a_row() {
        let r = route(
            bx(14.0, 40.0, 410.0, 740.0),
            bx(442.0, 40.0, 540.0, 52.0),
            None,
            1000.0,
            860.0,
            true,
        );
        assert!(!r.is_empty());
        for w in r.windows(2) {
            assert!((w[1].x - w[0].x).hypot(w[1].y - w[0].y) < 6.0);
        }
    }

    #[test]
    fn nothing_before_the_boxes_are_known() {
        let zero = bx(0.0, 0.0, 0.0, 0.0);
        assert!(route(zero, zero, None, 585.0, 788.0, false).is_empty());
    }
}
