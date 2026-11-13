//! The master height: the zero-mean surface in texels at the output size, its slopes and its edge mode. Every
//! From normal output is a function of the master plus that output's own options.
use super::input::Kind;
use super::{ao, detect, dims, fit, sizes, slopes, solve, tilt_deg, Slopes, MAX_SIZE};
use crate::resample::resample;
use engine::image::Rgba32F;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Edges {
    #[default]
    Repeats,
    DoesNotRepeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Green {
    #[default]
    Directx,
    Opengl,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MasterOpts {
    /// The longer side, a power of two up to the source's own and `MAX_SIZE`.
    pub size: u32,
    pub edges: Edges,
    /// `None`: guess from the normal.
    pub green: Option<Green>,
    /// Flatten broad shapes, 0 to 1.
    pub flatten: f32,
    /// A height input's black-to-white span as % of the width, used when its marker gives none (0.05 to 5).
    pub normal_depth_pct: f32,
}

impl Default for MasterOpts {
    fn default() -> Self {
        Self { size: 1024, edges: Edges::Repeats, green: None, flatten: 0.3, normal_depth_pct: 1.0 }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MasterReport {
    pub green: Green,
    pub green_guessed: bool,
    pub green_sure: bool,
    pub seam_ratio: f32,
    /// The seam ratio is above 2: "These edges may not join".
    pub seam_hint: bool,
    pub tilt_deg: f32,
    /// Normal input only: how well the height rebuilds the normal.
    pub fit_mean_deg: Option<f32>,
    pub fit_p95_deg: Option<f32>,
    /// False for a height input with no marker span: its scale came from `normal_depth_pct`.
    pub span_known: bool,
}

#[derive(Debug, Clone)]
pub struct Master {
    pub width: u32,
    pub height: u32,
    pub wrap: bool,
    /// Zero-mean height in texels at `width`.
    pub h: Vec<f64>,
    /// The normal's slopes at the master size (green applied), or the height's central differences.
    pub slopes: Slopes,
    pub from_normal: bool,
    pub report: MasterReport,
}

#[derive(Debug)]
pub enum BuildError {
    Parallax,
    BadOpts(&'static str),
    Cancelled,
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::Parallax => f.write_str(
                "This is a parallax map, which is made from height data and cannot be turned back. Load its height \
                 data, or build it again from the normal or image.",
            ),
            BuildError::BadOpts(e) => f.write_str(e),
            BuildError::Cancelled => f.write_str("cancelled"),
        }
    }
}

impl std::error::Error for BuildError {}

/// The seam ratio above which the seam hint shows (Tessalith's wrap threshold).
const SEAM_HINT: f64 = 2.0;
/// Fixed sizes for the seam hint and the green guess, as Tessalith, so a preview build and the final one agree.
const DETECT_SIZE: u32 = 1024;
const GREEN_SIZE: u32 = 512;

/// `s` resampled to `size` (slopes are not rescaled); the full-size tilt is kept.
// shortcut: bilinear on an axis that grows while the other shrinks (non-power-of-two sources), where Tessalith
// enlarges first and then area-filters; move to that if such sources show aliasing.
fn resize(s: &Slopes, size: (u32, u32)) -> Slopes {
    if (s.width, s.height) == size {
        return s.clone();
    }
    let img = Rgba32F {
        width: s.width,
        height: s.height,
        px: s.p.iter().zip(&s.q).map(|(&p, &q)| [p as f32, q as f32, 0.0, 0.0]).collect(),
    };
    let r = resample(&img, size);
    Slopes {
        width: size.0,
        height: size.1,
        p: r.px.iter().map(|v| f64::from(v[0])).collect(),
        q: r.px.iter().map(|v| f64::from(v[1])).collect(),
        tilt: s.tilt,
    }
}

/// The master of `img` at `o.size`. A parallax map is refused before anything else.
pub fn build(img: &Rgba32F, kind: &Kind, o: &MasterOpts, cancel: &AtomicBool) -> Result<Master, BuildError> {
    use BuildError::BadOpts;
    if *kind == Kind::Parallax {
        return Err(BuildError::Parallax);
    }
    let src = (img.width, img.height);
    if src.0 == 0 || src.1 == 0 {
        return Err(BadOpts("the image is empty"));
    }
    let largest = sizes(src).last().copied().unwrap_or(1);
    if !o.size.is_power_of_two() || o.size > MAX_SIZE.min(largest) {
        return Err(BadOpts("the size must be a power of two up to the image's own size and 4096"));
    }
    if !(0.0..=1.0).contains(&o.flatten) {
        return Err(BadOpts("Flatten broad shapes must be between 0 and 1"));
    }
    if !(0.05..=5.0).contains(&o.normal_depth_pct) {
        return Err(BadOpts("the depth must be between 0.05 and 5 % of the width"));
    }
    let step = || match cancel.load(Ordering::Relaxed) {
        true => Err(BuildError::Cancelled),
        false => Ok(()),
    };
    step()?;
    let wrap = o.edges == Edges::Repeats;
    let size = dims(src, o.size);
    let longer = largest.max(1);
    let mut report = MasterReport { span_known: true, ..Default::default() };
    let (h, s) = match kind {
        Kind::Normal => {
            let full = slopes(&img.px, src.0, src.1, false);
            report.tilt_deg = tilt_deg(&full);
            step()?;
            report.seam_ratio = detect::seam_ratio(&resize(&full, dims(src, longer.min(DETECT_SIZE)))) as f32;
            step()?;
            let (green, guessed, sure) = match o.green {
                Some(g) => (g, false, true),
                None => {
                    let (g, sure) = detect::guess_green(&resize(&full, dims(src, longer.min(GREEN_SIZE))), wrap);
                    (g, true, sure)
                }
            };
            (report.green, report.green_guessed, report.green_sure) = (green, guessed, sure);
            step()?;
            let mut s = resize(&full, size);
            if green == Green::Opengl {
                s = detect::flipped(&s);
            }
            let h = solve(&s, wrap, f64::from(o.flatten));
            let (mean, p95) = fit(&s, &h);
            (report.fit_mean_deg, report.fit_p95_deg) = (Some(mean), Some(p95));
            (h, s)
        }
        Kind::Height { span_texels } => {
            let d: Vec<f64> = resample(img, size).px.iter().map(|p| f64::from(p[0])).collect();
            let span = match span_texels {
                Some(t) => t * f64::from(size.0) / f64::from(src.0),
                None => f64::from(o.normal_depth_pct) / 100.0 * f64::from(size.0),
            };
            report.span_known = span_texels.is_some();
            let mean = d.iter().sum::<f64>() / d.len().max(1) as f64;
            let h: Vec<f64> = d.iter().map(|v| (v - mean) * span).collect();
            let s = ao::central(&h, size.0 as usize, size.1 as usize, wrap);
            // shortcut: the seam ratio at the master size, not at DETECT_SIZE as for a normal; resample first if the
            // hint flips between sizes on real heights.
            report.seam_ratio = detect::seam_ratio(&s) as f32;
            (h, s)
        }
        Kind::Parallax => unreachable!("refused above"),
    };
    step()?;
    report.seam_hint = f64::from(report.seam_ratio) > SEAM_HINT;
    Ok(Master { width: size.0, height: size.1, wrap, h, slopes: s, from_normal: *kind == Kind::Normal, report })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::from_normal::solve::tests::{bump, gauss, lcg, range};
    use engine::image::Rgba32F;

    pub(crate) fn no() -> AtomicBool {
        AtomicBool::new(false)
    }

    fn engine_img(n: crate::from_normal::solve::tests::Rgba32F) -> Rgba32F {
        Rgba32F { width: n.width, height: n.height, px: n.px }
    }

    /// A wrapped Gaussian bump 8 texels high, σ n/8, as a DirectX normal.
    pub(crate) fn bump_normal(n: usize) -> Rgba32F {
        let s = n as f64 / 8.0;
        let h: Vec<f64> = (0..n * n)
            .map(|i| 8.0 * gauss((i % n) as f64, (i / n) as f64, n as f64 / 2.0, n as f64 / 2.0, s))
            .collect();
        engine_img(bump(&h, n, n, 0.5, true))
    }

    fn noisy_normal(w: u32, h: u32) -> Rgba32F {
        let mut seed = 3u64;
        let px = (0..w * h)
            .map(|_| [0.5 + 0.2 * lcg(&mut seed) as f32, 0.5 + 0.2 * lcg(&mut seed) as f32, 1.0, 1.0])
            .collect();
        Rgba32F { width: w, height: h, px }
    }

    fn opengl(n: &Rgba32F) -> Rgba32F {
        Rgba32F { px: n.px.iter().map(|p| [p[0], 1.0 - p[1], p[2], p[3]]).collect(), ..n.clone() }
    }

    /// 0 at the left column to 1 at the right, grey.
    fn grey_ramp(n: u32) -> Rgba32F {
        let px = (0..n * n)
            .map(|i| {
                let v = (i % n) as f32 / (n - 1).max(1) as f32;
                [v, v, v, 1.0]
            })
            .collect();
        Rgba32F { width: n, height: n, px }
    }

    fn opts(size: u32) -> MasterOpts {
        MasterOpts { size, ..Default::default() }
    }

    #[test]
    fn a_bump_normal_builds_a_bump_at_the_asked_size() {
        let m = build(&bump_normal(256), &Kind::Normal, &opts(128), &no()).unwrap();
        assert_eq!((m.width, m.height), (128, 128));
        let (c, corner) = (m.h[64 * 128 + 64], m.h[0]);
        // Measured 1.19 above the corner: Flatten .3 damps this broad σ 16 bump from its 4 texels.
        assert!(c > corner + 0.8, "centre {c} must stand above the corner {corner}");
        let fit = m.report.fit_mean_deg.unwrap();
        assert!(fit < 2.0, "fit {fit}°"); // measured 1.04°
    }

    #[test]
    fn an_odd_sized_source_solves_on_power_of_two_sides() {
        let m = build(&noisy_normal(1000, 700), &Kind::Normal, &opts(512), &no()).unwrap();
        assert_eq!((m.width, m.height), crate::from_normal::dims((1000, 700), 512));
        assert!(m.width.is_power_of_two() && m.height.is_power_of_two());
        assert!(m.h.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn a_tiny_source_builds() {
        for side in [1, 2, 8] {
            let m = build(&noisy_normal(side, side), &Kind::Normal, &opts(side), &no()).unwrap();
            assert_eq!(m.h.len(), (side * side) as usize);
            assert!(m.h.iter().all(|v| v.is_finite()), "side {side}");
        }
    }

    #[test]
    fn an_opengl_normal_is_guessed_and_flipped() {
        let m = build(&opengl(&bump_normal(256)), &Kind::Normal, &opts(256), &no()).unwrap();
        assert_eq!((m.report.green, m.report.green_guessed, m.report.green_sure), (Green::Opengl, true, true));
        assert!(m.h[128 * 256 + 128] > m.h[0]);
        let told = MasterOpts { green: Some(Green::Directx), ..opts(256) };
        let m = build(&opengl(&bump_normal(256)), &Kind::Normal, &told, &no()).unwrap();
        assert_eq!((m.report.green, m.report.green_guessed), (Green::Directx, false));
    }

    #[test]
    fn a_height_input_uses_its_span_or_normal_depth() {
        let g = grey_ramp(256);
        let m = build(&g, &Kind::Height { span_texels: Some(20.0) }, &opts(128), &no()).unwrap();
        let r = range(&m.h);
        assert!((r - 10.0).abs() < 0.2, "a 20-texel span at 256 is 10 texels at 128, got {r}");
        assert!(m.report.span_known);
        let o = MasterOpts { normal_depth_pct: 2.0, ..opts(256) };
        let m = build(&g, &Kind::Height { span_texels: None }, &o, &no()).unwrap();
        let r = range(&m.h);
        assert!((r - 5.12).abs() < 0.1, "2 % of 256 texels, got {r}"); // measured 5.12
        assert!(!m.report.span_known);
        assert!(m.h.iter().sum::<f64>().abs() < 1e-6);
    }

    #[test]
    fn a_parallax_input_is_refused() {
        let e = build(&grey_ramp(16), &Kind::Parallax, &MasterOpts::default(), &no()).err().unwrap();
        assert!(matches!(e, BuildError::Parallax));
        assert!(e.to_string().starts_with("This is a parallax map"));
    }

    #[test]
    fn bad_options_and_cancel_are_errors() {
        let n = bump_normal(64);
        for o in [opts(48), opts(128), MasterOpts { flatten: 1.5, ..opts(64) }, MasterOpts { normal_depth_pct: 9.0, ..opts(64) }] {
            assert!(matches!(build(&n, &Kind::Normal, &o, &no()), Err(BuildError::BadOpts(_))));
        }
        assert!(matches!(build(&n, &Kind::Normal, &opts(64), &AtomicBool::new(true)), Err(BuildError::Cancelled)));
    }

    #[test]
    fn a_cut_tile_raises_the_seam_hint() {
        // A quadratic in x: its slope runs 0 to 0.16 across the tile, so it jumps back at the wrap seam.
        let ramp: Vec<f64> = (0..256 * 256).map(|i| 20.0 * (((i % 256) as f64 + 0.5) / 256.0).powi(2)).collect();
        let n = engine_img(bump(&ramp, 256, 256, 0.5, false));
        let m = build(&n, &Kind::Normal, &opts(256), &no()).unwrap();
        assert!(m.wrap, "Repeats stays the choice");
        assert!(m.report.seam_hint && m.report.seam_ratio > 2.0, "ratio {}", m.report.seam_ratio);
        let tile = build(&bump_normal(256), &Kind::Normal, &opts(256), &no()).unwrap();
        assert!(!tile.report.seam_hint, "ratio {}", tile.report.seam_ratio);
    }
}
