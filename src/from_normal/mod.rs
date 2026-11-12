//! Height, curvature and AO from a normal map. The core of Tessalith's `engine::from_normal`
//! (`crates/engine/src/from_normal/`, commit 36384560, GPL-3.0), which Tessalith 3.0 removes, plus two changes from
//! spike 4: the Nyquist denominator fix (`solve.rs`, `spectrum_with`) and the AO tangent plane from the height's own
//! slopes (`ao.rs`, `march`).
//! shortcut: the core only. `generate`'s sizing, power-of-two resizing, height mapping, resampling, tiling detection
//! and green detection come over with the From normal tab.
pub mod ao;
mod detect;
mod fft;
pub mod files;
pub mod input;
pub mod maps;
pub mod master;
pub mod open;
pub mod settings;
pub mod solve;
pub mod worker;
#[cfg(test)]
mod spike4;

pub use ao::march;
pub use solve::{curvature, fit, solve};

/// Image-space slopes (rows down), de-tilted: `p` and `q` have zero mean, and `tilt` is the mean `(p̄, q̄)` removed.
#[derive(Clone, Debug)]
pub struct Slopes {
    pub width: u32,
    pub height: u32,
    pub p: Vec<f64>,
    pub q: Vec<f64>,
    pub tilt: (f64, f64),
}

/// Image-space slopes `(p, q)` (y down) of a stored normal, DirectX green: x and y decoded from R and G, z rebuilt
/// (B is ignored, so BC5's zero blue reads like BC1/3/7) and clamped to at least 0.1.
fn texel_slope(px: [f32; 4]) -> (f64, f64) {
    let (x, y) = (f64::from(px[0]) * 2.0 - 1.0, f64::from(px[1]) * 2.0 - 1.0);
    let z = (1.0 - x * x - y * y).max(0.0).sqrt().max(0.1);
    (-x / z, -y / z)
}

/// The de-tilted slopes of a stored normal map, row-major `width × height` texels in 0..1; OpenGL green is flipped
/// first.
pub fn slopes(px: &[[f32; 4]], width: u32, height: u32, opengl: bool) -> Slopes {
    let flip = if opengl { -1.0 } else { 1.0 };
    let (mut p, mut q): (Vec<f64>, Vec<f64>) = px
        .iter()
        .map(|&px| {
            let (p, q) = texel_slope(px);
            // A non-finite texel reads as flat, so it cannot poison the mean and the whole solve.
            if p.is_finite() && q.is_finite() {
                (p, flip * q)
            } else {
                (0.0, 0.0)
            }
        })
        .unzip();
    let n = p.len().max(1) as f64;
    // Shifted means: a constant field de-tilts to exact zeros.
    let mean = |v: &[f64]| {
        v.first()
            .map_or(0.0, |&a| a + v.iter().map(|x| x - a).sum::<f64>() / n)
    };
    let tilt = (mean(&p), mean(&q));
    p.iter_mut().for_each(|v| *v -= tilt.0);
    q.iter_mut().for_each(|v| *v -= tilt.1);
    Slopes {
        width,
        height,
        p,
        q,
        tilt,
    }
}

/// The angle of the mean normal from +z, in degrees.
pub fn tilt_deg(s: &Slopes) -> f32 {
    s.tilt.0.hypot(s.tilt.1).atan().to_degrees() as f32
}

/// The sorted value at index `round(q·(n − 1))` (as `fit`'s p95); 0 when empty. Selection, not a full sort.
fn percentile(mut v: Vec<f64>, q: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let i = (v.len().saturating_sub(1) as f64 * q).round() as usize;
    *v.select_nth_unstable_by(i, f64::total_cmp).1
}

/// The one size ceiling for every output (spike 6: a 4K solve takes at most 2.1 s and 1.7 GB).
pub const MAX_SIZE: u32 = 4096;

/// The offered output sizes: powers of two from 256 up to the source's power-of-two longer side and `MAX_SIZE`, or
/// that side alone when it is smaller than 256.
pub fn sizes(source: (u32, u32)) -> Vec<u32> {
    let (w, h) = engine::resize::pot_dims(source);
    let longer = w.max(h);
    let v: Vec<u32> = [256, 512, 1024, 2048, MAX_SIZE].into_iter().filter(|&s| s <= longer).collect();
    if v.is_empty() {
        vec![longer.min(MAX_SIZE)]
    } else {
        v
    }
}

/// The source's power-of-two aspect with `long` on its longer side; each side at least 1.
pub fn dims(source: (u32, u32), long: u32) -> (u32, u32) {
    let (w, h) = engine::resize::pot_dims(source);
    let l = u64::from(w.max(h));
    let side = |s: u32| ((u64::from(s) * u64::from(long) / l) as u32).max(1);
    (side(w), side(h))
}

/// GTAO settings. `depth` scales the height before the march (the caller applies it, as Tessalith's `generate`).
#[derive(Debug, Clone, PartialEq)]
pub struct AoOpts {
    pub radius: f32,
    pub slices: u32,
    pub steps: u32,
    pub depth: f32,
    pub intensity: f32,
}

impl Default for AoOpts {
    fn default() -> Self {
        Self {
            radius: 0.0625,
            slices: 16,
            steps: 16,
            depth: 1.0,
            intensity: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ao::{march_with, BIAS};
    use super::solve::tests::{bump, lcg, torus, Directx};
    use super::solve::{curvature_with, solve_with};
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32};

    /// The torus plus noise up to Nyquist, so the unfixed and fixed solves differ.
    fn noisy(w: usize, h: usize) -> super::solve::tests::Rgba32F {
        let mut n = bump(&torus(w, h), w, h, 4.0, true);
        let mut seed = 5u64;
        for px in &mut n.px {
            px[0] += (0.02 * lcg(&mut seed)) as f32;
            px[1] += (0.02 * lcg(&mut seed)) as f32;
        }
        n
    }

    #[test]
    fn the_port_matches_tessalith_before_the_fix_and_the_march_change() {
        let (w, h) = (256, 256);
        let n = noisy(w, h);
        let input = engine::image::Rgba32F {
            width: w as u32,
            height: h as u32,
            px: n.px.clone(),
        };
        for wrap in [true, false] {
            let o = engine::from_normal::Opts {
                size: 256,
                tiling: if wrap {
                    engine::from_normal::Tiling::Wrap
                } else {
                    engine::from_normal::Tiling::Mirror
                },
                relief: 0.3,
                green: Some(engine::config::GreenConvention::Directx),
                height: None,
                ao: Some(engine::from_normal::AoOpts::default()),
                curvature: true,
            };
            let e = engine::from_normal::generate(
                &input,
                &o,
                &AtomicU32::new(0),
                &AtomicBool::new(false),
            )
            .expect("engine generates");
            let s = super::solve::tests::slopes(&n, Directx);
            let max_diff = |a: &[f32], b: &[f32]| {
                a.iter()
                    .zip(b)
                    .map(|(x, y)| (x - y).abs())
                    .fold(0.0f32, f32::max)
            };
            // At the solve size `generate` marches the solve × depth (1) with the normal's de-tilted slopes, unscaled.
            let ao = march_with(
                &solve_with(&s, wrap, 0.3, false),
                &s,
                wrap,
                &AoOpts::default(),
                BIAS,
                0.0,
            );
            let d = max_diff(&ao, &e.ao.unwrap().px);
            assert!(d < 1e-6, "wrap {wrap}: AO differs by {d}");
            let d = max_diff(
                &curvature_with(&s, wrap, 0.3, false),
                &e.curvature.unwrap().px,
            );
            assert!(d < 1e-6, "wrap {wrap}: curvature differs by {d}");
        }
        // The fix is the one change: on this wrapped input it moves the AO (mirror cannot move, see solve's tests).
        let s = super::solve::tests::slopes(&n, Directx);
        let run = |nyquist_den: bool| {
            march(
                &solve_with(&s, true, 0.3, nyquist_den),
                w as u32,
                h as u32,
                true,
                &AoOpts::default(),
                &AtomicBool::new(false),
            )
            .unwrap()
        };
        let d = run(true)
            .iter()
            .zip(&run(false))
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(d > 1e-3, "the fix changed the AO by only {d}");
    }

    #[test]
    fn sizes_are_powers_of_two_up_to_the_source_and_4096() {
        assert_eq!(engine::resize::pot_dims((1000, 700)).0, 1024);
        assert_eq!(sizes((1000, 700)), vec![256, 512, 1024]);
        assert_eq!(sizes((8192, 8192)), vec![256, 512, 1024, 2048, 4096]);
        assert_eq!(sizes((8, 8)), vec![8]);
        assert_eq!(sizes((1, 1)), vec![1]);
    }

    #[test]
    fn dims_keep_the_power_of_two_aspect() {
        let pot = engine::resize::pot_dims((1000, 700));
        assert_eq!(dims((1000, 700), 1024), pot);
        assert_eq!(dims((2048, 512), 256), (256, 64));
        assert_eq!(dims((4096, 1), 256), (256, 1));
    }

    #[test]
    fn curvature_does_not_see_the_nyquist_fix() {
        // Its σ 2 Gaussian passes exp(−2π²) ≈ 3e-9 at Nyquist, so the changed bins cannot reach the output.
        let n = noisy(256, 256);
        let s = super::solve::tests::slopes(&n, Directx);
        for wrap in [true, false] {
            let d = curvature_with(&s, wrap, 0.3, false)
                .iter()
                .zip(&curvature(&s, wrap, 0.3))
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f32, f32::max);
            assert!(d < 1e-5, "wrap {wrap}: curvature moved by {d}");
        }
    }
}
