//! RNM blend math. `decode`, `rnm`, `bump_normals`, `bent_weight` and `unit` are copied from Tessalith's
//! `crates/engine/src/rnm/blend.rs` (GPL-3.0); `apply` takes images already in memory instead of loading files.
//! Alpha is never written, so a `_cnr`'s coat roughness passes through.
use super::{Input, Kind};
use crate::resample::resample;
use engine::image::Rgba32F;
use rayon::prelude::*;
use std::borrow::Cow;

type V3 = [f32; 3];

/// Tessalith's degenerate-normal rule: divide when the length is above 1e-8, otherwise straight up.
fn unit(v: V3) -> V3 {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 1e-8 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// A stored texel (0..1) as a tangent normal at strength `s`: `t = (2c − 1)·s`, `n = unit(t.x, t.y, |t.z| + 1 − s)`
/// with z clamped at 0; `flip` negates Y.
pub fn decode(c: [f32; 4], s: f32, flip: bool) -> [f32; 3] {
    let y = (2.0 * c[1] - 1.0) * s;
    let t = [(2.0 * c[0] - 1.0) * s, if flip { -y } else { y }, (2.0 * c[2] - 1.0) * s];
    unit([t[0], t[1], (t[2].abs() + 1.0 - s).max(0.0)])
}

/// Reoriented Normal Mapping (Barre-Brisebois & Hill): `detail` reoriented onto `base`.
pub fn rnm(base: [f32; 3], detail: [f32; 3]) -> [f32; 3] {
    let t = [base[0], base[1], base[2] + 1.0];
    let u = [-detail[0], -detail[1], detail[2]];
    let k = (t[0] * u[0] + t[1] * u[1] + t[2] * u[2]) / t[2].max(1e-6);
    unit([t[0] * k - u[0], t[1] * k - u[1], t[2] * k - u[2]])
}

/// Bump height (mean RGB) to DirectX normals at the bump's own size: slope `k·(h[+1] − h[−1])`, +Y up the image.
pub fn bump_normals(img: &Rgba32F, k: f32) -> Rgba32F {
    let (w, h) = (img.width as i64, img.height as i64);
    let hgt = |x: i64, y: i64| {
        let p = img.at(x.clamp(0, w - 1) as u32, y.clamp(0, h - 1) as u32);
        (p[0] + p[1] + p[2]) / 3.0
    };
    let mut out = Rgba32F::new(img.width, img.height, [0.0, 0.0, 1.0, 1.0]);
    for y in 0..h {
        for x in 0..w {
            let du = hgt(x + 1, y) - hgt(x - 1, y);
            let dv = hgt(x, y - 1) - hgt(x, y + 1);
            let n = unit([-k * du, k * dv, 1.0]);
            *out.at_mut(x as u32, y as u32) = [n[0], n[1], n[2], 1.0];
        }
    }
    out
}

/// rnm.py's bent weight: `strength · (1 − ao)^power · mask`.
fn bent_weight(strength: f32, power: f32, ao: f32, mask: f32) -> f32 {
    strength * (1.0 - ao.clamp(0.0, 1.0)).powf(power) * mask
}

/// A stored base texel as a unit vector: `x = 2r − 1`, `y = 2g − 1`, z rebuilt, so a two-channel base works.
pub fn decode_base(img: &Rgba32F) -> Rgba32F {
    let px = img
        .px
        .iter()
        .map(|p| {
            let (x, y) = (2.0 * p[0] - 1.0, 2.0 * p[1] - 1.0);
            [x, y, (1.0 - x * x - y * y).max(0.0).sqrt(), p[3]]
        })
        .collect();
    Rgba32F { width: img.width, height: img.height, px }
}

/// Unit vectors back to stored texels, `(v + 1) / 2`, alpha kept.
pub fn encode(work: &Rgba32F) -> Vec<[f32; 4]> {
    work.px.iter().map(|p| [(p[0] + 1.0) / 2.0, (p[1] + 1.0) / 2.0, (p[2] + 1.0) / 2.0, p[3]]).collect()
}

fn sized(img: &Rgba32F, size: (u32, u32)) -> Cow<'_, Rgba32F> {
    if (img.width, img.height) == size {
        Cow::Borrowed(img)
    } else {
        Cow::Owned(resample(img.clone(), size))
    }
}

/// One channel of `img` at `size`, clamped to 0..1.
fn plane(img: &Rgba32F, size: (u32, u32), channel: usize) -> Vec<f32> {
    sized(img, size).px.iter().map(|p| p[channel].clamp(0.0, 1.0)).collect()
}

fn put(px: &mut [f32; 4], v: V3) {
    px[..3].copy_from_slice(&v);
}

/// Blend `layers` into `work` (unit vectors, as `decode_base` gives), first to last.
pub fn apply(work: &mut Rgba32F, layers: &[Input]) {
    let size = (work.width, work.height);
    work.px.par_iter_mut().for_each(|px| put(px, unit([px[0], px[1], px[2]])));
    for l in layers {
        let (k, flip) = (l.set.strength, l.set.flip);
        let mask = l.mask.as_ref().map(|m| plane(m, size, 0));
        let m = |i: usize| mask.as_ref().map_or(1.0, |v| v[i]);
        match l.set.kind {
            Kind::Bump => {
                let n = resample(bump_normals(&l.img, k), size);
                work.px.par_iter_mut().enumerate().for_each(|(i, px)| {
                    let q = n.px[i];
                    let mut d = unit([q[0], if flip { -q[1] } else { q[1] }, q[2]]);
                    if mask.is_some() {
                        // A bump mask scales the slope exactly (rnm.py `scale_slope`).
                        let z = d[2].max(1e-6);
                        d = unit([d[0] / z * m(i), d[1] / z * m(i), 1.0]);
                    }
                    put(px, rnm([px[0], px[1], px[2]], d));
                });
            }
            Kind::Bent => {
                let src = sized(&l.img, size);
                let ao = match (&l.ao, l.set.use_ao) {
                    (Some((a, channel)), true) => Some(plane(a, size, *channel)),
                    _ => None,
                };
                work.px.par_iter_mut().enumerate().for_each(|(i, px)| {
                    let b = decode(src.px[i], 1.0, flip);
                    let w = match &ao {
                        Some(a) => bent_weight(k, l.set.ao_power, a[i], m(i)),
                        None => k * m(i),
                    };
                    put(
                        px,
                        unit([
                            px[0] * (1.0 - w) + b[0] * w,
                            px[1] * (1.0 - w) + b[1] * w,
                            px[2] * (1.0 - w) + b[2] * w,
                        ]),
                    );
                });
            }
            Kind::Detail => {
                let src = sized(&l.img, size);
                work.px.par_iter_mut().enumerate().for_each(|(i, px)| {
                    put(px, rnm([px[0], px[1], px[2]], decode(src.px[i], k * m(i), flip)));
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rnm::Layer;
    use std::sync::Arc;

    fn close(a: [f32; 3], b: [f32; 3], eps: f32) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < eps)
    }
    const FLAT: [f32; 4] = [0.5, 0.5, 1.0, 1.0];

    fn img(w: u32, h: u32, f: impl Fn(u32, u32) -> [f32; 4]) -> Arc<Rgba32F> {
        let mut out = Rgba32F::new(w, h, [0.0; 4]);
        for y in 0..h {
            for x in 0..w {
                *out.at_mut(x, y) = f(x, y);
            }
        }
        Arc::new(out)
    }

    fn solid(v: [f32; 4]) -> Arc<Rgba32F> {
        img(16, 16, |_, _| v)
    }

    /// A non-flat unit-normal base, so a blend that does nothing differs from one that did something.
    fn base(n: u32) -> Rgba32F {
        let b = img(n, n, |x, y| {
            let v = unit([0.3 * (x as f32 / 5.0).sin(), 0.25 * (y as f32 / 4.0).cos(), 1.0]);
            [v[0], v[1], v[2], 1.0]
        });
        (*b).clone()
    }

    fn textured(x: u32, y: u32) -> [f32; 4] {
        [(40 + x * 11) as f32 / 255.0, (200 - y * 9) as f32 / 255.0, (150 + x * 3) as f32 / 255.0, 1.0]
    }

    fn max_diff(a: &Rgba32F, b: &Rgba32F) -> f32 {
        a.px.iter().zip(&b.px).flat_map(|(p, q)| (0..3).map(move |i| (p[i] - q[i]).abs())).fold(0.0, f32::max)
    }

    fn run(layers: &[Input]) -> Rgba32F {
        let mut w = base(16);
        apply(&mut w, layers);
        w
    }

    fn one(kind: Kind, img: Arc<Rgba32F>) -> Vec<Input> {
        vec![Input { set: Layer::new(kind), img, mask: None, ao: None }]
    }

    /// `v` with `mask` on its last layer.
    fn masked(mut v: Vec<Input>, mask: Arc<Rgba32F>) -> Vec<Input> {
        v.last_mut().expect("a layer").mask = Some(mask);
        v
    }

    #[test]
    fn strength_above_one_never_gives_negative_z() {
        assert!(decode([0.99, 0.5, 0.6, 1.0], 1.5, false)[2] >= 0.0);
        for s in [0.5f32, 1.0, 1.5, 2.0] {
            for b in 0..=255u8 {
                let z = decode([0.99, 0.5, b as f32 / 255.0, 1.0], s, false)[2];
                assert!(z >= 0.0, "strength {s}, blue {b}: z {z}");
            }
        }
    }

    #[test]
    fn flat_detail_leaves_the_base() {
        let base = unit([0.3, -0.2, 0.9]);
        assert!(close(rnm(base, decode(FLAT, 0.6, false)), base, 1e-5));
    }

    #[test]
    fn flat_base_gives_the_detail() {
        let d = decode([0.7, 0.4, 0.9, 1.0], 1.0, false);
        assert!(close(rnm([0.0, 0.0, 1.0], d), d, 1e-5));
    }

    #[test]
    fn strength_zero_is_flat_and_flip_negates_y() {
        assert!(close(decode([0.9, 0.1, 0.6, 1.0], 0.0, false), [0.0, 0.0, 1.0], 1e-6));
        let a = decode([0.7, 0.3, 0.9, 1.0], 1.0, false);
        let b = decode([0.7, 0.3, 0.9, 1.0], 1.0, true);
        assert!(close(a, [b[0], -b[1], b[2]], 1e-6));
    }

    /// Height rising toward the image top tilts the DirectX normal to +Y; rising to the right, to -X.
    #[test]
    fn bump_ramp_tilts_dx_plus_y() {
        let up = img(8, 8, |_, y| {
            let v = (7 - y) as f32 / 7.0;
            [v, v, v, 1.0]
        });
        let right = img(8, 8, |x, _| {
            let h = x as f32 / 7.0;
            [h, h, h, 1.0]
        });
        let n = bump_normals(&up, 1.0).at(4, 4);
        assert!(n[1] > 0.05 && n[0].abs() < 1e-6, "{n:?}");
        let n = bump_normals(&right, 1.0).at(4, 4);
        assert!(n[0] < -0.05 && n[1].abs() < 1e-6, "{n:?}");
    }

    #[test]
    fn bent_weight_follows_the_occlusion() {
        for (ao, want) in [(1.0f32, 0.0f32), (0.5, 0.35 * 0.5f32.powf(1.5)), (0.0, 0.35)] {
            assert!((bent_weight(0.35, 1.5, ao, 1.0) - want).abs() < 1e-6);
        }
        assert_eq!(bent_weight(0.35, 1.5, 0.0, 0.0), 0.0);
    }

    #[test]
    fn apply_never_touches_alpha() {
        let mut work = Rgba32F::new(16, 16, [0.0, 0.0, 1.0, 1.0]);
        for (i, px) in work.px.iter_mut().enumerate() {
            px[3] = i as f32 / 255.0;
        }
        let before = work.clone();
        apply(&mut work, &one(Kind::Detail, img(16, 16, |x, y| [x as f32 / 16.0, y as f32 / 16.0, 0.8, 1.0])));
        assert!(work.px.iter().zip(&before.px).all(|(a, b)| a[3].to_bits() == b[3].to_bits()));
        assert!(work.px.iter().zip(&before.px).any(|(a, b)| a[..3] != b[..3]));
    }

    #[test]
    fn black_mask_is_no_layer_and_white_mask_is_no_mask() {
        let none = run(&[]);
        let plain = one(Kind::Detail, img(16, 16, textured));
        let black = run(&masked(plain.clone(), solid([0.0, 0.0, 0.0, 1.0])));
        assert!(max_diff(&black, &none) < 1e-4);
        let white = run(&masked(plain.clone(), solid([1.0; 4])));
        assert!(max_diff(&white, &run(&plain)) < 1e-6);
        assert!(max_diff(&white, &none) > 1e-3, "the layer must do something");
    }

    /// AO channel 0 is a raw AO map's R, channel 2 the B of a packed file (R deliberately differs from B).
    #[test]
    fn bent_ao_wiring() {
        let k = Layer::new(Kind::Bent).strength;
        let src = img(16, 16, textured);
        let none = run(&[]);
        let mut weighted = none.clone();
        for (px, c) in weighted.px.iter_mut().zip(&src.px) {
            let b = decode(*c, 1.0, false);
            put(px, unit([px[0] * (1.0 - k) + b[0] * k, px[1] * (1.0 - k) + b[1] * k, px[2] * (1.0 - k) + b[2] * k]));
        }
        let with_ao = |rgba: [f32; 4], channel: usize, use_ao: bool| {
            let mut i = one(Kind::Bent, src.clone());
            i[0].ao = Some((solid(rgba), channel));
            i[0].set.use_ao = use_ao;
            run(&i)
        };
        assert!(max_diff(&with_ao([1.0, 0.0, 0.0, 1.0], 0, true), &none) < 1e-5);
        assert!(max_diff(&with_ao([0.0, 1.0, 1.0, 1.0], 0, true), &weighted) < 1e-4);
        assert!(max_diff(&with_ao([0.0, 0.0, 1.0, 1.0], 2, true), &none) < 1e-5);
        assert!(max_diff(&with_ao([1.0, 1.0, 0.0, 1.0], 2, true), &weighted) < 1e-4);
        assert!(max_diff(&with_ao([1.0; 4], 0, false), &weighted) < 1e-4);
        assert!(max_diff(&weighted, &none) > 1e-3);
    }

    /// One 64x64 smooth bump on a 16 and a 32 base gives the same normals at the same places, within 0.03.
    #[test]
    fn bump_is_resolution_invariant() {
        let bump = img(64, 64, |x, y| {
            let v = 0.5 + 0.4 * (x as f32 * 0.3).sin() * (y as f32 * 0.2).cos();
            let b = (v * 255.0).round() / 255.0;
            [b, b, b, 1.0]
        });
        let mut i = one(Kind::Bump, bump);
        i[0].set.strength = 2.0;
        let mut a = Rgba32F::new(16, 16, [0.0, 0.0, 1.0, 1.0]);
        let mut b = Rgba32F::new(32, 32, [0.0, 0.0, 1.0, 1.0]);
        apply(&mut a, &i);
        apply(&mut b, &i);
        let mut seen = 0.0f32;
        for y in 1..15u32 {
            for x in 1..15u32 {
                let v: Vec<f32> =
                    (0..3).map(|c| (0..4).map(|k| b.at(2 * x + k % 2, 2 * y + k / 2)[c]).sum::<f32>() / 4.0).collect();
                let u = a.at(x, y);
                for c in 0..3 {
                    assert!((u[c] - v[c]).abs() < 0.03, "({x},{y}) {u:?} vs {v:?}");
                }
                seen = seen.max(u[0].abs());
            }
        }
        assert!(seen > 0.05, "the bump must tilt the normals");
    }

    #[test]
    fn apply_follows_the_order() {
        let (d1, d2) = (img(16, 16, textured), img(16, 16, |x, y| textured(15 - y, x)));
        let a = Input { set: Layer::new(Kind::Detail), img: d1.clone(), mask: None, ao: None };
        let c = Input { set: Layer { strength: 0.3, ..Layer::new(Kind::Detail) }, img: d2.clone(), mask: None, ao: None };
        let (s1, s2) = (a.set.strength, c.set.strength);
        let b = base(16);
        let fold = |first: bool| {
            let mut w = b.clone();
            for (j, px) in w.px.iter_mut().enumerate() {
                let (a, c) = (decode(d1.px[j], s1, false), decode(d2.px[j], s2, false));
                let n = [px[0], px[1], px[2]];
                put(px, if first { rnm(rnm(n, a), c) } else { rnm(rnm(n, c), a) });
            }
            w
        };
        let ab = run(&[a.clone(), c.clone()]);
        assert!(max_diff(&ab, &fold(true)) < 1e-6);
        let ba = run(&[c, a]);
        assert!(max_diff(&ba, &fold(false)) < 1e-6);
        assert!(max_diff(&ab, &ba) > 1e-3);
    }

    #[test]
    fn layers_of_any_size_are_resampled_to_the_base() {
        let wave = |a: f32| move |x: u32, y: u32| [0.5 + 0.3 * (x as f32 * a).sin(), 0.5 + 0.3 * (y as f32 * a).cos(), 0.9, 1.0];
        let mut i = one(Kind::Bent, img(16, 16, wave(0.2)));
        i[0].ao = Some((img(48, 48, |x, _| [x as f32 / 47.0; 4]), 0));
        i.extend(one(Kind::Bump, img(24, 24, wave(0.4))));
        i.extend(masked(one(Kind::Detail, img(64, 64, wave(0.7))), img(7, 5, |x, _| [x as f32 / 6.0, 0.0, 0.0, 1.0])));
        let mut w = base(32);
        apply(&mut w, &i);
        assert_eq!((w.width, w.height), (32, 32));
        for p in &w.px {
            let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-5, "{p:?}");
        }
    }

    #[test]
    fn a_mask_reads_red_only() {
        let plain = one(Kind::Detail, img(16, 16, textured));
        let with = |m: [f32; 4]| run(&masked(plain.clone(), solid(m)));
        assert_eq!(with([1.0, 0.0, 0.0, 1.0]).px, with([1.0; 4]).px);
        assert_eq!(with([0.0, 1.0, 1.0, 1.0]).px, with([0.0, 0.0, 0.0, 1.0]).px);
    }

    #[test]
    fn ao_is_ignored_off_a_bent_layer_or_with_bend_in_shadows_off() {
        let ao = (img(16, 16, |x, y| [(x * y) as f32 / 225.0, 0.0, 0.0, 1.0]), 0);
        let mut i = one(Kind::Bump, img(16, 16, textured));
        i.extend(one(Kind::Detail, img(16, 16, |x, y| textured(y, x))));
        let without = run(&i);
        i[0].ao = Some(ao.clone());
        i[1].ao = Some(ao.clone());
        assert_eq!(run(&i).px, without.px);
        let mut bent = one(Kind::Bent, img(16, 16, textured));
        bent[0].set.use_ao = false;
        let without = run(&bent);
        bent[0].ao = Some(ao);
        assert_eq!(run(&bent).px, without.px);
    }

    #[test]
    fn a_two_channel_base_gets_its_z_back() {
        let two = img(2, 1, |x, _| if x == 0 { [0.5, 0.5, 0.0, 0.3] } else { [1.0, 0.5, 0.0, 1.0] });
        let d = decode_base(&two);
        assert_eq!(d.px, vec![[0.0, 0.0, 1.0, 0.3], [1.0, 0.0, 0.0, 1.0]]);
        let n = unit([0.3, -0.2, 0.9]);
        let x = img(1, 1, |_, _| [(n[0] + 1.0) / 2.0, (n[1] + 1.0) / 2.0, (n[2] + 1.0) / 2.0, 0.7]);
        let back = encode(&decode_base(&x));
        for c in 0..3 {
            assert!((back[0][c] - x.px[0][c]).abs() < 1e-6, "{back:?}");
        }
        assert_eq!(back[0][3], 0.7);
    }

    #[test]
    fn a_new_layer_has_the_spec_defaults() {
        assert_eq!(Kind::ALL.map(|k| Layer::new(k).strength), [0.35, 0.4, 0.6]);
        assert!(Kind::ALL.iter().all(|&k| {
            let l = Layer::new(k);
            l.kind == k && !l.flip && l.use_ao && l.ao_power == 1.5
        }));
    }

    #[test]
    fn each_bent_layer_reads_its_own_ao() {
        let src = img(16, 16, textured);
        let bent = |ao: f32| Input { set: Layer::new(Kind::Bent), img: src.clone(), mask: None, ao: Some((solid([ao, 0.0, 0.0, 1.0]), 0)) };
        let open = run(&[bent(0.0)]);
        // The first layer's AO is white, so its weight is 0 and only the second bends the base.
        assert!(max_diff(&run(&[bent(1.0), bent(0.0)]), &open) < 1e-6);
        assert!(max_diff(&open, &run(&[])) > 1e-3);
    }

    #[test]
    fn seven_layers_blend_in_list_order() {
        let layers: Vec<Input> = (0..7u32)
            .map(|k| Input {
                set: Layer { strength: 0.2 + 0.1 * k as f32, ..Layer::new(Kind::Detail) },
                img: img(16, 16, move |x, y| textured((x + k) % 16, (y + 2 * k) % 16)),
                mask: None,
                ao: None,
            })
            .collect();
        let mut want = base(16);
        for (j, px) in want.px.iter_mut().enumerate() {
            let mut n = [px[0], px[1], px[2]];
            for l in &layers {
                n = rnm(n, decode(l.img.px[j], l.set.strength, false));
            }
            put(px, n);
        }
        assert!(max_diff(&run(&layers), &want) < 1e-6);
    }
}
