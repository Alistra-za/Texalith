//! The seam hint and the green guess. Copied from Tessalith at 36384560 (GPL-3.0):
//! `crates/engine/src/from_normal/solve.rs` (`detect_tiling`) and `mod.rs` (the green block of `generate`).
use super::master::Green;
use super::{fit, solve, Slopes};

/// Below this error ratio the green guess is unsure and stays DirectX (Tessalith's `GREEN_MARGIN`, 07 B3).
const GREEN_MARGIN: f64 = 1.25;

/// Mean squared `(p, q)` jump across the wrap seams over the mean squared interior neighbour jump. Tessalith reads
/// at most 2 as tiling. Resolution-dependent, so callers run it at one fixed size.
pub(super) fn seam_ratio(s: &Slopes) -> f64 {
    let (w, h) = (s.width as usize, s.height as usize);
    let jump = |i: usize, j: usize| (s.p[i] - s.p[j]).powi(2) + (s.q[i] - s.q[j]).powi(2);
    let (mut seam, mut ns, mut int, mut ni) = (0.0, 0usize, 0.0, 0usize);
    for y in 0..h {
        seam += jump(y * w + w - 1, y * w);
        ns += 1;
        for x in 0..w.saturating_sub(1) {
            int += jump(y * w + x, y * w + x + 1);
            ni += 1;
        }
    }
    for x in 0..w {
        seam += jump((h - 1) * w + x, x);
        ns += 1;
        for y in 0..h.saturating_sub(1) {
            int += jump(y * w + x, (y + 1) * w + x);
            ni += 1;
        }
    }
    let (seam, int) = (seam / ns.max(1) as f64, int / ni.max(1) as f64);
    match (int == 0.0, seam == 0.0) {
        (true, true) => 0.0,
        (true, false) => f64::INFINITY,
        _ => seam / int,
    }
}

/// The same slopes read the other way up (q negated).
pub(super) fn flipped(s: &Slopes) -> Slopes {
    Slopes { q: s.q.iter().map(|v| -v).collect(), tilt: (s.tilt.0, -s.tilt.1), ..s.clone() }
}

/// The green convention whose relief-0 solve explains the slopes better, and whether the margin made it sure.
/// `s` is read as DirectX.
pub(super) fn guess_green(s: &Slopes, wrap: bool) -> (Green, bool) {
    let err = |s: &Slopes| f64::from(fit(s, &solve(s, wrap, 0.0)).0);
    let (e_dx, e_gl) = (err(s), err(&flipped(s)));
    let (lo, hi) = (e_dx.min(e_gl), e_dx.max(e_gl));
    let margin = if hi == 0.0 { 1.0 } else { hi / lo };
    match margin >= GREEN_MARGIN {
        true if e_gl < e_dx => (Green::Opengl, true),
        true => (Green::Directx, true),
        false => (Green::Directx, false),
    }
}
