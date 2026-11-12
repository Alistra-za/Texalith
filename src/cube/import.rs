//! A source picture to six cube faces. Copied from Tessalith's `cube::layout`, `cube::import_dir`, `sample::build`
//! and `plan::face_tag` (Tessalith 3.0 removes them), without its `CubemapSpec` options.
use super::{Flip, SourceType};
use engine::cube::latlong::dir_to_uv;
use engine::cube::world::to_world;
use engine::cube::{dir_at, Cube};
use engine::image::Rgba32F;
use rayon::prelude::*;

/// (col, row) of each face's cell, +X..-Z. A horizontal cross is 4x3 cells, a vertical one 3x4.
pub const HCROSS: [(u32, u32); 6] = [(2, 1), (0, 1), (1, 0), (1, 2), (1, 1), (3, 1)];
const VCROSS: [(u32, u32); 6] = [(2, 1), (0, 1), (1, 0), (1, 2), (1, 1), (1, 3)];

/// The type an image's exact aspect ratio says it has. A cross whose unused cells are not flat is a picture, not a
/// cross, and gives `None`, as do square images and every other ratio.
pub fn detect(img: &Rgba32F) -> Option<SourceType> {
    let (w, h) = (u64::from(img.width), u64::from(img.height));
    let t = if w == 0 || h == 0 {
        return None;
    } else if w * 3 == h * 4 {
        SourceType::HCross
    } else if w * 4 == h * 3 {
        SourceType::VCross
    } else if w == 6 * h {
        SourceType::HStrip
    } else if h == 6 * w {
        SourceType::VStrip
    } else if w == 2 * h {
        SourceType::Panorama
    } else {
        return None;
    };
    if matches!(t, SourceType::HCross | SourceType::VCross) {
        let (n, at) = cells(t, img.width, img.height)?;
        let (cols, rows) = if t == SourceType::HCross { (4, 3) } else { (3, 4) };
        let unused = (0..rows).flat_map(|r| (0..cols).map(move |c| (c, r)));
        if unused.filter(|cell| !at.contains(cell)).any(|(c, r)| !flat(img, c * n, r * n, n)) {
            return None;
        }
    }
    Some(t)
}

/// Cell edge and each face's cell for a `w` x `h` cross or strip; `None` for other types or when the size is not
/// exactly that layout.
fn cells(t: SourceType, w: u32, h: u32) -> Option<(u32, [(u32, u32); 6])> {
    let (w6, h6) = (u64::from(w), u64::from(h));
    if w == 0 || h == 0 {
        return None;
    }
    let exact = match t {
        SourceType::HCross => w6 * 3 == h6 * 4,
        SourceType::VCross => w6 * 4 == h6 * 3,
        SourceType::HStrip => w6 == 6 * h6,
        SourceType::VStrip => h6 == 6 * w6,
        _ => false,
    };
    if !exact {
        return None;
    }
    Some(match t {
        SourceType::HCross => (w / 4, HCROSS),
        SourceType::VCross => (w / 3, VCROSS),
        SourceType::HStrip => (h, [0, 1, 2, 3, 4, 5].map(|k| (k, 0))),
        _ => (w, [0, 1, 2, 3, 4, 5].map(|k| (0, k))),
    })
}

/// Whether a `w` x `h` image can be read as `t`: crosses and strips need exact cells, the direction-based types take
/// any non-empty image, and six face files are never one image.
pub fn fits(t: SourceType, w: u32, h: u32) -> bool {
    match t {
        SourceType::HCross | SourceType::VCross | SourceType::HStrip | SourceType::VStrip => {
            cells(t, w, h).is_some()
        }
        SourceType::Faces => false,
        _ => w > 0 && h > 0,
    }
}

/// The source's cell edge: a cross or strip cell, a quarter of a panorama, half of a photo, mirror ball or angular
/// map, or a face file's edge.
pub fn cell(t: SourceType, w: u32, h: u32) -> u32 {
    match t {
        SourceType::HCross | SourceType::Panorama => w / 4,
        SourceType::VCross => w / 3,
        SourceType::HStrip => h,
        SourceType::VStrip | SourceType::Faces => w,
        SourceType::MirrorBall | SourceType::Angular | SourceType::Photo => w / 2,
    }
}

/// A `n` cube from one image read as `t`. A vertical cross's -Z cell is turned 180 degrees when `turn_bottom`. Six
/// face files go through `faces`.
pub fn import(img: &Rgba32F, t: SourceType, n: u32, turn_bottom: bool) -> Cube {
    match t {
        SourceType::Panorama => project(img, n, true, |d| dir_to_uv(d, 0.0)),
        SourceType::Photo => import(&photo_panorama(img), SourceType::Panorama, n, false),
        SourceType::MirrorBall => mirror_ball(img, n),
        SourceType::Angular => angular(img, n),
        SourceType::Faces => unreachable!("six face files go through faces()"),
        _ => {
            let (c, at) = cells(t, img.width, img.height).expect("the tab checks fits() first");
            Cube {
                size: n,
                faces: std::array::from_fn(|k| {
                    let mut face = crop(img, at[k].0 * c, at[k].1 * c, c);
                    if t == SourceType::VCross && k == 5 && turn_bottom {
                        face = rotate(&face, 180);
                    }
                    crate::resample::resample(face, (n, n))
                }),
            }
        }
    }
}

pub(crate) const UNEQUAL: &str = "The six face files are not equal squares.";

/// A `n` cube from six face files, +X -X +Y -Y +Z -Z. Borrowed, so only one face is copied at a time.
pub fn faces(imgs: [&Rgba32F; 6], n: u32) -> Result<Cube, String> {
    let e = imgs[0].width;
    if imgs.iter().any(|f| f.width != e || f.height != e) {
        return Err(UNEQUAL.into());
    }
    Ok(Cube {
        size: n,
        faces: imgs.map(|f| crate::resample::resample(f.clone(), (n, n))),
    })
}

/// A trailing face tag (`_posx` .. `_negz`, `_px` .. `_nz`, ASCII case-insensitive): the stem without it and the
/// face index (+X -X +Y -Y +Z -Z).
pub fn face_tag(stem: &str) -> Option<(&str, usize)> {
    const TAGS: [(&str, &str); 6] = [
        ("_posx", "_px"),
        ("_negx", "_nx"),
        ("_posy", "_py"),
        ("_negy", "_ny"),
        ("_posz", "_pz"),
        ("_negz", "_nz"),
    ];
    let lower = stem.to_ascii_lowercase();
    TAGS.iter().enumerate().find_map(|(k, (long, short))| {
        [long, short]
            .into_iter()
            .find(|t| lower.len() > t.len() && lower.ends_with(*t))
            .map(|t| (&stem[..stem.len() - t.len()], k))
    })
}

/// A `size` cube whose every texel is `f(face, x, y)`, rows in parallel.
pub(crate) fn build(size: u32, f: impl Fn(usize, u32, u32) -> [f32; 4] + Sync) -> Cube {
    Cube {
        size,
        faces: std::array::from_fn(|face| {
            let mut img = Rgba32F::new(size, size, [0.0; 4]);
            img.px.par_chunks_mut(size as usize).enumerate().for_each(|(y, row)| {
                for (x, p) in row.iter_mut().enumerate() {
                    *p = f(face, x as u32, y as u32);
                }
            });
            img
        }),
    }
}

fn crop(img: &Rgba32F, x0: u32, y0: u32, n: u32) -> Rgba32F {
    let mut out = Rgba32F::new(n, n, [0.0; 4]);
    for y in 0..n {
        for x in 0..n {
            *out.at_mut(x, y) = img.at(x0 + x, y0 + y);
        }
    }
    out
}

/// Each face turned clockwise by `turn[k]` degrees, then flipped by `flip[k]`; faces at the defaults are untouched.
/// Tessalith's `cube::layout::orient`.
pub fn fix(c: &mut Cube, turn: &[u16; 6], flip: &[Flip; 6]) {
    for (k, face) in c.faces.iter_mut().enumerate() {
        if turn[k] != 0 {
            *face = rotate(face, turn[k]);
        }
        if flip[k] != Flip::None {
            *face = flip_face(face, flip[k]);
        }
    }
}

// Copied from Tessalith `crates/engine/src/cube/layout.rs` (`rotate`).
/// `src` turned clockwise by `deg` (0, 90, 180, 270).
fn rotate(src: &Rgba32F, deg: u16) -> Rgba32F {
    let n = src.width;
    let mut out = Rgba32F::new(n, n, [0.0; 4]);
    for y in 0..n {
        for x in 0..n {
            let (sx, sy) = match deg {
                90 => (y, n - 1 - x),
                180 => (n - 1 - x, n - 1 - y),
                270 => (n - 1 - y, x),
                _ => (x, y),
            };
            *out.at_mut(x, y) = src.at(sx, sy);
        }
    }
    out
}

// Copied from Tessalith `crates/engine/src/cube/layout.rs` (`flip`), `FaceFlip::H`/`V` as `LeftRight`/`TopBottom`.
fn flip_face(src: &Rgba32F, how: Flip) -> Rgba32F {
    let n = src.width;
    let mut out = src.clone();
    for y in 0..n {
        for x in 0..n {
            let (sx, sy) = match how {
                Flip::LeftRight => (n - 1 - x, y),
                Flip::TopBottom => (x, n - 1 - y),
                Flip::None => (x, y),
            };
            *out.at_mut(x, y) = src.at(sx, sy);
        }
    }
    out
}

/// True when every channel of the `n` x `n` cell at (`x0`, `y0`) varies by at most 2/255 (an unused cross corner).
fn flat(img: &Rgba32F, x0: u32, y0: u32, n: u32) -> bool {
    let (mut lo, mut hi) = ([f32::MAX; 4], [f32::MIN; 4]);
    for y in y0..y0 + n {
        for x in x0..x0 + n {
            for (c, v) in img.at(x, y).into_iter().enumerate() {
                lo[c] = lo[c].min(v);
                hi[c] = hi[c].max(v);
            }
        }
    }
    (0..4).all(|c| hi[c] - lo[c] <= 2.0 / 255.0)
}

/// Bilinear sample of `img` at `(u, v)` in 0..1: `u` wraps when `wrap` (the 360-degree seam), else clamps; `v` clamps.
fn bilinear(img: &Rgba32F, u: f32, v: f32, wrap: bool) -> [f32; 4] {
    let (w, h) = (img.width as i64, img.height as i64);
    let (fx, fy) = (u * w as f32 - 0.5, v * h as f32 - 0.5);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (wx, wy) = (fx - x0, fy - y0);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let at = |x: i64, y: i64| {
        let x = if wrap { x.rem_euclid(w) } else { x.clamp(0, w - 1) };
        img.at(x as u32, y.clamp(0, h - 1) as u32)
    };
    let (a, b, c, d) = (at(x0, y0), at(x0 + 1, y0), at(x0, y0 + 1), at(x0 + 1, y0 + 1));
    std::array::from_fn(|k| (a[k] * (1.0 - wx) + b[k] * wx) * (1.0 - wy) + (c[k] * (1.0 - wx) + d[k] * wx) * wy)
}

/// The fake 2:1 panorama of a single photo: `W = 2 * img.w`, `H = W / 2`. The photo fills the front half (centred on
/// `u = 0.5` and on the horizon row), the back half is it mirrored about `u = 0.25` and `u = 0.75` so the wrap seam is
/// continuous, and above and below it each column fades linearly to the mean of the photo's top and bottom row.
fn photo_panorama(img: &Rgba32F) -> Rgba32F {
    let (w, h) = (img.width as i64, img.height as i64);
    let (ww, hh) = (2 * w, w);
    let (fx, top) = ((ww - w) / 2, (hh - h) / 2);
    let mean = |row: i64| {
        let mut m = [0.0f32; 4];
        for x in 0..w {
            let p = img.at(x as u32, row as u32);
            for k in 0..4 {
                m[k] += p[k] / w as f32;
            }
        }
        m
    };
    let (pole_n, pole_s) = (mean(0), mean(h - 1));
    let lerp = |a: [f32; 4], b: [f32; 4], t: f32| -> [f32; 4] { std::array::from_fn(|k| a[k] * (1.0 - t) + b[k] * t) };
    let mut out = Rgba32F::new(ww as u32, hh as u32, [0.0; 4]);
    for x in 0..ww {
        let c = if x < fx {
            fx - 1 - x
        } else if x < fx + w {
            x - fx
        } else {
            2 * w - 1 - x + fx
        } as u32;
        let (edge_n, edge_s) = (img.at(c, 0), img.at(c, (h - 1) as u32));
        for y in 0..hh {
            let r = y - top;
            *out.at_mut(x as u32, y as u32) = if r < 0 {
                lerp(pole_n, edge_n, y as f32 / top as f32)
            } else if r >= h {
                lerp(edge_s, pole_s, (y - top - h + 1) as f32 / (hh - top - h) as f32)
            } else {
                img.at(c, r as u32)
            };
        }
    }
    out
}

/// Camera-space `(east, up, south)` of a world direction: the camera stands at world south looking north.
fn camera(d: [f32; 3]) -> [f32; 3] {
    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-12);
    [d[0] / l, d[2] / l, -d[1] / l]
}

/// A mirror-ball photo: `n = normalize(c + (0,0,1))`, `(u, v) = (n.x, n.y)` in the unit disc, radius clamped to 0.98
/// so the back pole (north) reads from the outer ring.
fn mirror_ball(img: &Rgba32F, size: u32) -> Cube {
    project(img, size, false, |d| {
        let c = camera(d);
        let n = [c[0], c[1], c[2] + 1.0];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let m = (n[0] * n[0] + n[1] * n[1]).sqrt();
        if len < 1e-6 {
            // Exactly north: no unique ring point; take image right.
            return (0.98, 0.0);
        }
        if m < 1e-9 {
            return (0.0, 0.0);
        }
        let k = (m / len).min(0.98) / m;
        (n[0] * k, n[1] * k)
    })
}

/// An angular map: `r = acos(-c.z) / pi` along `(c.x, c.y)`, centre north, rim south.
fn angular(img: &Rgba32F, size: u32) -> Cube {
    project(img, size, false, |d| {
        let c = camera(d);
        let r = (-c[2]).clamp(-1.0, 1.0).acos() / std::f32::consts::PI;
        let m = (c[0] * c[0] + c[1] * c[1]).sqrt();
        if m < 1e-9 {
            (r, 0.0)
        } else {
            (r * c[0] / m, r * c[1] / m)
        }
    })
}

/// Each cube texel is the mean of 3x3 bilinear samples over its footprint; `map` takes a world direction to the
/// source's `(u, v)` (panorama: 0..1; disc layouts: -1..1 with v up, mapped to the image here).
fn project(img: &Rgba32F, size: u32, panorama: bool, map: impl Fn([f32; 3]) -> (f32, f32) + Sync) -> Cube {
    // shortcut: fixed 3x3 taps alias when the face is far below the source cell (a large source to a small cube);
    // scale the tap count with cell/size when that matters.
    const K: u32 = 3;
    build(size, |face, x, y| {
        let mut sum = [0.0f32; 4];
        for j in 0..K {
            for i in 0..K {
                let s = (x as f32 + (i as f32 + 0.5) / K as f32) / size as f32;
                let t = (y as f32 + (j as f32 + 0.5) / K as f32) / size as f32;
                let (u, v) = map(to_world(dir_at(face, s, t)));
                let (u, v) = if panorama { (u, v) } else { ((u + 1.0) / 2.0, (1.0 - v) / 2.0) };
                let p = bilinear(img, u, v, panorama);
                for k in 0..4 {
                    sum[k] += p[k];
                }
            }
        }
        sum.map(|v| v / (K * K) as f32)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::SourceType;
    use engine::cube::sample::fetch;
    use engine::cube::world::to_cube;
    use engine::cube::Cube;
    use engine::image::Rgba32F;

    const EAST: [f32; 3] = [1.0, 0.0, 0.0];
    const WEST: [f32; 3] = [-1.0, 0.0, 0.0];
    const NORTH: [f32; 3] = [0.0, 1.0, 0.0];
    const SOUTH: [f32; 3] = [0.0, -1.0, 0.0];
    const UPW: [f32; 3] = [0.0, 0.0, 1.0];
    const DOWN: [f32; 3] = [0.0, 0.0, -1.0];
    const GREY: [f32; 4] = [0.5, 0.5, 0.5, 1.0];

    /// Six `n` x `n` faces, each its own colour, with a white band on the top rows and a black band on the left columns.
    fn arrow_faces(n: u32) -> [Rgba32F; 6] {
        let colours: [[f32; 3]; 6] = [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [1.0, 1.0, 0.0],
        ];
        colours.map(|c| {
            let mut f = Rgba32F::new(n, n, [c[0], c[1], c[2], 1.0]);
            for y in 0..n {
                for x in 0..n {
                    if y < n / 8 {
                        *f.at_mut(x, y) = [1.0; 4];
                    } else if x < n / 8 {
                        *f.at_mut(x, y) = [0.0, 0.0, 0.0, 1.0];
                    }
                }
            }
            f
        })
    }

    fn turn180(f: &Rgba32F) -> Rgba32F {
        let n = f.width;
        let mut o = f.clone();
        for y in 0..n {
            for x in 0..n {
                *o.at_mut(x, y) = f.at(n - 1 - x, n - 1 - y);
            }
        }
        o
    }

    /// The faces pasted at literal cell offsets (not `HCROSS`, so a wrong table cannot agree with itself).
    fn layout(faces: &[Rgba32F; 6], cols: u32, rows: u32, at: [(u32, u32); 6], negz_turned: bool) -> Rgba32F {
        let n = faces[0].width;
        let mut img = Rgba32F::new(cols * n, rows * n, GREY);
        for (k, (f, (c, r))) in faces.iter().zip(at).enumerate() {
            let f = if k == 5 && negz_turned { turn180(f) } else { f.clone() };
            for y in 0..n {
                for x in 0..n {
                    *img.at_mut(c * n + x, r * n + y) = f.at(x, y);
                }
            }
        }
        img
    }

    fn hcross(f: &[Rgba32F; 6]) -> Rgba32F {
        layout(f, 4, 3, [(2, 1), (0, 1), (1, 0), (1, 2), (1, 1), (3, 1)], false)
    }
    fn vcross(f: &[Rgba32F; 6]) -> Rgba32F {
        layout(f, 3, 4, [(2, 1), (0, 1), (1, 0), (1, 2), (1, 1), (1, 3)], true)
    }
    fn hstrip(f: &[Rgba32F; 6]) -> Rgba32F {
        layout(f, 6, 1, [0, 1, 2, 3, 4, 5].map(|k| (k, 0)), false)
    }
    fn vstrip(f: &[Rgba32F; 6]) -> Rgba32F {
        layout(f, 1, 6, [0, 1, 2, 3, 4, 5].map(|k| (0, k)), false)
    }

    fn sized(w: u32, h: u32) -> Rgba32F {
        Rgba32F::new(w, h, GREY)
    }

    /// An image whose red is x/w and green is y/h, so a sample says where in the image it came from.
    fn coord_image(w: u32, h: u32) -> Rgba32F {
        let mut img = Rgba32F::new(w, h, [0.0; 4]);
        for y in 0..h {
            for x in 0..w {
                *img.at_mut(x, y) = [
                    (x as f32 + 0.5) / w as f32,
                    (y as f32 + 0.5) / h as f32,
                    0.0,
                    1.0,
                ];
            }
        }
        img
    }

    /// Where in the unit disc (u right, v up) the cube's colour toward world `d` was read from.
    fn disc_at(c: &Cube, d: [f32; 3]) -> (f32, f32) {
        let p = fetch(c, to_cube(d));
        assert!(p.iter().all(|v| v.is_finite()), "{d:?} -> {p:?}");
        (2.0 * p[0] - 1.0, 1.0 - 2.0 * p[1])
    }

    fn close(got: (f32, f32), want: (f32, f32)) {
        assert!(
            (got.0 - want.0).abs() < 0.03 && (got.1 - want.1).abs() < 0.03,
            "{got:?} vs {want:?}"
        );
    }

    #[test]
    fn detect_takes_exact_ratios_only() {
        let faces = arrow_faces(16);
        assert_eq!(detect(&hcross(&faces)), Some(SourceType::HCross));
        assert_eq!(detect(&vcross(&faces)), Some(SourceType::VCross));
        assert_eq!(detect(&sized(1536, 256)), Some(SourceType::HStrip));
        assert_eq!(detect(&sized(256, 1536)), Some(SourceType::VStrip));
        assert_eq!(detect(&sized(2048, 1024)), Some(SourceType::Panorama));
        for (w, h) in [(512, 512), (1024, 2048), (4096, 2049), (1025, 768), (6145, 1024)] {
            assert_eq!(detect(&sized(w, h)), None, "{w}x{h}");
        }
    }

    #[test]
    fn every_cross_and_strip_round_trips_the_arrow_cube() {
        let faces = arrow_faces(64);
        for (t, img) in [
            (SourceType::HCross, hcross(&faces)),
            (SourceType::VCross, vcross(&faces)),
            (SourceType::HStrip, hstrip(&faces)),
            (SourceType::VStrip, vstrip(&faces)),
        ] {
            assert!(fits(t, img.width, img.height), "{t:?}");
            assert_eq!(cell(t, img.width, img.height), 64, "{t:?}");
            let c = import(&img, t, 64, true);
            assert_eq!(c.size, 64);
            assert!(c.faces == faces, "{t:?}");
        }
    }

    #[test]
    fn vcross_negz_is_turned_upright() {
        let faces = arrow_faces(32);
        // `vcross` stores -Z turned 180 degrees, as most tools write it.
        let img = vcross(&faces);
        assert!(img.at(32 + 5, 3 * 32 + 31 - 1) == [1.0; 4]); // white band at the bottom of the stored cell
        let c = import(&img, SourceType::VCross, 32, true);
        assert!(c.faces[5] == faces[5]);
    }

    #[test]
    fn a_photo_shaped_like_a_cross_is_not_detected() {
        let (w, h) = (64, 48);
        let mut seed = 12345u32;
        let mut img = Rgba32F::new(w, h, [0.0; 4]);
        for p in &mut img.px {
            for c in &mut p[..3] {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                *c = (seed >> 24) as f32 / 255.0;
            }
            p[3] = 1.0;
        }
        assert_eq!(detect(&img), None);
        // Picked by hand it still reads as a cross.
        assert!(fits(SourceType::HCross, w, h));
    }

    #[test]
    fn panorama_mapping_matches_the_in_game_check() {
        // u 0 west, 0.25 north, 0.5 east, 0.75 south (clockwise seen from above), row 0 up.
        let c = import(&coord_image(256, 128), SourceType::Panorama, 32, true);
        let u = |d: [f32; 3]| fetch(&c, to_cube(d))[0];
        let v = |d: [f32; 3]| fetch(&c, to_cube(d))[1];
        assert!((u(NORTH) - 0.25).abs() < 0.02, "{}", u(NORTH));
        assert!((u(EAST) - 0.5).abs() < 0.02, "{}", u(EAST));
        assert!((u(SOUTH) - 0.75).abs() < 0.02, "{}", u(SOUTH));
        // Just north of west: u a little above 0 (exactly west sits on the wrap seam).
        let l = (1.0f32 + 0.1 * 0.1).sqrt();
        assert!(u([-1.0 / l, 0.1 / l, 0.0]) < 0.04, "{}", u([-1.0 / l, 0.1 / l, 0.0]));
        assert!(v(UPW) < 0.03 && v(DOWN) > 0.97, "{} {}", v(UPW), v(DOWN));
        assert!((v(WEST) - 0.5).abs() < 0.02);
    }

    #[test]
    fn mirror_ball_centre_is_behind_the_camera() {
        let c = import(&coord_image(256, 256), SourceType::MirrorBall, 64, true);
        close(disc_at(&c, SOUTH), (0.0, 0.0));
        close(disc_at(&c, UPW), (0.0, 0.707));
        close(disc_at(&c, DOWN), (0.0, -0.707));
        close(disc_at(&c, EAST), (0.707, 0.0));
        // North is the rim, clamped to 0.98.
        close(disc_at(&c, [0.3, 0.954, 0.0]), (0.98, 0.0));
        let c = import(&coord_image(256, 256), SourceType::MirrorBall, 8, true);
        assert!(c.faces.iter().all(|f| f.px.iter().all(|p| p[0].is_finite())));
    }

    #[test]
    fn angular_centre_is_forward() {
        let c = import(&coord_image(256, 256), SourceType::Angular, 64, true);
        close(disc_at(&c, NORTH), (0.0, 0.0));
        close(disc_at(&c, EAST), (0.5, 0.0));
        close(disc_at(&c, UPW), (0.0, 0.5));
        close(disc_at(&c, WEST), (-0.5, 0.0));
        // Just east of north: acos(0.954) / pi = 0.098 along +x.
        close(disc_at(&c, [0.3, 0.954, 0.0]), (0.098, 0.0));
    }

    #[test]
    fn photo_panorama_is_seamless_and_mirrored() {
        let (w, h) = (64u32, 32u32);
        let mut img = Rgba32F::new(w, h, [0.0; 4]);
        for y in 0..h {
            for x in 0..w {
                *img.at_mut(x, y) = [x as f32 * 0.003, 0.2 + y as f32 * 0.002, 0.5, 1.0];
            }
        }
        let p = photo_panorama(&img);
        let ww = 2 * w;
        assert_eq!((p.width, p.height), (ww, ww / 2));
        for y in 0..p.height {
            let (a, b) = (p.at(0, y), p.at(ww - 1, y));
            for k in 0..4 {
                assert!((a[k] - b[k]).abs() < 1.0 / 255.0, "seam row {y}");
            }
            for k in 0..ww / 4 {
                assert_eq!(p.at(ww / 4 - 1 - k, y), p.at(ww / 4 + k, y), "left {k}");
                assert_eq!(p.at(3 * ww / 4 + k, y), p.at(3 * ww / 4 - 1 - k, y), "right {k}");
            }
        }
        // The front half is the photo, centred on the horizon row.
        let top = (p.height - h) / 2;
        assert_eq!(p.at(ww / 4 + 5, top + 7), img.at(5, 7));
        // And a Photo source imports through it with no seam at the back.
        assert_eq!(cell(SourceType::Photo, w, h), w / 2);
        let c = import(&img, SourceType::Photo, 16, true);
        assert!(c.faces.iter().all(|f| f.px.iter().all(|p| p[0].is_finite())));
    }

    #[test]
    fn face_tags_parse_both_spellings() {
        assert_eq!(face_tag("chrome_e_posx"), Some(("chrome_e", 0)));
        assert_eq!(face_tag("chrome_e_NZ"), Some(("chrome_e", 5)));
        assert_eq!(face_tag("sky_NegY"), Some(("sky", 3)));
        assert_eq!(face_tag("sky_pz"), Some(("sky", 4)));
        assert_eq!(face_tag("chrome_e"), None);
        assert_eq!(face_tag("_px"), None);
    }

    #[test]
    fn faces_reads_the_opened_images_in_place() {
        // Borrowed, so the six (up to 8192-square) images are never copied as a set before resampling.
        let imgs = arrow_faces(16);
        let c = faces(std::array::from_fn(|k| &imgs[k]), 8).unwrap();
        assert_eq!((c.size, c.faces[0].width), (8, 8));
    }

    #[test]
    fn a_face_fix_turns_then_flips_one_face() {
        // Face 0 holds a ramp; turn 90 clockwise then flip left to right is a transpose.
        let n = 3u32;
        let mut c = Cube { size: n, faces: std::array::from_fn(|_| Rgba32F::new(n, n, [0.0; 4])) };
        for (i, p) in c.faces[0].px.iter_mut().enumerate() {
            *p = [i as f32, 0.0, 0.0, 1.0];
        }
        let before = c.clone();
        fix(&mut c, &[90, 0, 0, 0, 0, 0], &[Flip::LeftRight, Flip::None, Flip::None, Flip::None, Flip::None, Flip::None]);
        for y in 0..n {
            for x in 0..n {
                assert_eq!(c.faces[0].at(x, y), before.faces[0].at(y, x), "({x},{y})");
            }
        }
        assert!(c.faces[1..] == before.faces[1..]);
    }

    #[test]
    fn turn_bottom_off_keeps_the_stored_bottom_cell() {
        // A vertical cross whose -Z cell (col 1, row 3) is a ramp.
        let n = 4u32;
        let mut img = Rgba32F::new(3 * n, 4 * n, [0.0; 4]);
        for y in 0..n {
            for x in 0..n {
                *img.at_mut(n + x, 3 * n + y) = [(y * n + x) as f32, 0.0, 0.0, 1.0];
            }
        }
        let kept = import(&img, SourceType::VCross, n, false);
        let turned = import(&img, SourceType::VCross, n, true);
        assert_eq!(kept.faces[5].at(0, 0)[0], 0.0);
        assert_eq!(turned.faces[5].at(0, 0)[0], (n * n - 1) as f32);
    }

    #[test]
    fn six_unequal_faces_are_refused() {
        let mut imgs = arrow_faces(16);
        imgs[3] = arrow_faces(8)[3].clone();
        assert_eq!(faces(std::array::from_fn(|k| &imgs[k]), 16).unwrap_err(), "The six face files are not equal squares.");
        let good = arrow_faces(16);
        let ok = faces(std::array::from_fn(|k| &good[k]), 16).unwrap();
        assert!(ok.faces == arrow_faces(16));
        assert!(!fits(SourceType::Faces, 16, 16));
    }
}
