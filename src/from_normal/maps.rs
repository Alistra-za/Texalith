//! The From normal outputs made from the master: Height data, the Parallax map, Normal from height, AO, Cavity and
//! Curvature.
use super::ao::march_with_progress;
use super::master::{Green, Master};
use super::solve::curvature_lines;
use super::{dims, percentile, AoOpts};
use engine::image::Rgba32F;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU32};

/// One grey value per texel, row-major, 0 to 1.
#[derive(Debug, Clone, PartialEq)]
pub struct Gray {
    pub width: u32,
    pub height: u32,
    pub px: Vec<f32>,
}

/// Height data: the true minimum black and the true maximum white, plus `max − min` in texels. A flat master is all
/// 0.5 with span 0.
pub fn height_data(m: &Master) -> (Gray, f64) {
    let lo = m.h.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = m.h.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = hi - lo;
    let (px, span) = match span.is_finite() && span >= 1e-12 {
        true => (m.h.iter().map(|v| ((v - lo) / span) as f32).collect(), span),
        false => (vec![0.5; m.h.len()], 0.0),
    };
    (Gray { width: m.width, height: m.height, px }, span)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlatLevel {
    /// 0.5 is the untouched surface, as ordinary parallax shaders and True PBR read it.
    #[default]
    MidGrey,
    /// 1.0 is the untouched surface and everything sinks (a multilayer inner layer).
    White,
}

/// Values stay this far from black and white (fixed, report: "Safety margin (fixed at 8/255)").
pub const MARGIN: f32 = 8.0 / 255.0;

#[derive(Debug, Clone, PartialEq)]
pub struct ParallaxOpts {
    /// Share of the range the p99.5 relief fills, 0.01 to 1.0.
    pub depth: f32,
    pub flat: FlatLevel,
    /// Smooth sharp steps: Gaussian σ in texels, 0 to 8.
    pub smooth_texels: f32,
    pub invert: bool,
}

impl Default for ParallaxOpts {
    fn default() -> Self {
        Self { depth: 0.9, flat: FlatLevel::MidGrey, smooth_texels: 0.0, invert: false }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParallaxReport {
    /// How much deeper than the normal implies, at the preview's depth scale (full range = 0.1 UV).
    pub times_normal: f32,
    /// The black-to-white span as % of the texture width: the shader depth that matches the normal.
    pub shader_depth_pct: f32,
    /// Distinct 8-bit levels, as BC4 will keep them.
    pub levels_used: u32,
    /// Share of texels the knee or the flat level's clamp changed.
    pub clipped_share: f32,
}

/// The preview's gain for a texel of height: its full range spans 0.1 UV at HeightScale 1.
fn unit_gain(width: u32) -> f64 {
    10.0 / f64::from(width.max(1))
}

/// The identity up to `a`, then a tanh shoulder that approaches `l` and never passes it.
fn knee(x: f64, a: f64, l: f64) -> f64 {
    if x <= a || l <= a {
        x.min(l)
    } else {
        a + (l - a) * ((x - a) / (l - a)).tanh()
    }
}

/// A separable Gaussian of σ `sigma` texels, wrapped or with clamped edges.
fn blur(h: &[f64], w: usize, hgt: usize, sigma: f64, wrap: bool) -> Vec<f64> {
    let r = (3.0 * sigma).ceil() as isize;
    let k: Vec<f64> = (-r..=r).map(|i| (-(i * i) as f64 / (2.0 * sigma * sigma)).exp()).collect();
    let sum: f64 = k.iter().sum();
    let at = |i: isize, n: usize| match wrap {
        true => i.rem_euclid(n as isize) as usize,
        false => i.clamp(0, n as isize - 1) as usize,
    };
    let mut tmp = vec![0.0; h.len()];
    for y in 0..hgt {
        for x in 0..w {
            tmp[y * w + x] = (-r..=r).map(|i| k[(i + r) as usize] * h[y * w + at(x as isize + i, w)]).sum::<f64>() / sum;
        }
    }
    let mut out = vec![0.0; h.len()];
    for y in 0..hgt {
        for x in 0..w {
            out[y * w + x] = (-r..=r).map(|i| k[(i + r) as usize] * tmp[at(y as isize + i, hgt) * w + x]).sum::<f64>() / sum;
        }
    }
    out
}

/// p99.5 of `|h − median|`, the relief scale Depth is measured against; with the median.
fn relief(h: &[f64]) -> (f64, f64) {
    let med = percentile(h.to_vec(), 0.5);
    (med, percentile(h.iter().map(|v| (v - med).abs()).collect(), 0.995))
}

/// The Parallax map from the master: median pivot (Mid-grey) or p99.5 top (White), gain from Depth, a soft knee on
/// the outliers and the fixed margin (report: "Median pivot, outlier knee, 8/255 margin").
pub fn parallax(m: &Master, o: &ParallaxOpts) -> (Gray, ParallaxReport) {
    let h = match o.smooth_texels > 0.0 {
        true => blur(&m.h, m.width as usize, m.height as usize, f64::from(o.smooth_texels), m.wrap),
        false => m.h.clone(),
    };
    let (med, r) = relief(&h);
    let gray = |px| Gray { width: m.width, height: m.height, px };
    if r.is_nan() || r < 1e-12 {
        let flat = match (o.flat, o.invert) {
            (FlatLevel::MidGrey, _) => 0.5,
            (FlatLevel::White, false) => 1.0,
            (FlatLevel::White, true) => 0.0,
        };
        return (gray(vec![flat; h.len()]), ParallaxReport { levels_used: 1, ..Default::default() });
    }
    let depth = f64::from(o.depth.clamp(0.01, 1.0));
    let g = depth * 0.5 / r;
    let margin = f64::from(MARGIN);
    let top = percentile(h.clone(), 0.995);
    let mut clipped = 0usize;
    let px: Vec<f32> = h
        .iter()
        .map(|&v| {
            let p = match o.flat {
                FlatLevel::MidGrey => {
                    let x = g * (v - med);
                    let k = knee(x.abs(), 0.5 * depth, 0.5 - margin);
                    clipped += usize::from(k != x.abs());
                    0.5 + k.copysign(x)
                }
                FlatLevel::White => {
                    let raw = g * (top - v);
                    let y = raw.max(0.0);
                    let k = knee(y, depth, 1.0 - margin);
                    clipped += usize::from(k != raw);
                    1.0 - k
                }
            };
            let p = if o.invert { 1.0 - p } else { p };
            p as f32
        })
        .collect();
    let mut seen = [false; 256];
    px.iter().for_each(|&p| seen[(p.clamp(0.0, 1.0) * 255.0).round() as usize] = true);
    let unit = unit_gain(m.width);
    let report = ParallaxReport {
        times_normal: (g / unit) as f32,
        shader_depth_pct: (100.0 / (g * f64::from(m.width))) as f32,
        levels_used: seen.iter().filter(|&&s| s).count() as u32,
        clipped_share: clipped as f32 / px.len().max(1) as f32,
    };
    (gray(px), report)
}

/// The Depth at which the Parallax map is exactly as deep as the normal implies, clamped to 0.01 to 1.0.
pub fn match_depth(m: &Master) -> f32 {
    let (_, r) = relief(&m.h);
    if r.is_nan() || r < 1e-12 {
        return 0.9;
    }
    ((unit_gain(m.width) * r / 0.5) as f32).clamp(0.01, 1.0)
}

/// A normal from the master's height: Scharr 3-10-3 (wrapped, or clamped edges), `unit(−p, −q, 1)` stored as
/// `n·0.5 + 0.5`, green flipped for OpenGL, alpha 1.
pub fn normal_from_height(m: &Master, green: Green) -> Vec<[f32; 4]> {
    let (w, hgt) = (m.width as isize, m.height as isize);
    let at = |i: isize, n: isize| match m.wrap {
        true => i.rem_euclid(n),
        false => i.clamp(0, n - 1),
    };
    let h = |x: isize, y: isize| m.h[(at(y, hgt) * w + at(x, w)) as usize];
    let gy = if green == Green::Opengl { -1.0 } else { 1.0 };
    (0..hgt)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| {
            let dx = |dy| h(x + 1, y + dy) - h(x - 1, y + dy);
            let dy = |dx| h(x + dx, y + 1) - h(x + dx, y - 1);
            let p = (3.0 * dx(-1) + 10.0 * dx(0) + 3.0 * dx(1)) / 32.0;
            let q = (3.0 * dy(-1) + 10.0 * dy(0) + 3.0 * dy(1)) / 32.0;
            let l = (p * p + q * q + 1.0).sqrt();
            [(-p / l * 0.5 + 0.5) as f32, (-gy * q / l * 0.5 + 0.5) as f32, (1.0 / l * 0.5 + 0.5) as f32, 1.0]
        })
        .collect()
}

/// The AO output's controls. Ranges: reach 0.015625..=0.25 (of the width), darkness 0.5..=3, surface depth 0.25..=4.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AoSettings {
    pub reach: f32,
    pub darkness: f32,
    pub surface_depth: f32,
}

impl Default for AoSettings {
    fn default() -> Self {
        Self { reach: 0.0625, darkness: 1.0, surface_depth: 1.0 }
    }
}

/// The master's height times `depth` at `long` on its longer side: resampled, and scaled by the size ratio so it
/// stays in texels of the new size, when `long` is smaller than the master.
fn height_at(m: &Master, long: u32, depth: f32) -> (u32, u32, Vec<f64>) {
    let d = f64::from(depth);
    if long >= m.width.max(m.height) {
        return (m.width, m.height, m.h.iter().map(|v| v * d).collect());
    }
    let (w, h) = dims((m.width, m.height), long);
    let img = Rgba32F {
        width: m.width,
        height: m.height,
        px: m.h.iter().map(|&v| [v as f32, 0.0, 0.0, 1.0]).collect(),
    };
    let k = d * f64::from(w) / f64::from(m.width);
    let px = crate::resample::resample(&img, (w, h)).px.iter().map(|p| f64::from(p[0]) * k).collect();
    (w, h, px)
}

/// AO at `long` on the longer side (the 1K preview while a slider moves), 8 slices and 16 steps; `None` when
/// cancelled.
pub fn ao(m: &Master, o: &AoSettings, long: u32, progress: &AtomicU32, cancel: &AtomicBool) -> Option<Gray> {
    let (width, height, h) = height_at(m, long, o.surface_depth);
    let opts = AoOpts { radius: o.reach, slices: 8, steps: 16, depth: o.surface_depth, intensity: o.darkness };
    let px = march_with_progress(&h, width, height, m.wrap, &opts, progress, cancel)?;
    Some(Gray { width, height, px })
}

/// AO made at strength 1, raised to `darkness`: what the march does with its own power, to within one f32 step.
pub fn darken(mut g: Gray, darkness: f32) -> Gray {
    if darkness != 1.0 {
        g.px.par_iter_mut().for_each(|v| *v = v.powf(darkness));
    }
    g
}

/// The default crevice width in texels: `width / 128`, within 2..=16.
pub fn default_crevice(width: u32) -> f32 {
    (width as f32 / 128.0).clamp(2.0, 16.0)
}

/// Cavity: AO with a radius of `crevice_texels` (2..=16), 8 slices and `min(round(crevice_texels), 8)` steps, at the
/// master's size.
pub fn cavity(
    m: &Master,
    crevice_texels: f32,
    surface_depth: f32,
    progress: &AtomicU32,
    cancel: &AtomicBool,
) -> Option<Gray> {
    let (width, height, h) = height_at(m, m.width.max(m.height), surface_depth);
    let opts = AoOpts {
        radius: crevice_texels / width as f32,
        slices: 8,
        steps: (crevice_texels.round() as u32).clamp(1, 8),
        depth: surface_depth,
        intensity: 1.0,
    };
    let px = march_with_progress(&h, width, height, m.wrap, &opts, progress, cancel)?;
    Some(Gray { width, height, px })
}

/// The Curvature output's controls. Ranges: line width 1..=8 texels (the Gaussian σ), contrast 0.5..=2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurvatureOpts {
    pub line_texels: f32,
    pub contrast: f32,
}

impl Default for CurvatureOpts {
    fn default() -> Self {
        Self { line_texels: 2.0, contrast: 1.0 }
    }
}

/// Curvature lines from the master's slopes: convex bright, concave dark, flat 0.5.
pub fn curvature(m: &Master, o: &CurvatureOpts) -> Gray {
    let px = curvature_lines(&m.slopes, m.wrap, f64::from(o.line_texels), f64::from(o.contrast));
    Gray { width: m.width, height: m.height, px }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use crate::from_normal::ao::central;
    use crate::from_normal::master::{Master, MasterReport};

    /// A wrapped master of height `h` (texels), slopes from its central differences.
    pub(crate) fn master_of(h: &[f64], w: u32, hgt: u32) -> Master {
        Master {
            width: w,
            height: hgt,
            wrap: true,
            h: h.to_vec(),
            slopes: central(h, w as usize, hgt as usize, true),
            from_normal: false,
            report: MasterReport::default(),
        }
    }

    /// A ramp from −5 to 5 texels in x, and every 500th texel 250 texels high.
    fn spiky_ramp(n: usize) -> Vec<f64> {
        (0..n * n)
            .map(|i| if i % 500 == 0 { 250.0 } else { 10.0 * (i % n) as f64 / n as f64 - 5.0 })
            .collect()
    }

    /// `amp·sin(2πx/64)` in texels.
    pub(crate) fn sine(n: usize, amp: f64) -> Vec<f64> {
        (0..n * n).map(|i| amp * (2.0 * std::f64::consts::PI * (i % n) as f64 / 64.0).sin()).collect()
    }

    fn median_f32(v: &[f32]) -> f32 {
        let mut s = v.to_vec();
        s.sort_by(f32::total_cmp);
        s[s.len() / 2]
    }

    /// Scharr's x derivative of a wrapped height, the test's own copy.
    fn scharr_p(m: &Master) -> Vec<f64> {
        let (w, h) = (m.width as isize, m.height as isize);
        let at = |x: isize, y: isize| m.h[(y.rem_euclid(h) * w + x.rem_euclid(w)) as usize];
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (y, x)))
            .map(|(y, x)| {
                let d = |dy| at(x + 1, y + dy) - at(x - 1, y + dy);
                (3.0 * d(-1) + 10.0 * d(0) + 3.0 * d(1)) / 32.0
            })
            .collect()
    }

    fn no() -> AtomicBool {
        AtomicBool::new(false)
    }

    fn p0() -> AtomicU32 {
        AtomicU32::new(0)
    }

    /// Pits 6 texels deep, σ 3, every 16 texels.
    fn pits(n: usize) -> Vec<f64> {
        (0..n * n)
            .map(|i| {
                let (x, y) = ((i % n) as f64 % 16.0 - 8.0, (i / n) as f64 % 16.0 - 8.0);
                -6.0 * (-(x * x + y * y) / 18.0).exp()
            })
            .collect()
    }

    /// A flat-floored groove along y: `depth` texels down where |x − n/2| < width/2.
    fn groove(n: usize, width: usize, depth: f64) -> Vec<f64> {
        (0..n * n).map(|i| if ((i % n) as isize - (n / 2) as isize).unsigned_abs() < width / 2 { -depth } else { 0.0 }).collect()
    }

    fn box_down(g: &Gray) -> Vec<f32> {
        let (w, h) = (g.width as usize, g.height as usize);
        (0..h / 2)
            .flat_map(|y| (0..w / 2).map(move |x| (x, y)))
            .map(|(x, y)| {
                let at = |dx, dy| g.px[(2 * y + dy) * w + 2 * x + dx];
                (at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1)) / 4.0
            })
            .collect()
    }

    #[test]
    fn ao_reports_progress_to_1000_and_stops_on_cancel() {
        let m = master_of(&pits(128), 128, 128);
        let p = p0();
        let g = ao(&m, &AoSettings::default(), 128, &p, &no()).unwrap();
        assert_eq!(p.load(Ordering::Relaxed), 1000);
        assert!(g.px.iter().any(|&v| v < 0.9), "pits darken");
        assert!(ao(&m, &AoSettings::default(), 128, &p0(), &AtomicBool::new(true)).is_none());
    }

    #[test]
    fn a_small_ao_matches_the_full_one_downsampled() {
        let m = master_of(&pits(256), 256, 256);
        let full = ao(&m, &AoSettings::default(), 256, &p0(), &no()).unwrap();
        let small = ao(&m, &AoSettings::default(), 128, &p0(), &no()).unwrap();
        assert_eq!((small.width, small.height), (128, 128));
        let mae = box_down(&full).iter().zip(&small.px).map(|(a, b)| (a - b).abs()).sum::<f32>() / small.px.len() as f32;
        assert!(mae < 0.03, "MAE {mae}"); // measured 0.0091
    }

    #[test]
    fn flat_gives_white_ao_and_cavity_and_grey_curvature() {
        let m = master_of(&[0.0; 64 * 64], 64, 64);
        assert!(ao(&m, &AoSettings::default(), 64, &p0(), &no()).unwrap().px.iter().all(|&v| v == 1.0));
        assert!(cavity(&m, 2.0, 1.0, &p0(), &no()).unwrap().px.iter().all(|&v| v == 1.0));
        assert!(curvature(&m, &CurvatureOpts::default()).px.iter().all(|&v| v == 0.5));
    }

    #[test]
    fn cavity_darkens_a_flat_floored_groove_that_curvature_leaves_grey() {
        // Floor centre 8 texels from each wall: inside the full-weight part of the 16-texel cavity radius (the falloff
        // takes the outer 61.5 %, so a wall 12 away left it at .97), past 3σ of curvature's σ 2.
        let m = master_of(&groove(128, 16, 12.0), 128, 128);
        let c = cavity(&m, 16.0, 1.0, &p0(), &no()).unwrap();
        let k = curvature(&m, &CurvatureOpts::default());
        let floor = 64 * 128 + 64;
        assert!(c.px[floor] < 0.9, "cavity {}", c.px[floor]); // measured 0.838
        assert!((k.px[floor] - 0.5).abs() < 0.05, "curvature {}", k.px[floor]); // measured 0.491
        assert_eq!(default_crevice(128), 2.0);
        assert_eq!(default_crevice(1024), 8.0);
        assert_eq!(default_crevice(4096), 16.0);
    }

    #[test]
    fn height_data_spans_black_to_white_and_reports_texels() {
        let (g, span) = height_data(&master_of(&[-2.0, 0.0, 1.0, 3.0], 2, 2));
        assert_eq!(g.px, vec![0.0, 0.4, 0.6, 1.0]);
        assert_eq!(span, 5.0);
    }

    #[test]
    fn a_flat_master_gives_flat_maps() {
        let m = master_of(&[0.0; 16], 4, 4);
        let (g, span) = height_data(&m);
        assert!(g.px.iter().all(|&v| v == 0.5) && span == 0.0);
        let (p, r) = parallax(&m, &ParallaxOpts::default());
        assert!(p.px.iter().all(|&v| v == 0.5) && r.levels_used == 1);
        let (w, _) = parallax(&m, &ParallaxOpts { flat: FlatLevel::White, ..Default::default() });
        assert!(w.px.iter().all(|&v| v == 1.0));
        assert!(normal_from_height(&m, Green::Directx).iter().all(|n| *n == [0.5, 0.5, 1.0, 1.0]));
    }

    #[test]
    fn parallax_pivots_on_the_median_and_stays_in_the_margin() {
        let m = master_of(&spiky_ramp(64), 64, 64);
        let (p, r) = parallax(&m, &ParallaxOpts::default());
        let med = median_f32(&p.px);
        assert!((med - 0.5).abs() < 1.0 / 255.0, "median {med}");
        assert!(p.px.iter().all(|&v| (MARGIN..=1.0 - MARGIN).contains(&v)));
        assert!(r.clipped_share > 0.0 && r.clipped_share < 0.01, "clipped {}", r.clipped_share);
        assert!(r.levels_used > 60, "levels {}", r.levels_used);
        let (inv, _) = parallax(&m, &ParallaxOpts { invert: true, ..Default::default() });
        assert!(inv.px.iter().zip(&p.px).all(|(a, b)| (a - (1.0 - b)).abs() < 1e-6));
    }

    #[test]
    fn white_puts_the_top_at_one_and_sinks_the_rest() {
        let m = master_of(&spiky_ramp(64), 64, 64);
        let (p, _) = parallax(&m, &ParallaxOpts { flat: FlatLevel::White, ..Default::default() });
        assert!(p.px.iter().all(|v| (MARGIN..=1.0).contains(v)));
        let at_top = p.px.iter().filter(|&&v| v == 1.0).count() as f32 / p.px.len() as f32;
        assert!(at_top >= 0.005, "share at 1.0: {at_top}");
    }

    #[test]
    fn smoothing_softens_a_step() {
        let step: Vec<f64> = (0..64 * 64).map(|i| if (i % 64) < 32 { -3.0 } else { 3.0 }).collect();
        let m = master_of(&step, 64, 64);
        let (sharp, _) = parallax(&m, &ParallaxOpts::default());
        let (soft, _) = parallax(&m, &ParallaxOpts { smooth_texels: 2.0, ..Default::default() });
        let jump = |g: &Gray| (g.px[32] - g.px[31]).abs();
        assert!(jump(&soft) < 0.5 * jump(&sharp));
    }

    #[test]
    fn match_depth_gives_one_times_the_normal() {
        let m = master_of(&sine(256, 4.0), 256, 256);
        let d = match_depth(&m);
        assert!((0.1..0.9).contains(&d), "depth {d}");
        let (_, r) = parallax(&m, &ParallaxOpts { depth: d, ..Default::default() });
        assert!((r.times_normal - 1.0).abs() < 1e-3, "{}x", r.times_normal);
        assert!((r.shader_depth_pct - 10.0).abs() < 1e-2, "{} %", r.shader_depth_pct);
        let tall = master_of(&sine(256, 400.0), 256, 256);
        assert_eq!(match_depth(&tall), 1.0);
    }

    #[test]
    fn depth_runs_from_1_to_100_percent() {
        let m = master_of(&spiky_ramp(64), 64, 64);
        for flat in [FlatLevel::MidGrey, FlatLevel::White] {
            let at = |depth| parallax(&m, &ParallaxOpts { depth, flat, ..Default::default() });
            // A White map's untouched surface is exactly 1.0; only its deep end has the margin.
            let top = if flat == FlatLevel::White { 1.0 } else { 1.0 - MARGIN + 1e-6 };
            for d in [0.01, 1.0] {
                let (g, _) = at(d);
                assert!(g.px.iter().all(|v| v.is_finite() && *v >= MARGIN - 1e-6 && *v <= top), "{flat:?} {d}");
            }
            assert!(at(0.01).1.levels_used < at(1.0).1.levels_used);
            // The old ends, 10 % and 90 %, are no longer where Depth stops.
            assert_ne!(at(0.01).0.px, at(0.1).0.px);
            assert_ne!(at(1.0).0.px, at(0.9).0.px);
            assert_eq!(at(0.0).0.px, at(0.01).0.px); // clamped below
            assert_eq!(at(2.0).0.px, at(1.0).0.px); // clamped above
        }
    }

    #[test]
    fn normal_from_height_round_trips_through_slopes() {
        let m = master_of(&sine(256, 3.0), 256, 256);
        let n = normal_from_height(&m, Green::Directx);
        let back = crate::from_normal::slopes(&n, 256, 256, false);
        let want = scharr_p(&m);
        let d = back.p.iter().zip(&want).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        assert!(d < 1e-3, "slope differs by {d}");
        let gl = normal_from_height(&m, Green::Opengl);
        assert!((gl[300][1] - (1.0 - n[300][1])).abs() < 1e-6);
    }
}
