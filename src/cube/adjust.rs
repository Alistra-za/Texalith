//! The Advanced steps on a linear-light cube: Turn, Mirror and Ground are copied from Tessalith's
//! `crates/engine/src/cube/filter.rs` (`yaw`, `mirror`, `ground_fill`); `tone` is new. Alpha is never changed.
use super::{Ground, Settings};
use engine::cube::sample::resample;
use engine::cube::world::{to_cube, to_world};
use engine::cube::{cube_dir, Cube};
use engine::ops::game_gamma_to_linear;

// Copied from Tessalith `cube/filter.rs` (`yaw`).
/// The cube turned `degrees` about world up, positive clockwise seen from above (Skyrim's heading sense: 90 moves
/// east to south). 0 returns the input unchanged.
pub fn turn(c: &Cube, degrees: f32) -> Cube {
    if degrees == 0.0 {
        return c.clone();
    }
    // out(d) = in(R d), R the counter-clockwise-from-above turn by `degrees` about world Z (up), in world space.
    let (s, co) = degrees.to_radians().sin_cos();
    resample(c, c.size, |d| {
        let w = to_world(d);
        to_cube([w[0] * co - w[1] * s, w[0] * s + w[1] * co, w[2]])
    })
}

// Copied from Tessalith `cube/filter.rs` (`mirror`).
/// The cube with world east and west swapped: `out(d) = in(to_cube(diag(-1, 1, 1) * to_world(d)))`.
pub fn mirror(c: &Cube) -> Cube {
    resample(c, c.size, |d| {
        let w = to_world(d);
        to_cube([-w[0], w[1], w[2]])
    })
}

// Copied from Tessalith `cube/filter.rs` (`fill_weight`).
/// Blend weight of the fill at elevation `e`: 1 below `h - 1`, 0 above `h + 1`, linear between.
fn fill_weight(e: f64, h: f64) -> f32 {
    ((h + 1.0 - e) / 2.0).clamp(0.0, 1.0) as f32
}

// Copied from Tessalith `cube/filter.rs` (`ground_fill`), `GroundFill::Color` as `Colour`, `Mode(Gradient)` as `Darken`.
/// Fill the cube below `horizon` degrees; `Keep` changes nothing.
pub fn ground(c: &mut Cube, g: Ground, horizon: f32) {
    if g == Ground::Keep {
        return;
    }
    let size = c.size;
    let elev = |f: usize, i: usize| {
        let d = cube_dir(f, i as u32 % size, i as u32 / size, size);
        (to_world(d)[2].clamp(-1.0, 1.0) as f64).asin().to_degrees()
    };
    let h = f64::from(horizon);
    let rgb_of = |p: &[f32; 4]| [p[0] as f64, p[1] as f64, p[2] as f64];
    let mean_where = |c: &Cube, keep: &dyn Fn(f64) -> bool| {
        let (mut sum, mut n) = ([0f64; 3], 0u32);
        for (f, face) in c.faces.iter().enumerate() {
            for (i, p) in face.px.iter().enumerate() {
                if keep(elev(f, i)) {
                    let v = rgb_of(p);
                    (0..3).for_each(|k| sum[k] += v[k]);
                    n += 1;
                }
            }
        }
        (sum, n)
    };
    // Horizon colour: mean over [h, h+5]. If no texel is there (very small cubes), pass 1 finds the smallest
    // elevation e0 >= h and pass 2 averages the texels with e < e0 + 0.5.
    let (mut sum, mut n) = mean_where(c, &|e| (h..=h + 5.0).contains(&e));
    if n == 0 {
        let e0 = (0..6)
            .flat_map(|f| (0..(size * size) as usize).map(move |i| (f, i)))
            .map(|(f, i)| elev(f, i))
            .filter(|&e| e >= h)
            .fold(f64::MAX, f64::min);
        (sum, n) = mean_where(c, &|e| e >= h && e < e0 + 0.5);
    }
    let hc = sum.map(|v| (v / n.max(1) as f64) as f32);
    let fill = match g {
        Ground::Colour(rgb) => Some(rgb.map(|v| game_gamma_to_linear(v as f32 / 255.0))),
        _ => None,
    };
    for (f, face) in c.faces.iter_mut().enumerate() {
        for (i, p) in face.px.iter_mut().enumerate() {
            let e = elev(f, i);
            let a = fill_weight(e, h);
            if a == 0.0 {
                continue;
            }
            let target = fill.unwrap_or_else(|| {
                let t = ((h - e) / (h + 90.0)).clamp(0.0, 1.0) as f32;
                hc.map(|v| v * (1.0 - 0.5 * t))
            });
            for k in 0..3 {
                p[k] = p[k] * (1.0 - a) + target[k] * a;
            }
        }
    }
}

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Brightness, Highlight rolloff, Colour strength and Tint, in linear light, after the blur. Each step is skipped at
/// its default, so the default settings leave the cube bit-identical.
pub fn tone(c: &mut Cube, s: &Settings) {
    let bright = s.brightness != 0.0;
    let gain = 2f32.powf(s.brightness);
    // Tessalith's highlight clamp: white point w, l' = min(l (1 + l / w^2) / (1 + l), 1) on Rec. 709 luma.
    let w2 = s.rolloff.map(|w| w * w);
    let colour = s.colour != 1.0;
    // A multiply by the tint decoded with game gamma equals Tessalith's display-space multiply.
    let tint = (s.tint != [255; 3]).then(|| s.tint.map(|v| game_gamma_to_linear(v as f32 / 255.0)));
    if !bright && w2.is_none() && !colour && tint.is_none() {
        return;
    }
    for p in c.faces.iter_mut().flat_map(|f| f.px.iter_mut()) {
        let mut rgb = [p[0], p[1], p[2]];
        if bright {
            rgb = rgb.map(|v| v * gain);
        }
        if let Some(w2) = w2 {
            let l = luma(rgb);
            if l > 0.0 {
                let l2 = (l * (1.0 + l / w2) / (1.0 + l)).min(1.0);
                rgb = rgb.map(|v| v * (l2 / l));
            }
        }
        if colour {
            let y = luma(rgb);
            rgb = rgb.map(|v| (y + (v - y) * s.colour).max(0.0));
        }
        if let Some(t) = tint {
            rgb = [rgb[0] * t[0], rgb[1] * t[1], rgb[2] * t[2]];
        }
        p[..3].copy_from_slice(&rgb);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::cube::sample::fetch;
    use engine::cube::world::{to_cube, to_world};
    use engine::cube::{cube_dir, dir_to_face};
    use engine::image::Rgba32F;
    use engine::ops::{game_gamma_to_linear, linear_to_game_gamma};

    fn flat(rgb: [f32; 3]) -> Cube {
        let faces = std::array::from_fn(|_| Rgba32F::new(4, 4, [rgb[0], rgb[1], rgb[2], 0.7]));
        Cube { size: 4, faces }
    }

    fn flat_size(size: u32, v: f32) -> Cube {
        Cube { size, faces: std::array::from_fn(|_| Rgba32F::new(size, size, [v, v, v, 1.0])) }
    }

    fn colour_code(c: &mut Cube, rgb: [u8; 3], h: f64) {
        ground(c, Ground::Colour(rgb), h as f32);
    }

    fn grad(c: &mut Cube, h: f64) {
        ground(c, Ground::Darken, h as f32);
    }

    fn elevation(face: usize, x: u32, y: u32, size: u32) -> f64 {
        let w = to_world(cube_dir(face, x, y, size));
        (w[2] as f64).asin().to_degrees()
    }

    fn texels(size: u32) -> impl Iterator<Item = (usize, usize, f64)> {
        (0..6).flat_map(move |f| (0..size * size).map(move |i| (f, i as usize, elevation(f, i % size, i / size, size))))
    }

    fn luma(p: [f32; 4]) -> f32 {
        0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
    }

    /// A 16 cube, red where world east is within about 25 degrees, black elsewhere.
    fn red_east() -> Cube {
        let n = 16;
        let mut c = flat_size(n, 0.0);
        for (f, face) in c.faces.iter_mut().enumerate() {
            for (i, p) in face.px.iter_mut().enumerate() {
                if to_world(cube_dir(f, i as u32 % n, i as u32 / n, n))[0] > 0.9 {
                    *p = [1.0, 0.0, 0.0, 1.0];
                }
            }
        }
        c
    }

    const EAST: [f32; 3] = [1.0, 0.0, 0.0];
    const WEST: [f32; 3] = [-1.0, 0.0, 0.0];
    const SOUTH: [f32; 3] = [0.0, -1.0, 0.0];

    fn red_at(c: &Cube, w: [f32; 3]) -> f32 {
        fetch(c, to_cube(w))[0]
    }

    #[test]
    fn turn_90_moves_east_to_south() {
        let t = turn(&red_east(), 90.0);
        assert!(red_at(&t, SOUTH) > 0.9, "{}", red_at(&t, SOUTH));
        assert!(red_at(&t, EAST) < 0.1, "{}", red_at(&t, EAST));
    }

    #[test]
    fn mirror_swaps_east_and_west() {
        let m = mirror(&red_east());
        assert!(red_at(&m, WEST) > 0.9, "{}", red_at(&m, WEST));
        assert!(red_at(&m, EAST) < 0.1, "{}", red_at(&m, EAST));
    }

    #[test]
    fn turn_0_and_keep_change_nothing() {
        let c = red_east();
        assert_eq!(turn(&c, 0.0), c);
        let mut k = c.clone();
        ground(&mut k, Ground::Keep, 30.0);
        assert_eq!(k, c);
    }

    fn settings() -> Settings {
        Settings::default()
    }

    #[test]
    fn tone_defaults_change_nothing() {
        let mut c = flat([0.1, 0.2, 0.3]);
        for (i, f) in c.faces.iter_mut().enumerate() {
            let col = [0.05 + 0.05 * i as f32, 0.9, 3.5, 1.0];
            for y in 1..3 {
                for x in 0..4 {
                    f.px[y * 4 + x] = col;
                }
            }
        }
        let before = c.clone();
        tone(&mut c, &settings());
        assert_eq!(c, before);
    }

    #[test]
    fn brightness_one_doubles_linear() {
        let mut c = Cube {
            size: 4,
            faces: std::array::from_fn(|k| Rgba32F::new(4, 4, [0.1 * k as f32, 0.2, 0.3, 0.5])),
        };
        tone(&mut c, &Settings { brightness: 1.0, ..settings() });
        for k in 0..6 {
            for p in &c.faces[k].px {
                assert_eq!(*p, [0.2 * k as f32, 0.4, 0.6, 0.5], "face {k}");
            }
        }
    }

    #[test]
    fn rolloff_keeps_a_bright_sun_finite_and_at_most_white() {
        let s = Settings { rolloff: Some(4.0), ..settings() };
        let mut c = flat([1000.0, 900.0, 800.0]);
        tone(&mut c, &s);
        for p in c.faces.iter().flat_map(|f| &f.px) {
            assert!(p[..3].iter().all(|v| v.is_finite()), "{p:?}");
            assert!(luma(*p) <= 1.0 + 1e-5, "{p:?}");
        }
        // Tessalith's highlight_clamp_maps_white_point_to_one.
        let mut c = flat([4.0, 4.0, 4.0]);
        tone(&mut c, &s);
        assert!((c.faces[0].px[0][0] - 1.0).abs() < 1e-5);
        let mut c = flat([0.1, 0.1, 0.1]);
        tone(&mut c, &s);
        let v = c.faces[0].px[0][0];
        assert!(v < 0.1 && v > 0.08, "{v}");
    }

    #[test]
    fn a_sun_at_the_loaders_cap_stays_white_through_every_tone_step() {
        // What `open::finite` makes of an infinite texel, at the hardest settings for it.
        let sun = crate::cube::open::SUN;
        let s = Settings { brightness: 4.0, rolloff: Some(0.5), colour: 0.5, tint: [200, 255, 255], ..settings() };
        let mut c = flat([sun, sun, sun]);
        tone(&mut c, &s);
        let p = c.faces[0].px[0];
        assert!(p[..3].iter().all(|v| v.is_finite()), "{p:?}");
        assert!(p[1] > 0.99 && p[1] <= 1.0 + 1e-5, "{p:?}");
    }

    #[test]
    fn colour_strength_zero_is_grey_and_stays_positive() {
        let mut c = flat([0.8, 0.2, 0.1]);
        tone(&mut c, &Settings { colour: 0.0, ..settings() });
        let p = c.faces[0].px[0];
        assert!((p[0] - p[1]).abs() < 1e-6 && (p[1] - p[2]).abs() < 1e-6, "{p:?}");
        let mut c = flat([1.0, 0.0, 0.0]);
        tone(&mut c, &Settings { colour: 2.0, ..settings() });
        let p = c.faces[0].px[0];
        assert!(p[..3].iter().all(|&v| v >= 0.0), "{p:?}");
        assert!(p[0] > 1.0, "{p:?}");
    }

    #[test]
    fn tint_matches_a_game_gamma_multiply() {
        let mut c = flat([0.5, 0.5, 0.5]);
        let d = linear_to_game_gamma(0.5);
        tone(&mut c, &Settings { tint: [255, 128, 255], ..settings() });
        let p = c.faces[0].px[0];
        assert!((linear_to_game_gamma(p[1]) - d * 128.0 / 255.0).abs() < 1e-5);
        assert_eq!(p[0], 0.5);
        assert_eq!(p[3], 0.7);
    }

    // The ground tests below are Tessalith's `cube::filter` tests, with `GroundFill` as `Ground`.

    #[test]
    fn ground_colour_replaces_the_lower_hemisphere() {
        let mut c = Cube { size: 16, faces: std::array::from_fn(|_| Rgba32F::new(16, 16, [0.5, 0.5, 0.5, 0.7])) };
        colour_code(&mut c, [0, 0, 0], 0.0);
        let down = dir_to_face(engine::cube::world::UP.map(|v| -v)).0;
        assert_eq!(down, 5);
        assert_eq!(c.faces[5].px[8 * 16 + 8], [0.0, 0.0, 0.0, 0.7]);
        assert_eq!(c.faces[4].px[8 * 16 + 8], [0.5, 0.5, 0.5, 0.7]);
    }

    #[test]
    fn colour_code_decodes_with_game_gamma() {
        let mut c = flat([0.5; 3]);
        colour_code(&mut c, [255, 0, 128], 0.0);
        let p = c.faces[5].px[0];
        assert!((p[0] - 1.0).abs() < 1e-6 && p[1] == 0.0);
        assert!((p[2] - game_gamma_to_linear(128.0 / 255.0)).abs() < 1e-6);
    }

    #[test]
    fn gradient_follows_the_pinned_ramp_at_any_horizon() {
        let size = 64;
        for h in [0.0, 30.0, -30.0] {
            let mut c = flat_size(size, 0.6);
            grad(&mut c, h);
            let mut checked = 0;
            for (f, i, e) in texels(size) {
                if e < h - 1.0 {
                    let t = ((h - e) / (h + 90.0)) as f32;
                    let want = 0.6 * (1.0 - 0.5 * t);
                    let got = c.faces[f].px[i][0];
                    assert!((got - want).abs() < 1e-4, "h {h} e {e}: {got} vs {want}");
                    checked += 1;
                } else if e > h + 1.0 {
                    assert_eq!(c.faces[f].px[i][0], 0.6);
                }
            }
            assert!(checked > 100);
        }
    }

    #[test]
    fn blend_weights_at_the_edges() {
        for h in [0.0, 30.0, -30.0] {
            assert_eq!(fill_weight(h - 1.0, h), 1.0);
            assert_eq!(fill_weight(h, h), 0.5);
            assert_eq!(fill_weight(h + 1.0, h), 0.0);
        }
        // Texels inside the blend zone land strictly between the original and the fill.
        let size = 64;
        let mut c = flat_size(size, 0.5);
        colour_code(&mut c, [0, 0, 0], 20.0);
        let mut seen = 0;
        for (f, i, e) in texels(size) {
            if (19.1..20.9).contains(&e) {
                let v = c.faces[f].px[i][0];
                assert!((v - 0.5 * (1.0 - fill_weight(e, 20.0))).abs() < 1e-6 && v > 0.0 && v < 0.5);
                seen += 1;
            }
        }
        assert!(seen > 0);
    }

    #[test]
    fn horizon_colour_is_the_in_band_mean() {
        let size = 64;
        let h = 10.0;
        let mut c = flat_size(size, 0.0);
        for (f, i, e) in texels(size) {
            // Distinct values above, in and below the band, so a wrong band or a whole-cube mean shows.
            c.faces[f].px[i][..3].fill(if e > h + 5.0 {
                1.0
            } else if e >= h {
                0.2
            } else {
                0.7
            });
        }
        grad(&mut c, h);
        let mut checked = 0;
        for (f, i, e) in texels(size) {
            if e < h - 1.0 {
                let t = ((h - e) / (h + 90.0)) as f32;
                let got = c.faces[f].px[i][0];
                assert!((got - 0.2 * (1.0 - 0.5 * t)).abs() < 1e-4, "e {e}: {got}");
                checked += 1;
            }
        }
        assert!(checked > 100);
    }

    #[test]
    fn empty_band_uses_the_nearest_row_above() {
        // A 2x2 cube has texels only at e = +-24.1 and +-54.7 degrees, so [0, 5] is empty.
        let mut c = flat_size(2, 0.0);
        let mut rows = 0;
        for (f, i, e) in texels(2) {
            assert!(!(0.0..=5.0).contains(&e));
            c.faces[f].px[i][..3].fill(if e > 30.0 {
                0.2
            } else if e > 0.0 {
                rows += 1;
                0.8
            } else {
                0.5
            });
        }
        assert!(rows > 0);
        grad(&mut c, 0.0);
        for (f, i, e) in texels(2) {
            if e < -1.0 {
                let want = 0.8 * (1.0 - 0.5 * (-e / 90.0) as f32);
                let got = c.faces[f].px[i][0];
                assert!((got - want).abs() < 1e-4 && got > 0.0, "e {e}: {got} vs {want}");
            }
        }
    }

    #[test]
    fn horizon_moves_the_line() {
        let size = 32;
        let run = |h: f64| {
            let mut c = flat_size(size, 0.5);
            colour_code(&mut c, [0, 0, 0], h);
            c
        };
        let (a, b) = (run(0.0), run(20.0));
        let (f, i, _) = texels(size).find(|t| (9.0..11.0).contains(&t.2)).unwrap();
        assert_eq!(a.faces[f].px[i][0], 0.5);
        assert_eq!(b.faces[f].px[i][0], 0.0);
    }
}
