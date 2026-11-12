//! GTAO-style ambient occlusion marched over the solved height. Copied from Tessalith
//! `crates/engine/src/from_normal/ao.rs` at commit 36384560, with one change: the tangent plane comes from the
//! height's own slopes, not the input normal's (spike 4, [`march`]). Progress is per finished row
//! ([`march_with_progress`]).
use super::{AoOpts, Slopes};
use rayon::prelude::*;
use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

/// tan(4°): horizons this little above the tangent plane are noise on flats and are ignored.
pub(super) const BIAS: f64 = 0.0699;

/// The height being marched, with its tiling (wrap: indices mod the size; mirror: clamped to the edge).
struct Field<'a> {
    h: &'a [f64],
    w: usize,
    hgt: usize,
    wrap: bool,
}

impl Field<'_> {
    fn bilinear(&self, x: f64, y: f64) -> f64 {
        let axis = |v: f64, n: usize| {
            let i = v.floor();
            let t = v - i;
            let i = i as i64;
            let at = |j: i64| {
                if self.wrap {
                    j.rem_euclid(n as i64) as usize
                } else {
                    j.clamp(0, n as i64 - 1) as usize
                }
            };
            (at(i), at(i + 1), t)
        };
        let (x0, x1, tx) = axis(x, self.w);
        let (y0, y1, ty) = axis(y, self.hgt);
        let row = |y: usize| {
            let (a, b) = (self.h[y * self.w + x0], self.h[y * self.w + x1]);
            a + (b - a) * tx
        };
        let (top, bot) = (row(y0), row(y1));
        top + (bot - top) * ty
    }
}

/// Slice directions `(cos φ, sin φ)`, `φ = (k + ½)·π/n`.
fn directions(n: usize) -> Vec<(f64, f64)> {
    (0..n)
        .map(|k| {
            let phi = (k as f64 + 0.5) * PI / n as f64;
            (phi.cos(), phi.sin())
        })
        .collect()
}

/// Linear falloff over the outer 61.5 % of the radius (XeGTAO's range).
fn falloff(r: f64, big_r: f64) -> f64 {
    ((big_r - r) / (0.615 * big_r)).clamp(0.0, 1.0)
}

/// Cosine-weighted visibility at `(x, y)`, clamped to [0, 1]: the GTAO arc integral per slice, horizons searched
/// at `radii` (`(r, falloff)`) on both sides.
fn texel(
    f: &Field,
    s: &Slopes,
    x: usize,
    y: usize,
    dirs: &[(f64, f64)],
    radii: &[(f64, f64)],
    bias: f64,
) -> f64 {
    let i = y * f.w + x;
    let (p, q) = (s.p[i], s.q[i]);
    let l = (p * p + q * q + 1.0).sqrt();
    let (nx, ny, nz) = (-p / l, -q / l, 1.0 / l);
    let h0 = f.h[i];
    let (px, py) = (x as f64, y as f64);
    let mut vis = 0.0;
    for &(dx, dy) in dirs {
        let nd = nx * dx + ny * dy;
        let proj = nd.hypot(nz);
        let gamma = nd.atan2(nz);
        let (cg, sg) = (gamma.cos(), gamma.sin());
        let mut arc = 0.0;
        for side in [1.0, -1.0] {
            let tan_t = -side * nd / nz;
            let mut best = tan_t;
            for &(r, w) in radii {
                let e = (f.bilinear(px + side * r * dx, py + side * r * dy) - h0) / r;
                let c = tan_t + (e - tan_t) * w;
                if c > tan_t + bias {
                    best = best.max(c);
                }
            }
            // Horizon angle from zenith, signed toward side·d; `best` starts at the tangent, so no GTAO clamp.
            let theta = side * (FRAC_PI_2 - best.atan());
            arc += 0.25 * (-(2.0 * theta - gamma).cos() + cg + 2.0 * theta * sg);
        }
        vis += proj * arc;
    }
    (vis / dirs.len() as f64).clamp(0.0, 1.0)
}

/// AO in [0, 1] (1 = open) of the row-major `width × height` height `h` (texel units, depth already applied); `None`
/// when cancelled. The tangent plane at each texel comes from `h`'s own central differences, so it agrees with the
/// horizons marched over `h`. Tessalith used the input normal's slopes, and where the normal holds detail the height
/// can't, that speckled the AO (spike 4).
pub fn march(
    h: &[f64],
    width: u32,
    height: u32,
    wrap: bool,
    o: &AoOpts,
    cancel: &AtomicBool,
) -> Option<Vec<f32>> {
    march_with_progress(h, width, height, wrap, o, &AtomicU32::new(0), cancel)
}

/// [`march`], storing the finished share of rows in `progress` in per mille (1000 when done).
pub fn march_with_progress(
    h: &[f64],
    width: u32,
    height: u32,
    wrap: bool,
    o: &AoOpts,
    progress: &AtomicU32,
    cancel: &AtomicBool,
) -> Option<Vec<f32>> {
    let s = central(h, width as usize, height as usize, wrap);
    run(h, &s, wrap, o, progress, cancel, BIAS, 0.0)
}

/// Central differences over 2, wrapped or edge-repeating like the march's samples, as slopes with no tilt.
pub(super) fn central(h: &[f64], w: usize, hgt: usize, wrap: bool) -> Slopes {
    let at = |i: isize, n: usize| match wrap {
        true => i.rem_euclid(n as isize) as usize,
        false => i.clamp(0, n as isize - 1) as usize,
    };
    let (mut p, mut q) = (vec![0.0; w * hgt], vec![0.0; w * hgt]);
    for y in 0..hgt {
        for x in 0..w {
            let (xi, yi) = (x as isize, y as isize);
            p[y * w + x] = (h[y * w + at(xi + 1, w)] - h[y * w + at(xi - 1, w)]) / 2.0;
            q[y * w + x] = (h[at(yi + 1, hgt) * w + x] - h[at(yi - 1, hgt) * w + x]) / 2.0;
        }
    }
    Slopes {
        width: w as u32,
        height: hgt as u32,
        p,
        q,
        tilt: (0.0, 0.0),
    }
}

/// The march with the tangent plane from `s`, as Tessalith marched, its angle bias, and a floor on the march radii in
/// texels (the test seam).
#[cfg(test)]
pub(super) fn march_with(
    h: &[f64],
    s: &Slopes,
    wrap: bool,
    o: &AoOpts,
    bias: f64,
    floor: f64,
) -> Vec<f32> {
    run(h, s, wrap, o, &AtomicU32::new(0), &AtomicBool::new(false), bias, floor).unwrap()
}

#[allow(clippy::too_many_arguments)]
fn run(
    h: &[f64],
    s: &Slopes,
    wrap: bool,
    o: &AoOpts,
    progress: &AtomicU32,
    cancel: &AtomicBool,
    bias: f64,
    floor: f64,
) -> Option<Vec<f32>> {
    let (w, hgt) = (s.width as usize, s.height as usize);
    let big_r = f64::from(o.radius) * w as f64;
    if big_r < 0.5 {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        progress.store(1000, Ordering::Relaxed);
        return Some(vec![1.0; w * hgt]);
    }
    let done = AtomicUsize::new(0);
    let ns = (o.steps.max(1) as usize).min(big_r.ceil() as usize);
    let radii: Vec<(f64, f64)> = (1..=ns)
        .map(|i| {
            let r = (big_r * (i as f64 / ns as f64).powi(2)).max(floor);
            (r, falloff(r, big_r))
        })
        .collect();
    let dirs = directions(o.slices.max(1) as usize);
    let f = Field { h, w, hgt, wrap };
    let power = f64::from(o.intensity);
    let rows: Option<Vec<Vec<f32>>> = (0..hgt)
        .into_par_iter()
        .map(|y| {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let row: Vec<f32> = (0..w)
                .map(|x| texel(&f, s, x, y, &dirs, &radii, bias).powf(power) as f32)
                .collect();
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress.fetch_max((d * 1000 / hgt) as u32, Ordering::Relaxed);
            Some(row)
        })
        .collect();
    Some(rows?.concat())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::from_normal::solve::solve;
    use crate::from_normal::solve::tests::{
        blobs, bump, constant, gauss, lcg, slopes, torus, Directx, Green, Opengl, Rgba32F,
    };

    fn no() -> AtomicBool {
        AtomicBool::new(false)
    }

    fn ao_opts(radius: f32, slices: u32) -> AoOpts {
        AoOpts {
            radius,
            slices,
            ..AoOpts::default()
        }
    }

    /// The AO Tessalith's `generate` makes at the solve size, with Texalith's march: the march over
    /// `solve(relief 0.3) × depth`.
    fn gen_ao(n: &Rgba32F, wrap: bool, green: Green, ao: AoOpts) -> Vec<f32> {
        let s = slopes(n, green);
        let h: Vec<f64> = solve(&s, wrap, 0.3)
            .iter()
            .map(|v| v * f64::from(ao.depth))
            .collect();
        march(&h, n.width, n.height, wrap, &ao, &no()).unwrap()
    }

    #[test]
    fn a_normal_the_height_cannot_hold_does_not_speckle_the_ao() {
        // Noise in the stored normal barely moves the solved height (`nyquist_noise_stays_bounded`). Marched with
        // the height's own slopes the AO stays near the clean one; with the noisy normal's slopes, as Tessalith
        // marched, every texel's tangent plane carries the noise.
        let n = 256;
        let clean = bump(&torus(n, n), n, n, 4.0, true);
        let mut noisy = clean.clone();
        let mut seed = 11u64;
        for px in &mut noisy.px {
            px[0] += (0.02 * lcg(&mut seed)) as f32;
            px[1] += (0.02 * lcg(&mut seed)) as f32;
        }
        let o = AoOpts::default();
        let side = n as u32;
        let reference = march(&solve(&slopes(&clean, Directx), true, 0.3), side, side, true, &o, &no()).unwrap();
        let s = slopes(&noisy, Directx);
        let h = solve(&s, true, 0.3);
        let err = |a: &[f32]| {
            a.iter()
                .zip(&reference)
                .map(|(x, y)| f64::from((x - y).abs()))
                .sum::<f64>()
                / a.len() as f64
        };
        let own = err(&march(&h, side, side, true, &o, &no()).unwrap());
        let normal = err(&march_with(&h, &s, true, &o, BIAS, 0.0));
        assert!(own < 0.5 * normal, "own slopes {own} vs the normal's {normal}");
    }

    #[test]
    fn the_dark_share_counts_texels_below_one_half() {
        // A deep narrow pit: its floor is below 0.5, so the share is not 0 and a wrong comparison shows.
        let n = 128;
        let hf: Vec<f64> = (0..n * n)
            .map(|i| -9.0 * gauss((i % n) as f64, (i / n) as f64, 64.0, 64.0, 3.0))
            .collect();
        let g = gen_ao(&bump(&hf, n, n, 0.5, true), true, Directx, AoOpts::default());
        let dark = g.iter().filter(|&&v| v < 0.5).count();
        let quarter = g.iter().filter(|&&v| v < 0.25).count();
        assert!(dark > quarter, "below 0.5 {dark}, below 0.25 {quarter}");
    }

    /// Analytic height plus its central-difference slopes (`k = 0.5`: the slope, not twice it).
    fn analytic(hf: &[f64], w: usize, h: usize, wrap: bool) -> Slopes {
        slopes(&bump(hf, w, h, 0.5, wrap), Directx)
    }

    #[test]
    fn ao_on_a_flat_surface_is_one() {
        let h = vec![0.0; 64 * 64];
        for wrap in [true, false] {
            let ao = march(&h, 64, 64, wrap, &AoOpts::default(), &no()).unwrap();
            assert!(ao.iter().all(|&v| (v - 1.0).abs() < 1e-6), "wrap {wrap}");
        }
        // R < 0.5 texel: AO = 1 everywhere, without marching, even on relief.
        let t = torus(64, 64);
        let ao = march(&t, 64, 64, true, &ao_opts(0.0, 16), &no()).unwrap();
        assert!(ao.iter().all(|&v| v == 1.0));
    }

    #[test]
    fn a_tilted_constant_normal_gives_ao_one() {
        let n = constant(64, 64, [0.6, 0.45, 1.0, 1.0]);
        for wrap in [true, false] {
            let ao = gen_ao(&n, wrap, Directx, AoOpts::default());
            let min = ao.iter().copied().fold(1.0f32, f32::min);
            assert!(min >= 0.999, "wrap {wrap}: min {min}");
        }
    }

    #[test]
    fn a_pit_centre_matches_the_open_cone() {
        let n = 256;
        let (c, sigma, depth) = (128.0, 4.0, 6.0);
        let hf: Vec<f64> = (0..n * n)
            .map(|i| -depth * gauss((i % n) as f64, (i / n) as f64, c, c, sigma))
            .collect();
        let big_r = 64.0;
        let hr = |r: f64| -depth * (-(r * r) / (2.0 * sigma * sigma)).exp();
        let e_star = (1..=640_000)
            .map(|i| {
                let r = f64::from(i) * 1e-4;
                ((hr(r) - hr(0.0)) / r).atan()
            })
            .fold(f64::MIN, f64::max);
        let want = e_star.cos().powi(2);
        let o = ao_opts((big_r / n as f64) as f32, 16);
        let centre = 128 * n + 128;
        let s = analytic(&hf, n, n, true);
        let ao = march_with(&hf, &s, true, &o, 0.0, 0.0);
        assert!(
            (f64::from(ao[centre]) - want).abs() < 0.01,
            "analytic: {} vs cos²(e*) {want}",
            ao[centre]
        );
        let hs = solve(&s, true, 0.0);
        let ao = march_with(&hs, &central(&hs, n, n, true), true, &o, 0.0, 0.0);
        assert!(
            (f64::from(ao[centre]) - want).abs() < 0.02,
            "from the normal: {} vs cos²(e*) {want}",
            ao[centre]
        );
    }

    #[test]
    fn the_default_march_is_close_to_a_brute_force_reference() {
        let n = 256;
        let hf = torus(n, n);
        let s = analytic(&hf, n, n, true);
        let o = AoOpts::default();
        let fast = march(&hf, n as u32, n as u32, true, &o, &no()).unwrap();
        // Reference: 128 slices, every texel out to R (linear, 1-texel steps), same falloff and bias.
        let big_r = f64::from(o.radius) * n as f64;
        let dirs = directions(128);
        let radii: Vec<(f64, f64)> = (1..=big_r.ceil() as usize)
            .map(|i| (i as f64, falloff(i as f64, big_r)))
            .collect();
        let field = Field {
            h: &hf,
            w: n,
            hgt: n,
            wrap: true,
        };
        let mut d: Vec<f64> = (0..n * n)
            .map(|i| {
                let r = texel(&field, &s, i % n, i / n, &dirs, &radii, BIAS);
                (r - f64::from(fast[i])).abs()
            })
            .collect();
        let mean = d.iter().sum::<f64>() / d.len() as f64;
        d.sort_by(f64::total_cmp);
        let p99 = d[(0.99 * (d.len() - 1) as f64).round() as usize];
        assert!(mean < 0.02 && p99 < 0.06, "mean {mean}, p99 {p99}");
    }

    #[test]
    fn more_depth_lowers_ao() {
        // Pinned: a flat texel gets darker, the mean falls, and no texel rises by more than 0.02. (Tessalith kept the
        // source normal fixed while the height doubled, so some sides opened; the height's own slopes double with it.)
        let n = bump(&torus(128, 128), 128, 128, 4.0, true);
        let run = |depth: f32| {
            gen_ao(
                &n,
                true,
                Directx,
                AoOpts {
                    depth,
                    ..AoOpts::default()
                },
            )
        };
        let (a1, a2) = (run(1.0), run(2.0));
        let rise = a1
            .iter()
            .zip(&a2)
            .map(|(x, y)| y - x)
            .fold(f32::MIN, f32::max);
        assert!(rise <= 0.02, "max rise {rise}");
        let mean = |g: &[f32]| g.iter().map(|&v| f64::from(v)).sum::<f64>() / g.len() as f64;
        assert!(
            mean(&a2) < mean(&a1) - 0.02,
            "{} vs {}",
            mean(&a2),
            mean(&a1)
        );
        // A flat normal (the pit centre) has tan_t = 0 in every direction: deeper is strictly darker there.
        let n = 128;
        let hf: Vec<f64> = (0..n * n)
            .map(|i| -6.0 * gauss((i % n) as f64, (i / n) as f64, 64.0, 64.0, 4.0))
            .collect();
        let h2: Vec<f64> = hf.iter().map(|v| v * 2.0).collect();
        let o = AoOpts::default();
        let side = n as u32;
        let (b1, b2) = (
            march(&hf, side, side, true, &o, &no()).unwrap(),
            march(&h2, side, side, true, &o, &no()).unwrap(),
        );
        let c = 64 * n + 64;
        assert!(
            b2[c] < b1[c],
            "pit centre: depth 2 {} vs depth 1 {}",
            b2[c],
            b1[c]
        );
    }

    #[test]
    fn a_wrapped_ao_tiles() {
        let (w, h) = (256, 256);
        let g = gen_ao(&bump(&torus(w, h), w, h, 4.0, true), true, Directx, AoOpts::default());
        let at = |x: usize, y: usize| f64::from(g[y * w + x]);
        let (mut seam_x, mut int_x, mut seam_y, mut int_y) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for y in 0..h {
            seam_x = seam_x.max((at(0, y) - at(w - 1, y)).abs());
            for x in 0..w - 1 {
                int_x = int_x.max((at(x, y) - at(x + 1, y)).abs());
            }
        }
        for x in 0..w {
            seam_y = seam_y.max((at(x, 0) - at(x, h - 1)).abs());
            for y in 0..h - 1 {
                int_y = int_y.max((at(x, y) - at(x, y + 1)).abs());
            }
        }
        assert!(seam_x <= 1.5 * int_x, "x seam {seam_x} vs interior {int_x}");
        assert!(seam_y <= 1.5 * int_y, "y seam {seam_y} vs interior {int_y}");
    }

    #[test]
    fn the_wrong_green_changes_ao_strongly() {
        let n = bump(&blobs(128, 128), 128, 128, 2.0, true);
        let run = |g, slices| gen_ao(&n, true, g, ao_opts(0.0625, slices));
        let mean_diff = |a: &[f32], b: &[f32]| {
            a.iter()
                .zip(b)
                .map(|(x, y)| f64::from((x - y).abs()))
                .sum::<f64>()
                / a.len() as f64
        };
        let (right, right32, wrong) = (run(Directx, 16), run(Directx, 32), run(Opengl, 16));
        let (dw, ds) = (mean_diff(&right, &wrong), mean_diff(&right, &right32));
        assert!(dw > 4.0 * ds, "wrong green {dw} vs slices {ds}");
    }

    #[test]
    fn the_bias_ignores_tan_4_degree_horizons_only() {
        // Ridges along y, period 64, R = 16: amplitude 1 peaks about tan 2.4° above the tangent, amplitude 4 about
        // tan 7°. BIAS = 0 darkens the first; BIAS = 0.2 opens the second.
        let n = 256;
        let min_ao = |a: f64, bias: Option<f64>| {
            let hf: Vec<f64> = (0..n * n)
                .map(|i| a * (2.0 * PI * (i % n) as f64 / 64.0).sin())
                .collect();
            let o = AoOpts::default();
            match bias {
                Some(b) => march_with(&hf, &central(&hf, n, n, true), true, &o, b, 0.0),
                None => march(&hf, n as u32, n as u32, true, &o, &no()).unwrap(),
            }
            .into_iter()
            .fold(1.0f32, f32::min)
        };
        assert_eq!(min_ao(1.0, None), 1.0, "a gentle ridge is fully open");
        assert!(
            min_ao(1.0, Some(0.0)) < 1.0 - 1e-4,
            "bias 0 sees its horizons"
        );
        assert!(min_ao(4.0, None) < 0.999, "a 7° horizon is not ignored");
    }

    #[test]
    fn mirror_samples_clamp_to_the_edge() {
        // A tall bump 4 texels from the right edge: wrapped it shadows column 0; clamped it is out of reach.
        let n = 128;
        let hf: Vec<f64> = (0..n * n)
            .map(|i| 30.0 * gauss((i % n) as f64, (i / n) as f64, 124.0, 64.0, 3.0))
            .collect();
        let o = AoOpts::default();
        let col0 = |wrap: bool| {
            let ao = march(&hf, n as u32, n as u32, wrap, &o, &no()).unwrap();
            (0..n).map(|y| ao[y * n]).fold(1.0f32, f32::min)
        };
        assert!(col0(false) >= 0.999, "mirror: column 0 {}", col0(false));
        assert!(col0(true) < 0.99, "wrap: column 0 {}", col0(true));
    }

    #[test]
    fn cancel_stops_the_march() {
        let t = torus(64, 64);
        assert!(march(&t, 64, 64, true, &AoOpts::default(), &AtomicBool::new(true)).is_none());
    }

    #[test]
    fn an_open_ramp_is_not_darkened_like_dot_up() {
        // Ridges along y with 45° faces, period 64: valley at x ≡ 0, peak at x ≡ 32 (mod 64).
        let n = 256;
        let tri = |x: usize| {
            let m = (x % 64) as f64;
            if m <= 32.0 {
                m
            } else {
                64.0 - m
            }
        };
        let hf: Vec<f64> = (0..n * n).map(|i| tri(i % n)).collect();
        let g = gen_ao(&bump(&hf, n, n, 0.5, true), true, Directx, AoOpts::default());
        let row = 100 * n;
        let (face, valley) = (g[row + 64 + 16], g[row + 64]);
        assert!(face > 0.9, "mid-face {face}");
        assert!(valley < face - 0.1, "valley {valley} vs mid-face {face}");
    }
}
