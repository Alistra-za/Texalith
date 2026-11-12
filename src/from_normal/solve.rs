//! The Poisson height solve from de-tilted slopes and the fit measure. Copied from Tessalith
//! `crates/engine/src/from_normal/solve.rs` at commit 36384560 (`detect_tiling` not yet), with one change: the
//! denominator uses the true symbol magnitude at Nyquist (spike 4, `spectrum_with`).
use super::fft::{fft2, C};
use super::Slopes;
use std::f64::consts::PI;

/// The half-texel difference symbol's magnitude, `2·sin(π·k̃/n)`: 2 at Nyquist.
fn magnitude(k: usize, n: usize) -> f64 {
    let kt = if 2 * k < n {
        k as f64
    } else {
        k as f64 - n as f64
    };
    2.0 * (PI * kt / n as f64).sin()
}

/// The half-texel difference symbol's imaginary part, `2·sin(π·k̃/n)`, 0 at Nyquist (keeps h real).
pub(super) fn symbol(k: usize, n: usize) -> f64 {
    if 2 * k == n {
        return 0.0;
    }
    magnitude(k, n)
}

/// Zero-mean height in texel units at `s`'s size from its de-tilted slopes: wrap solves the tile as periodic, mirror
/// solves its half-sample-symmetric 2W × 2H extension and crops. `relief` ∈ [0, 1] damps content coarser than
/// `8·relief` cycles across the width.
pub fn solve(s: &Slopes, wrap: bool, relief: f64) -> Vec<f64> {
    solve_with(s, wrap, relief, true)
}

/// [`solve`] with Tessalith's denominator (`nyquist_den` false: the symbol's 0 at Nyquist) or the fixed one.
pub(super) fn solve_with(s: &Slopes, wrap: bool, relief: f64, nyquist_den: bool) -> Vec<f64> {
    let (hh, n, m) = spectrum_with(s, wrap, relief, nyquist_den);
    let mut out = crop(hh, n, m, s);
    let mean = out.iter().sum::<f64>() / out.len().max(1) as f64;
    out.iter_mut().for_each(|v| *v -= mean);
    out
}

/// The inverse FFT of an `n × m` spectrum, cropped to `s`'s size (real parts).
fn crop(mut hh: Vec<C>, n: usize, m: usize, s: &Slopes) -> Vec<f64> {
    let (w, h) = (s.width as usize, s.height as usize);
    fft2(&mut hh, n, m, true);
    (0..h)
        .flat_map(|y| hh[y * n..y * n + w].iter().map(|c| c.re))
        .collect()
}

/// Gaussian band limit of the curvature, in texels at the solve size: fixed, there is no radius control.
pub const CURVATURE_SIGMA: f64 = 2.0;

/// The 0.5-centred curvature at `s`'s size: `K̂ = −den·Ĥ` from `solve`'s spectrum (same symbol and relief),
/// band-limited by `exp(−σ²(ωx² + ωy²)/2)`, `ω = 2π·k̃/N` on the FFT grid (the mirror's 2W × 2H included); stored as
/// `0.5 − 0.5·clamp(κ/Rκ, −1, 1)` with `Rκ = p99.5(|κ|)`, so peaks (convex) are bright and flat is 0.5 (`Rκ < 1e-12`:
/// all 0.5).
pub fn curvature(s: &Slopes, wrap: bool, relief: f64) -> Vec<f32> {
    curvature_with(s, wrap, relief, true)
}

/// The Curvature output's lines: [`curvature`] at relief 0 (the divergence of the slopes, band-limited), with `sigma`
/// in place of [`CURVATURE_SIGMA`] and the stored value `0.5 − 0.5·clamp(contrast·κ/Rκ, −1, 1)`.
pub fn curvature_lines(s: &Slopes, wrap: bool, sigma: f64, contrast: f64) -> Vec<f32> {
    curvature_core(s, wrap, 0.0, true, sigma, contrast)
}

/// [`curvature`] over [`solve_with`]'s spectrum.
pub(super) fn curvature_with(s: &Slopes, wrap: bool, relief: f64, nyquist_den: bool) -> Vec<f32> {
    curvature_core(s, wrap, relief, nyquist_den, CURVATURE_SIGMA, 1.0)
}

fn curvature_core(s: &Slopes, wrap: bool, relief: f64, nyquist_den: bool, sigma: f64, contrast: f64) -> Vec<f32> {
    let (mut hh, n, m) = spectrum_with(s, wrap, relief, nyquist_den);
    let axis = |len: usize| -> Vec<(f64, f64)> {
        (0..len)
            .map(|k| {
                let kt = if 2 * k <= len {
                    k as f64
                } else {
                    k as f64 - len as f64
                };
                (symbol(k, len), 2.0 * PI * kt / len as f64)
            })
            .collect()
    };
    let (ax, ay) = (axis(n), axis(m));
    let sig2 = sigma * sigma;
    for (l, &(dy, wy)) in ay.iter().enumerate() {
        for (k, &(dx, wx)) in ax.iter().enumerate() {
            let gain = -(dx * dx + dy * dy) * (-sig2 * (wx * wx + wy * wy) / 2.0).exp();
            hh[l * n + k] = hh[l * n + k].scale(gain);
        }
    }
    let kappa = crop(hh, n, m, s);
    let r = super::percentile(kappa.iter().map(|v| v.abs()).collect(), 0.995);
    // A NaN (a non-finite slope reaching the solve) reads as flat too.
    if r.is_nan() || r < 1e-12 {
        return vec![0.5; kappa.len()];
    }
    kappa
        .iter()
        .map(|&k| (0.5 - 0.5 * (contrast * k / r).clamp(-1.0, 1.0)) as f32)
        .collect()
}

/// The relief-damped height spectrum `Ĥ·den/(den + λ)` of `s`'s slopes on the FFT grid `n × m`: the tile when
/// wrapped, its half-sample-symmetric 2W × 2H extension otherwise.
///
/// The numerator keeps the symbol's 0 at Nyquist, so the height stays real. Tessalith's denominator `dx² + dy²` used
/// the same 0, so at x-Nyquist with a low y frequency it collapsed to `dy²` and any `q` there came out as a period-2
/// line scaled by `1/dy`. With `nyquist_den` the denominator uses the true magnitude, 2, at Nyquist instead.
fn spectrum_with(s: &Slopes, wrap: bool, relief: f64, nyquist_den: bool) -> (Vec<C>, usize, usize) {
    let (w, h) = (s.width as usize, s.height as usize);
    let (n, m) = if wrap { (w, h) } else { (2 * w, 2 * h) };
    let fold = |i: usize, len: usize| {
        if i < len {
            (i, 1.0)
        } else {
            (2 * len - 1 - i, -1.0)
        }
    };
    let mut g = vec![C::default(); n * m];
    for y in 0..m {
        let (yy, sy) = fold(y, h);
        for x in 0..n {
            let (xx, sx) = fold(x, w);
            let i = yy * w + xx;
            g[y * n + x] = C::new(sx * s.p[i], sy * s.q[i]);
        }
    }
    fft2(&mut g, n, m, false);
    let (dx, dy): (Vec<f64>, Vec<f64>) = (
        (0..n).map(|k| symbol(k, n)).collect(),
        (0..m).map(|l| symbol(l, m)).collect(),
    );
    let den_axis = |len: usize, d: &[f64]| -> Vec<f64> {
        match nyquist_den {
            true => (0..len).map(|k| magnitude(k, len).powi(2)).collect(),
            false => d.iter().map(|v| v * v).collect(),
        }
    };
    let (ex, ey) = (den_axis(n, &dx), den_axis(m, &dy));
    let lambda = 4.0 * (PI * 8.0 * relief / w as f64).sin().powi(2);
    let mut hh = vec![C::default(); n * m];
    for l in 0..m {
        for k in 0..n {
            let den = ex[k] + ey[l];
            if den == 0.0 {
                continue;
            }
            let (a, b) = (g[l * n + k], g[((m - l) % m) * n + (n - k) % n].conj());
            let ph = (a + b).scale(0.5);
            let d = a - b; // Q̂ = d / 2j
            let qh = C::new(d.im * 0.5, -d.re * 0.5);
            // conj(D)·X with D = j·d: −j·d·X; Ĥ·den/(den + λ) = num/(den + λ).
            let num = C::new(
                dx[k] * ph.im + dy[l] * qh.im,
                -(dx[k] * ph.re + dy[l] * qh.re),
            );
            hh[l * n + k] = num.scale(1.0 / (den + lambda));
        }
    }
    (hh, n, m)
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

/// How well `h` explains the de-tilted slopes: mean and p95 angle (degrees) between W8's central-difference normals
/// of `k·h` (least-squares `k`) and `unit(−p, −q, 1)`, over interior texels. `(0, 0)` for a flat `h` or no interior.
pub fn fit(s: &Slopes, h: &[f64]) -> (f32, f32) {
    let (w, hgt) = (s.width as usize, s.height as usize);
    let interior = || {
        (1..hgt.saturating_sub(1))
            .flat_map(move |y| (1..w.saturating_sub(1)).map(move |x| y * w + x))
    };
    let re = |i: usize| (h[i + 1] - h[i - 1], h[i + w] - h[i - w]);
    let (mut num, mut den) = (0.0, 0.0);
    for i in interior() {
        let (pr, qr) = re(i);
        num += s.p[i] * pr + s.q[i] * qr;
        den += pr * pr + qr * qr;
    }
    if den == 0.0 {
        return (0.0, 0.0);
    }
    let k = num / den;
    let mut ang: Vec<f64> = interior()
        .map(|i| {
            let (pr, qr) = re(i);
            let (a, b) = (unit([-k * pr, -k * qr, 1.0]), unit([-s.p[i], -s.q[i], 1.0]));
            let cross = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            cross[0]
                .hypot(cross[1])
                .hypot(cross[2])
                .atan2(dot)
                .to_degrees()
        })
        .collect();
    let mean = ang.iter().sum::<f64>() / ang.len() as f64;
    let i = ((ang.len() - 1) as f64 * 0.95).round() as usize;
    let p95 = *ang.select_nth_unstable_by(i, f64::total_cmp).1;
    (mean as f32, p95 as f32)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::from_normal::tilt_deg;
    use std::f64::consts::PI;

    /// Stand-ins for engine's `Rgba32F` and `GreenConvention`, so the tests copied from Tessalith read as there.
    #[derive(Debug, Clone)]
    pub(crate) struct Rgba32F {
        pub width: u32,
        pub height: u32,
        pub px: Vec<[f32; 4]>,
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub(crate) enum Green {
        Directx,
        Opengl,
    }
    pub(crate) use Green::{Directx, Opengl};

    pub(crate) fn slopes(n: &Rgba32F, g: Green) -> Slopes {
        crate::from_normal::slopes(&n.px, n.width, n.height, g == Opengl)
    }

    pub(crate) fn lcg(s: &mut u64) -> f64 {
        *s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (*s >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }

    pub(crate) fn gauss(x: f64, y: f64, cx: f64, cy: f64, sigma: f64) -> f64 {
        (-((x - cx).powi(2) + (y - cy).powi(2)) / (2.0 * sigma * sigma)).exp()
    }

    /// Periodic: sines of periods W, W/2, W/4, W/8 in x (H… in y) plus a wrapped Gaussian bump (σ 8).
    pub(crate) fn torus(w: usize, h: usize) -> Vec<f64> {
        let (wf, hf) = (w as f64, h as f64);
        let mut out = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                let (xf, yf) = (x as f64, y as f64);
                let mut v = 0.0;
                for d in [1.0, 2.0, 4.0, 8.0] {
                    v += (2.0 * PI * d * xf / wf).sin() + (2.0 * PI * d * yf / hf + d).cos();
                }
                for ox in [-wf, 0.0, wf] {
                    for oy in [-hf, 0.0, hf] {
                        v += 4.0 * gauss(xf + ox, yf + oy, wf / 3.0, hf / 3.0, 8.0);
                    }
                }
                out[y * w + x] = 0.25 * v; // max slope about 1.2 at k = 4: steep, but not near the z clamp
            }
        }
        out
    }

    /// `f(u) = cos(πu) − cos(3πu)`, `u = (x + 0.5)/W`: even at both edges, not periodic, mean slope 0.
    fn half_cosine(w: usize, h: usize) -> Vec<f64> {
        let f = |u: f64| (PI * u).cos() - (3.0 * PI * u).cos();
        let mut out = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                let (u, v) = ((x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64);
                out[y * w + x] = 6.0 * f(u) * f(v) + gauss(x as f64, y as f64, 40.0, 60.0, 8.0);
            }
        }
        out
    }

    /// Stored codes of `unit(−p, −q, 1)`.
    pub(crate) fn encode(p: &[f64], q: &[f64], w: usize, h: usize) -> Rgba32F {
        let px = p
            .iter()
            .zip(q)
            .map(|(&p, &q)| {
                let l = (p * p + q * q + 1.0).sqrt();
                [
                    (-p / l * 0.5 + 0.5) as f32,
                    (-q / l * 0.5 + 0.5) as f32,
                    (1.0 / l * 0.5 + 0.5) as f32,
                    1.0,
                ]
            })
            .collect();
        Rgba32F {
            width: w as u32,
            height: h as u32,
            px,
        }
    }

    /// W8's central difference `k·(h[x+1] − h[x−1])`, with wrapped or mirrored (edge-repeating) indices.
    pub(crate) fn diffs(
        hf: &[f64],
        w: usize,
        h: usize,
        k: f64,
        wrap: bool,
    ) -> (Vec<f64>, Vec<f64>) {
        let at = |i: isize, n: usize| -> usize {
            if wrap {
                i.rem_euclid(n as isize) as usize
            } else {
                i.clamp(0, n as isize - 1) as usize
            }
        };
        let (mut p, mut q) = (vec![0.0; w * h], vec![0.0; w * h]);
        for y in 0..h {
            for x in 0..w {
                let (xi, yi) = (x as isize, y as isize);
                p[y * w + x] = k * (hf[y * w + at(xi + 1, w)] - hf[y * w + at(xi - 1, w)]);
                q[y * w + x] = k * (hf[at(yi + 1, h) * w + x] - hf[at(yi - 1, h) * w + x]);
            }
        }
        (p, q)
    }

    pub(crate) fn bump(hf: &[f64], w: usize, h: usize, k: f64, wrap: bool) -> Rgba32F {
        let (p, q) = diffs(hf, w, h, k, wrap);
        encode(&p, &q, w, h)
    }

    pub(crate) fn range(v: &[f64]) -> f64 {
        v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
    }

    /// Max |a − s·b| over mean-free a and b with the least-squares scale s.
    fn scaled_err(a: &[f64], b: &[f64]) -> f64 {
        let (ma, mb) = (
            a.iter().sum::<f64>() / a.len() as f64,
            b.iter().sum::<f64>() / b.len() as f64,
        );
        let (mut ab, mut bb) = (0.0, 0.0);
        for (x, y) in a.iter().zip(b) {
            ab += (x - ma) * (y - mb);
            bb += (y - mb) * (y - mb);
        }
        let s = ab / bb;
        // A bump stays a bump: the solve's sign is pinned, not absorbed by the scale.
        assert!(s > 0.0, "height came out inverted (scale {s})");
        a.iter()
            .zip(b)
            .map(|(x, y)| ((x - ma) - s * (y - mb)).abs())
            .fold(0.0, f64::max)
    }

    fn max_abs_diff(a: &[f64], b: &[f64]) -> f64 {
        a.iter()
            .zip(b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f64::max)
    }

    pub(crate) fn constant(w: usize, h: usize, px: [f32; 4]) -> Rgba32F {
        Rgba32F {
            width: w as u32,
            height: h as u32,
            px: vec![px; w * h],
        }
    }

    /// Seam step ≤ 1.5 × the largest interior neighbour step, across columns and rows.
    fn assert_no_seam(hs: &[f64], w: usize, h: usize) {
        let (mut seam_x, mut int_x, mut seam_y, mut int_y) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for y in 0..h {
            seam_x = seam_x.max((hs[y * w] - hs[y * w + w - 1]).abs());
            for x in 0..w - 1 {
                int_x = int_x.max((hs[y * w + x] - hs[y * w + x + 1]).abs());
            }
        }
        for x in 0..w {
            seam_y = seam_y.max((hs[x] - hs[(h - 1) * w + x]).abs());
            for y in 0..h - 1 {
                int_y = int_y.max((hs[y * w + x] - hs[(y + 1) * w + x]).abs());
            }
        }
        assert!(
            seam_x <= 1.5 * int_x,
            "column seam {seam_x} vs interior {int_x}"
        );
        assert!(
            seam_y <= 1.5 * int_y,
            "row seam {seam_y} vs interior {int_y}"
        );
    }

    /// Three anisotropic diagonal blobs (sigma 16 x 4): green-sensitive, as their x and y slopes differ.
    pub(crate) fn blobs(w: usize, h: usize) -> Vec<f64> {
        let mut hf = vec![0.0; w * h];
        for (i, v) in hf.iter_mut().enumerate() {
            let (x, y) = ((i % w) as f64, (i / w) as f64);
            for (cx, cy, a) in [(40.0, 40.0, 5.0), (90.0, 70.0, -4.0), (50.0, 100.0, 3.0)] {
                let (u, t) = (
                    (x - cx + y - cy) / 2f64.sqrt(),
                    (x - cx - y + cy) / 2f64.sqrt(),
                );
                *v += a * (-(u * u) / (2.0 * 16.0 * 16.0) - t * t / (2.0 * 4.0 * 4.0)).exp();
            }
        }
        hf
    }

    /// `q = ε·(−1)^x·sin(2π·4y/H)`, `p = 0`: all its energy sits at x-Nyquist, y frequency 4.
    fn x_nyquist_line(w: usize, h: usize, eps: f64) -> Slopes {
        Slopes {
            width: w as u32,
            height: h as u32,
            p: vec![0.0; w * h],
            q: (0..w * h)
                .map(|i| {
                    let sign = if (i % w).is_multiple_of(2) { 1.0 } else { -1.0 };
                    eps * sign * (2.0 * PI * 4.0 * (i / w) as f64 / h as f64).sin()
                })
                .collect(),
            tilt: (0.0, 0.0),
        }
    }

    #[test]
    fn an_x_nyquist_line_in_q_is_not_amplified() {
        // Wrap: least squares with the true magnitude gives amplitude ε·dy/(4 + dy²); Tessalith's 0 gave ε/dy, about
        // 400x that at dy = 2·sin(4π/256).
        let (w, h, eps) = (256, 256, 0.01);
        let s = x_nyquist_line(w, h, eps);
        let amp = |v: &[f64]| v.iter().fold(0.0f64, |a, x| a.max(x.abs()));
        let dy = 2.0 * (PI * 4.0 / h as f64).sin();
        let (old, new) = (amp(&solve_with(&s, true, 0.0, false)), amp(&solve(&s, true, 0.0)));
        assert!(
            (old - eps / dy).abs() < 0.05 * eps / dy,
            "unfixed amplitude {old} vs ε/dy {}",
            eps / dy
        );
        let want = eps * dy / (4.0 + dy * dy);
        assert!(
            (new - want).abs() < 0.05 * want,
            "fixed amplitude {new} vs {want}"
        );
    }

    #[test]
    fn the_fix_changes_nothing_in_mirror_mode() {
        // The half-sample-even extension of q has no energy at the 2W grid's x-Nyquist, and p's is multiplied by the
        // symbol's 0, so that line's numerator is 0 whatever the denominator (the same for y).
        let s = x_nyquist_line(256, 256, 0.01);
        let n = bump(&torus(128, 64), 128, 64, 4.0, false);
        for s in [s, slopes(&n, Directx)] {
            let (old, new) = (solve_with(&s, false, 0.3, false), solve(&s, false, 0.3));
            let d = max_abs_diff(&old, &new);
            // Only FFT round-off reaches those bins: 1e-9 against heights of order 1.
            assert!(d < 1e-9, "mirror moved by {d} (range {})", range(&old));
        }
    }

    #[test]
    fn round_trip_fit_recovers_an_analytic_height() {
        let t = torus(256, 256);
        let s = slopes(&bump(&t, 256, 256, 4.0, true), Directx);
        let h = solve(&s, true, 0.0);
        let err = scaled_err(&t, &h);
        assert!(
            err < range(&t) / 255.0,
            "max error {err}, range {}",
            range(&t)
        );
        let (mean, p95) = fit(&s, &h);
        assert!(mean < 0.5, "fit {mean}° (p95 {p95}°)");
    }

    #[test]
    fn flat_in_gives_flat_out() {
        let s = slopes(&constant(64, 32, [0.5, 0.5, 1.0, 1.0]), Directx);
        for wrap in [true, false] {
            let h = solve(&s, wrap, 0.3);
            assert!(h.iter().all(|&v| v == 0.0), "wrap {wrap}");
            assert_eq!(fit(&s, &h), (0.0, 0.0));
        }
        assert_eq!(tilt_deg(&s), 0.0);
        let s = slopes(&constant(2, 2, [0.6, 0.4, 1.0, 1.0]), Directx);
        assert_eq!(fit(&s, &[0.0, 1.0, -1.0, 0.0]), (0.0, 0.0));
    }

    #[test]
    fn a_tilted_constant_normal_gives_a_flat_height() {
        let s = slopes(&constant(64, 64, [0.6, 0.45, 1.0, 1.0]), Directx);
        assert!(s.p.iter().chain(&s.q).all(|v| v.abs() < 1e-12));
        assert!(
            (s.tilt.0 + 0.205).abs() < 1e-3 && (s.tilt.1 - 0.103).abs() < 1e-3,
            "{:?}",
            s.tilt
        );
        for wrap in [true, false] {
            let h = solve(&s, wrap, 0.3);
            let m = h.iter().fold(0.0f64, |a, v| a.max(v.abs()));
            assert!(m < 1e-9, "wrap {wrap}: max |h| {m}");
            assert_eq!(fit(&s, &h), (0.0, 0.0), "wrap {wrap}");
        }
        assert!((tilt_deg(&s) - 12.9).abs() < 0.1, "{}", tilt_deg(&s));
    }

    #[test]
    fn a_tilted_relief_solves_like_the_untilted_one_in_both_modes() {
        let (w, h) = (128, 128);
        let (p, q) = diffs(&half_cosine(w, h), w, h, 1.0, false);
        let (pt, qt): (Vec<f64>, Vec<f64>) = (
            p.iter().map(|v| v + 0.2).collect(),
            q.iter().map(|v| v - 0.1).collect(),
        );
        let (flat, tilted) = (
            slopes(&encode(&p, &q, w, h), Directx),
            slopes(&encode(&pt, &qt, w, h), Directx),
        );
        for wrap in [true, false] {
            let (a, b) = (solve(&flat, wrap, 0.3), solve(&tilted, wrap, 0.3));
            let d = max_abs_diff(&a, &b);
            // 1e-6, not 1e-9: the two fixtures are stored as f32, whose rounding alone moves the slopes by ~1e-6 and
            // the heights by ~3e-8 × range. A tilt integrated into a ramp would be ~1 × range.
            assert!(
                d < 1e-6 * range(&a),
                "wrap {wrap}: {d} vs range {}",
                range(&a)
            );
            let (fa, fb) = (fit(&flat, &a).0, fit(&tilted, &b).0);
            assert!((fa - fb).abs() < 0.01, "wrap {wrap}: fit {fa} vs {fb}");
        }
    }

    #[test]
    fn nyquist_noise_stays_bounded() {
        let t = torus(256, 256);
        let clean = bump(&t, 256, 256, 4.0, true);
        let mut noisy = clean.clone();
        let mut seed = 11u64;
        for px in &mut noisy.px {
            px[0] += (0.02 * lcg(&mut seed)) as f32;
            px[1] += (0.02 * lcg(&mut seed)) as f32;
        }
        let hc = solve(&slopes(&clean, Directx), true, 0.0);
        let hn = solve(&slopes(&noisy, Directx), true, 0.0);
        let mut d: Vec<f64> = hc.iter().zip(&hn).map(|(a, b)| (a - b).abs()).collect();
        d.sort_by(f64::total_cmp);
        let p995 = d[(d.len() as f64 * 0.995) as usize];
        assert!(
            p995 < 0.02 * range(&hc),
            "p99.5 {p995} vs range {}",
            range(&hc)
        );
    }

    #[test]
    fn a_wrapped_solve_has_no_seam_and_a_mirrored_one_is_exact_at_the_edges() {
        let (w, h) = (256, 256);
        let hs = solve(
            &slopes(&bump(&torus(w, h), w, h, 4.0, true), Directx),
            true,
            0.0,
        );
        assert_no_seam(&hs, w, h);
        let ha = half_cosine(w, h);
        let hm = solve(&slopes(&bump(&ha, w, h, 4.0, false), Directx), false, 0.0);
        let err = scaled_err(&ha, &hm);
        assert!(
            err < range(&ha) / 255.0,
            "mirror error {err}, range {}",
            range(&ha)
        );
    }

    #[test]
    fn a_rectangular_normal_solves_without_a_seam() {
        let (w, h) = (256, 128);
        let t = torus(w, h);
        let s = slopes(&bump(&t, w, h, 4.0, true), Directx);
        let hs = solve(&s, true, 0.0);
        assert!(fit(&s, &hs).0 < 0.5, "fit {:?}", fit(&s, &hs));
        assert!(scaled_err(&t, &hs) < range(&t) / 255.0);
        assert_no_seam(&hs, w, h);
        let ha = half_cosine(w, h);
        let hm = solve(&slopes(&bump(&ha, w, h, 4.0, false), Directx), false, 0.0);
        let err = scaled_err(&ha, &hm);
        assert!(
            err < range(&ha) / 255.0,
            "mirror error {err}, range {}",
            range(&ha)
        );
    }

    #[test]
    fn a_non_finite_texel_does_not_poison_the_height() {
        let (w, h) = (64, 64);
        let mut n = bump(&torus(w, h), w, h, 4.0, true);
        n.px[100][0] = f32::NAN;
        n.px[200][1] = f32::INFINITY;
        let s = slopes(&n, Directx);
        assert!(s.p.iter().chain(&s.q).all(|v| v.is_finite()));
        for wrap in [true, false] {
            assert!(
                solve(&s, wrap, 0.3).iter().all(|v| v.is_finite()),
                "wrap {wrap}"
            );
        }
    }

    #[test]
    fn the_wrong_green_fits_much_worse() {
        let (w, h) = (128, 128);
        let n = bump(&blobs(w, h), w, h, 2.0, true);
        let fits: Vec<f32> = [Directx, Opengl]
            .into_iter()
            .map(|g| {
                let s = slopes(&n, g);
                fit(&s, &solve(&s, true, 0.0)).0
            })
            .collect();
        assert!(
            fits[0] < 0.5 * fits[1],
            "directx {}° vs opengl {}°",
            fits[0],
            fits[1]
        );
    }

    #[test]
    fn relief_balance_follows_its_analytic_gain() {
        let (w, h) = (256, 256);
        let periods = [256.0, 32.0, 4.0];
        // cos(2π(x + ½)/P) with P | 2W is periodic over W and even about both edges, so it is one exact bin in
        // either mode, and the mirror's 2W field keeps the same gain (λ is set by the original width).
        let basis = |x: usize, p: f64| (2.0 * PI * (x as f64 + 0.5) / p).cos();
        let hf: Vec<f64> = (0..w * h)
            .map(|i| periods.iter().map(|&p| basis(i % w, p)).sum::<f64>() * 0.5)
            .collect();
        let proj = |v: &[f64], p: f64| -> f64 {
            v.iter().enumerate().map(|(i, a)| a * basis(i % w, p)).sum()
        };
        let lambda = 4.0 * (8.0 * PI / 256.0).sin().powi(2);
        for wrap in [true, false] {
            let s = slopes(&bump(&hf, w, h, 1.0, wrap), Directx);
            let (h0, h1) = (solve(&s, wrap, 0.0), solve(&s, wrap, 1.0));
            for p in periods {
                let den = 4.0 * (PI * (256.0 / p) / 256.0).sin().powi(2);
                let (got, want) = (proj(&h1, p) / proj(&h0, p), den / (den + lambda));
                assert!(
                    (got / want - 1.0).abs() < 0.01,
                    "wrap {wrap}, period {p}: {got} vs {want}"
                );
            }
        }
    }

    /// Slopes of a height varying along x only (q = 0): `p = dh(x)`, `w`×`h`.
    fn x_slopes(w: usize, h: usize, dh: impl Fn(f64) -> f64) -> Slopes {
        let p: Vec<f64> = (0..w * h).map(|i| dh((i % w) as f64)).collect();
        Slopes {
            width: w as u32,
            height: h as u32,
            q: vec![0.0; w * h],
            p,
            tilt: (0.0, 0.0),
        }
    }

    #[test]
    fn curvature_lines_is_relief_0_curvature_with_its_own_sigma_and_contrast() {
        // h = sin(2πx/64): peak at x = 16, zero at 0; at x = 8 the stored value sits .35 from grey.
        let om = 2.0 * PI / 64.0;
        let s = x_slopes(256, 256, |x| om * (om * x).cos());
        let base = curvature(&s, true, 0.0);
        let same = curvature_lines(&s, true, CURVATURE_SIGMA, 1.0);
        let d = base.iter().zip(&same).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(d < 1e-6, "differs by {d}");
        let row = 100 * 256;
        assert!(same[row + 16] > 0.9, "peaks bright");
        let more = curvature_lines(&s, true, CURVATURE_SIGMA, 2.0);
        assert!((more[row + 8] - 0.5).abs() > (base[row + 8] - 0.5).abs() + 0.05);
        // Line thickness: a σ-1 ridge at x = 128 lights more columns under a σ-4 band limit than under σ 1.
        let s2 = x_slopes(256, 256, |x| -4.0 * (x - 128.0) * (-(x - 128.0).powi(2) / 2.0).exp());
        let lit = |c: &[f32]| c[row..row + 256].iter().filter(|&&v| v > 0.75).count();
        let (thin, thick) = (lit(&curvature_lines(&s2, true, 1.0, 1.0)), lit(&curvature_lines(&s2, true, 4.0, 1.0)));
        assert!(thick > thin, "σ 4 lit {thick} columns, σ 1 lit {thin}");
    }

    #[test]
    fn curvature_of_a_sine_is_bright_on_peaks_and_half_where_flat() {
        // h = sin(2πx/64) on 256²: peaks at x = 16 + 64k, zero crossings at 32k, troughs at 48 + 64k.
        let om = 2.0 * PI / 64.0;
        let s = x_slopes(256, 256, |x| om * (om * x).cos());
        let c = curvature(&s, true, 0.0);
        assert_eq!(c.len(), 256 * 256);
        for k in 0..4 {
            let at = |x: usize| c[100 * 256 + x + 64 * k];
            assert!(at(16) > 0.9, "peak {}", at(16));
            assert!((at(0) - 0.5).abs() < 0.02, "zero {}", at(0));
            assert!((at(32) - 0.5).abs() < 0.02, "zero {}", at(32));
            assert!(at(48) < 0.1, "trough {}", at(48));
        }
        // Mirror mode, the same sign: h = cos(2π(x + ½)/64) is even about both edges; peaks at x + ½ = 64k.
        let s = x_slopes(256, 256, |x| -om * (om * (x + 0.5)).sin());
        let c = curvature(&s, false, 0.0);
        let row = &c[100 * 256..101 * 256];
        assert!(row[63] > 0.9 && row[127] > 0.9, "{} {}", row[63], row[127]);
        assert!(row[95] < 0.1 && row[159] < 0.1, "{} {}", row[95], row[159]);
    }

    #[test]
    fn a_rectangular_curvature_keeps_its_axes() {
        // 256 wide, 128 high, a sine along x (wrap) and an edge-even cosine along y (mirror).
        let om = 2.0 * PI / 64.0;
        let s = x_slopes(256, 128, |x| om * (om * x).cos());
        let c = curvature(&s, true, 0.0);
        assert_eq!(c.len(), 256 * 128);
        let row = &c[100 * 256..101 * 256];
        assert!(row[16] > 0.9 && row[48] < 0.1, "{} {}", row[16], row[48]);
        let (w, h) = (128usize, 256usize);
        let s = Slopes {
            width: w as u32,
            height: h as u32,
            p: vec![0.0; w * h],
            q: (0..w * h)
                .map(|i| -om * (om * ((i / w) as f64 + 0.5)).sin())
                .collect(),
            tilt: (0.0, 0.0),
        };
        let c = curvature(&s, false, 0.0);
        let col = |y: usize| c[y * w + 40];
        assert!(col(63) > 0.9 && col(127) > 0.9, "{} {}", col(63), col(127));
        assert!(col(95) < 0.1 && col(159) < 0.1, "{} {}", col(95), col(159));
    }

    #[test]
    fn curvature_is_band_limited_by_a_sigma_2_gaussian() {
        // Periods 64 and 8, amplitudes chosen so their curvatures are equal before the Gaussian (κ amplitude
        // 2·A·ω·sin(ω/2) = 2 each). The output's period-8 share is then exp(−σ²ω8²/2) / exp(−σ²ω64²/2) ≈ 0.297 at
        // σ = 2 (0.74 at σ = 1, 0.07 at σ = 3).
        let (w, h) = (256, 8);
        let oms = [2.0 * PI / 64.0, 2.0 * PI / 8.0];
        let amp = |om: f64| 1.0 / (om * (om / 2.0).sin());
        let s = x_slopes(w, h, |x| {
            oms.iter().map(|&om| amp(om) * om * (om * x).cos()).sum()
        });
        let c = curvature(&s, true, 0.0);
        let proj = |om: f64| -> f64 {
            c.iter()
                .enumerate()
                .map(|(i, v)| (f64::from(*v) - 0.5) * (om * (i % w) as f64).sin())
                .sum()
        };
        let (lo, hi) = (proj(oms[0]), proj(oms[1]));
        assert!(
            lo > 0.0,
            "peaks bright: the sine's own component is positive ({lo})"
        );
        let want = (-4.0 * oms[1].powi(2) / 2.0).exp() / (-4.0 * oms[0].powi(2) / 2.0).exp();
        assert!((hi / lo - want).abs() < 0.03, "{} vs {want}", hi / lo);
    }

    #[test]
    fn flat_normal_gives_half_curvature() {
        for px in [[0.5, 0.5, 1.0, 1.0], [0.6, 0.45, 1.0, 1.0]] {
            let s = slopes(&constant(64, 64, px), Directx);
            for wrap in [true, false] {
                let c = curvature(&s, wrap, 0.3);
                assert!(c.iter().all(|&v| v == 0.5), "{px:?} wrap {wrap}");
            }
        }
    }

    #[test]
    fn curvature_follows_the_solves_relief() {
        // Relief damps the same coarse content as in the height: at relief 1 a one-cycle sine fades beside a
        // period-16 one (equal curvatures before the damping).
        let w = 256;
        let s = x_slopes(w, 8, |x| {
            [256.0f64, 16.0]
                .iter()
                .map(|&p| {
                    let om = 2.0 * PI / p;
                    (om * x).cos() / (om / 2.0).sin()
                })
                .sum()
        });
        let share = |relief: f64| {
            let c = curvature(&s, true, relief);
            let proj = |p: f64| -> f64 {
                c.iter()
                    .enumerate()
                    .map(|(i, v)| (f64::from(*v) - 0.5) * (2.0 * PI * (i % w) as f64 / p).sin())
                    .sum()
            };
            (proj(256.0) / proj(16.0)).abs()
        };
        assert!(
            share(1.0) < 0.5 * share(0.0),
            "{} vs {}",
            share(1.0),
            share(0.0)
        );
    }
}
