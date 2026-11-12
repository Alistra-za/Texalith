//! `cone` copied from Tessalith's `crates/engine/src/cube/sample.rs` (`blur`); the rest is new.
//! Six faces to the `_e.png` strip: face size, cone blur and the game-gamma 6:1 layout.
use super::import::build;
use engine::cube::sample::{fetch, mip_chain, resample};
use engine::cube::{cube_dir, Cube};
use engine::dds::full_chain_len;
use engine::ops::linear_to_game_gamma;

/// The written face edge: 128 for a sharp reflection, 64 once Reflection softness reaches 0.1, never above the
/// source's cell (rounded down to a power of two) and never below 4.
pub fn face_size(softness: f32, cell: u32) -> u32 {
    let base = if softness < 0.1 { 128 } else { 64 };
    let cap = 1u32 << (31 - cell.max(1).leading_zeros());
    base.min(cap).max(4)
}

/// Cone blur: half-angle `amount` x 45 degrees (`amount` clamped to 0..=1), Gaussian weight with sigma = half the
/// cone, sampled from the coarsest mip whose texel angle is at most a quarter of the cone. `amount` 0 (or NaN)
/// returns the input. Copied from Tessalith's `cube::sample::blur`.
// shortcut: Gaussian cone blur, not GGX prefiltering; upgrade to a GGX split-sum chain if a CS PBR consumer of static cubes appears.
fn cone(c: &Cube, amount: f64) -> Cube {
    if amount.is_nan() || amount <= 0.0 {
        return c.clone();
    }
    let theta = (amount.min(1.0) * std::f64::consts::FRAC_PI_4) as f32;
    // A face spans 90 degrees, so a level of size m has texels about (pi/2)/m wide. 1e-4 slack: at amount 1 the
    // size-8 level sits exactly on the bound.
    let fits = |m: u32| std::f32::consts::FRAC_PI_2 / m as f32 <= theta / 4.0 * 1.0001;
    let level = (0..full_chain_len(c.size, c.size)).take_while(|&k| fits((c.size >> k).max(1))).last().unwrap_or(0);
    let coarse;
    let src = if level == 0 {
        c
    } else {
        coarse = mip_chain(c.clone(), level + 1).pop().unwrap();
        &coarse
    };
    // Fixed (2R+1)^2 grid in the tangent plane, step theta/R: bounded cost, about two samples per source texel.
    // Each entry: (cos a, sin a * ox / a, sin a * oy / a, weight).
    const R: i32 = 8;
    let sigma = theta / 2.0;
    let mut table = Vec::new();
    for j in -R..=R {
        for i in -R..=R {
            let (ox, oy) = (i as f32 * theta / R as f32, j as f32 * theta / R as f32);
            let ang = (ox * ox + oy * oy).sqrt();
            if ang > theta {
                continue;
            }
            // sin(a)/a turns the flat angle grid into equal solid angle per sample.
            let (sn, cs) = ang.sin_cos();
            let jac = if ang == 0.0 { 1.0 } else { sn / ang };
            let w = (-0.5 * (ang / sigma).powi(2)).exp() * jac;
            table.push((cs, ox * jac, oy * jac, w));
        }
    }
    let wsum: f32 = table.iter().map(|e| e.3).sum();
    build(c.size, |f, x, y| {
        let d = cube_dir(f, x, y, c.size);
        // Orthonormal tangent basis around d.
        let a = if d[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let t1 = normalize(cross(d, a));
        let t2 = cross(d, t1);
        let mut sum = [0.0f32; 4];
        for &(cs, a, b, w) in &table {
            let p = fetch(src, std::array::from_fn(|k| cs * d[k] + a * t1[k] + b * t2[k]));
            for ch in 0..4 {
                sum[ch] += w * p[ch];
            }
        }
        sum.map(|v| v / wsum)
    })
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    v.map(|c| c / l)
}

/// The largest face `blur` works on.
pub const BLUR_MAX: u32 = 256;

/// `blur_capped` with `BLUR_MAX`.
pub fn blur(c: &Cube, amount: f64) -> Cube {
    blur_capped(c, amount, BLUR_MAX)
}

/// The cone blur, run on a face of at most `cap` and resampled back to `c.size`. At 2048 a full-size blur is 25
/// million texels x up to 289 samples.
// shortcut: blurs at most a 256 face; raise BLUR_MAX if a very slight blur on a 1024+ cube looks soft.
pub fn blur_capped(c: &Cube, amount: f64, cap: u32) -> Cube {
    if amount > 0.0 && c.size > cap {
        let id = |d: [f32; 3]| d;
        return resample(&cone(&shrink(c, cap), amount), c.size, id);
    }
    cone(c, amount)
}

/// `c` halved until its face is at most `cap`. Each 2:1 `resample` is an exact 2x2 box; one 8:1 step would read only
/// a quarter of the texels.
fn shrink(c: &Cube, cap: u32) -> Cube {
    let mut m = resample(c, (c.size / 2).max(cap), |d| d);
    while m.size > cap {
        m = resample(&m, (m.size / 2).max(cap), |d| d);
    }
    m
}

/// The 6n x n strip, +X -X +Y -Y +Z -Z left to right, RGB in game gamma (clipped to 0..1), alpha 1.
pub fn strip(c: &Cube) -> (u32, u32, Vec<[f32; 4]>) {
    let n = c.size;
    let mut px = Vec::with_capacity((6 * n * n) as usize);
    for y in 0..n {
        for f in &c.faces {
            for x in 0..n {
                let p = f.at(x, y);
                px.push([linear_to_game_gamma(p[0]), linear_to_game_gamma(p[1]), linear_to_game_gamma(p[2]), 1.0]);
            }
        }
    }
    (6 * n, n, px)
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::cube::Cube;
    use engine::image::Rgba32F;
    use engine::ops::linear_to_game_gamma;

    fn cube_from(n: u32, f: impl Fn(usize, u32, u32) -> [f32; 4]) -> Cube {
        Cube {
            size: n,
            faces: std::array::from_fn(|k| {
                let mut img = Rgba32F::new(n, n, [0.0; 4]);
                for y in 0..n {
                    for x in 0..n {
                        *img.at_mut(x, y) = f(k, x, y);
                    }
                }
                img
            }),
        }
    }

    /// Six muted face colours, a white band on rows 0..n/8 and a black band on columns 0..n/8.
    fn arrow_cube(n: u32) -> Cube {
        cube_from(n, |f, x, y| {
            if y < n / 8 {
                [1.0; 4]
            } else if x < n / 8 {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                let g = 0.4 + 0.04 * f as f32;
                [g, g + 0.01, g + 0.02, 1.0]
            }
        })
    }

    fn face_std(img: &Rgba32F) -> f32 {
        (0..3)
            .map(|k| {
                let n = img.px.len() as f32;
                let mean = img.px.iter().map(|p| p[k]).sum::<f32>() / n;
                (img.px.iter().map(|p| (p[k] - mean).powi(2)).sum::<f32>() / n).sqrt()
            })
            .sum::<f32>()
            / 3.0
    }

    #[test]
    fn face_size_follows_softness_and_the_source() {
        assert_eq!(face_size(0.0, 4096), 128);
        assert_eq!(face_size(0.09, 4096), 128);
        assert_eq!(face_size(0.1, 4096), 64);
        assert_eq!(face_size(0.0, 100), 64);
        assert_eq!(face_size(0.0, 48), 32);
        assert_eq!(face_size(0.0, 2), 4);
    }

    #[test]
    fn blur_zero_is_identity_and_blur_one_flattens() {
        let c = arrow_cube(32);
        assert_eq!(blur(&c, 0.0), c);
        let b = blur(&c, 1.0);
        for f in 0..6 {
            let (before, after) = (face_std(&c.faces[f]), face_std(&b.faces[f]));
            // 1/3: on -Z the +Y and -Z white bands meet at one edge and form a band twice as wide.
            assert!(after < before / 3.0, "face {f}: {before} -> {after}");
        }
    }

    #[test]
    fn a_large_soft_cube_is_blurred_small_and_comes_back_full_size() {
        // One bright texel, so a full-size blur differs from the small one.
        let c = cube_from(16, |f, x, y| if (f, x, y) == (0, 5, 9) { [50.0, 40.0, 30.0, 1.0] } else { [0.1, 0.1, 0.1, 1.0] });
        let id = |d: [f32; 3]| d;
        let got = blur_capped(&c, 0.5, 8);
        assert_eq!(got.size, 16);
        assert!(got == resample(&blur_capped(&resample(&c, 8, id), 0.5, 8), 16, id));
        assert!(blur_capped(&c, 0.0, 8) == c);
    }

    #[test]
    fn shrinking_8_to_1_averages_every_texel() {
        // A lone bright texel at a block corner, where one 8:1 resample's taps never land.
        let c = cube_from(16, |f, x, y| if (f, x, y) == (0, 0, 0) { [64.0, 64.0, 64.0, 1.0] } else { [0.0, 0.0, 0.0, 1.0] });
        let s = shrink(&c, 2);
        assert_eq!(s.size, 2);
        assert!((s.faces[0].at(0, 0)[0] - 1.0).abs() < 1e-4, "{}", s.faces[0].at(0, 0)[0]);
    }

    #[test]
    fn bright_sources_clip_and_stay_finite() {
        let mut c = cube_from(4, |_, _, _| [1e6, 1e6, 1e6, 1.0]);
        crate::cube::adjust::tone(&mut c, &crate::cube::Settings { brightness: 10.0, ..Default::default() });
        let (w, h, px) = strip(&c);
        assert_eq!((w, h, px.len()), (24, 4, 96));
        assert!(px.iter().flatten().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
    }

    #[test]
    fn the_strip_puts_face_k_at_column_k_n() {
        let n = 4;
        let c = cube_from(n, |k, _, _| [k as f32 / 8.0, k as f32 / 8.0, k as f32 / 8.0, 0.3]);
        let (w, h, px) = strip(&c);
        assert_eq!((w, h), (6 * n, n));
        for k in 0..6u32 {
            let want = linear_to_game_gamma(k as f32 / 8.0);
            for y in 0..n {
                for x in 0..n {
                    let p = px[(y * w + k * n + x) as usize];
                    assert_eq!(p, [want, want, want, 1.0], "face {k} ({x},{y})");
                }
            }
        }
    }
}
