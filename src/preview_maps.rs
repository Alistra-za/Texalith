//! The 3D preview's maps from the From normal outputs: RGBA8 DDS files with full mip chains for the `n`, `p` and `r`
//! slots (spec: "Values to pin", Maps).
use crate::from_normal::maps::Gray;
use crate::from_normal::worker::Outputs;
use std::collections::HashMap;

/// DDS files by render3d slot name.
pub type Maps = HashMap<&'static str, Vec<u8>>;

/// The longest side of a map the 3D preview binds; a larger output is box-filtered down to it.
pub const PREVIEW_MAX: u32 = 4096;

/// A DX10 RGBA8 DDS of `w`×`h` texels with a full box-filtered mip chain; each side halves down to 1, and a texel
/// averages the source texels its 2×2 block holds.
pub fn dds_rgba8(w: u32, h: u32, base: &[[u8; 4]], srgb: bool) -> Vec<u8> {
    let mips = 32 - w.max(h).leading_zeros();
    let mut b = vec![0u8; 148];
    let mut put = |at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    put(0, u32::from_le_bytes(*b"DDS "));
    put(4, 124);
    put(8, 0x1 | 0x2 | 0x4 | 0x8 | 0x1000 | 0x20000);
    put(12, h);
    put(16, w);
    put(20, w * 4);
    put(28, mips);
    put(76, 32);
    put(80, 0x4);
    put(84, u32::from_le_bytes(*b"DX10"));
    put(108, 0x1000 | 0x8 | 0x400000);
    put(128, if srgb { 29 } else { 28 });
    put(132, 3);
    put(140, 1);
    let mut level = base.to_vec();
    let (mut lw, mut lh) = (w as usize, h as usize);
    loop {
        b.extend(level.iter().flatten());
        if lw == 1 && lh == 1 {
            break;
        }
        (level, lw, lh) = halve(&level, lw, lh);
    }
    b
}

/// `level` (`lw`×`lh`) at half each side, never below 1: a texel averages the texels its 2×2 block holds.
fn halve(level: &[[u8; 4]], lw: usize, lh: usize) -> (Vec<[u8; 4]>, usize, usize) {
    let (nw, nh) = ((lw / 2).max(1), (lh / 2).max(1));
    let mut next = Vec::with_capacity(nw * nh);
    for y in 0..nh {
        let ys = [2 * y, (2 * y + 1).min(lh - 1)];
        let ys = if ys[0] == ys[1] { &ys[..1] } else { &ys[..] };
        for x in 0..nw {
            let xs = [2 * x, (2 * x + 1).min(lw - 1)];
            let xs = if xs[0] == xs[1] { &xs[..1] } else { &xs[..] };
            let n = (xs.len() * ys.len()) as u32;
            next.push(std::array::from_fn(|k| {
                let sum: u32 = ys.iter().flat_map(|&j| xs.iter().map(move |&i| (i, j))).map(|(i, j)| u32::from(level[j * lw + i][k])).sum();
                ((sum + n / 2) / n) as u8
            }));
        }
    }
    (next, nw, nh)
}

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn gray_dds(g: &Gray) -> Vec<u8> {
    let px: Vec<[u8; 4]> = g.px.iter().map(|&v| { let c = byte(v); [c, c, c, 255] }).collect();
    dds_rgba8(g.width, g.height, &px, false)
}

/// The preview's maps: `n` always, `p` when a Parallax map was made, `r` (AO in B, rough 255, metal 0, spec 255)
/// when AO was made. Never an albedo: the shader's white default stands in.
pub fn maps_for(o: &Outputs) -> HashMap<&'static str, Vec<u8>> {
    let (w, h) = o.size;
    let n: Vec<[u8; 4]> = o.preview_normal.iter().map(|p| [byte(p[0]), byte(p[1]), byte(p[2]), 255]).collect();
    let mut maps = HashMap::from([("n", dds_rgba8(w, h, &n, false))]);
    if let Some((g, _)) = &o.parallax {
        maps.insert("p", gray_dds(g));
    }
    if let Some(g) = &o.ao {
        let px: Vec<[u8; 4]> = g.px.iter().map(|&v| [255, 0, byte(v), 255]).collect();
        maps.insert("r", dds_rgba8(g.width, g.height, &px, false));
    }
    maps
}

/// The RNM tab's preview maps: the `n` slot only, with the blend's alpha. An output over `PREVIEW_MAX` is halved
/// until it fits: an 8192 map would be 360 MB of preview texture.
pub fn normal_only(size: (u32, u32), px: &[[f32; 4]]) -> Maps {
    let mut n: Vec<[u8; 4]> = px.iter().map(|p| p.map(byte)).collect();
    let (mut w, mut h) = (size.0 as usize, size.1 as usize);
    while w.max(h) > PREVIEW_MAX as usize {
        (n, w, h) = halve(&n, w, h);
    }
    HashMap::from([("n", dds_rgba8(w as u32, h as u32, &n, false))])
}

/// The Cubemap tab's preview map: the `e` slot only.
pub fn cube_only(c: &engine::cube::Cube) -> Maps {
    HashMap::from([("e", crate::cube::files::preview_dds(c))])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::from_normal::maps::{Gray, ParallaxReport};
    use crate::from_normal::master::MasterReport;
    use crate::from_normal::worker::Outputs;

    fn u(d: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(d[at..at + 4].try_into().unwrap())
    }

    fn gray(v: f32) -> Gray {
        Gray { width: 2, height: 2, px: vec![v; 4] }
    }

    fn outputs(parallax: bool, ao: Option<f32>) -> Outputs {
        Outputs {
            id: 1,
            draft: false,
            size: (2, 2),
            report: MasterReport::default(),
            match_depth: 0.9,
            height: (gray(0.5), 0.0),
            parallax: parallax.then(|| (gray(0.5), ParallaxReport::default())),
            normal: None,
            preview_normal: vec![[0.5, 0.5, 1.0, 1.0]; 4],
            ao: ao.map(gray),
            cavity: None,
            curvature: None,
            compare: None,
            preview: None,
        }
    }

    #[test]
    fn a_normal_over_the_preview_limit_is_halved_for_the_preview() {
        let maps = normal_only((8192, 2), &vec![[0.5, 0.5, 1.0, 1.0]; 8192 * 2]);
        assert_eq!((u(&maps["n"], 16), u(&maps["n"], 12)), (4096, 1)); // width, height
        assert_eq!(&maps["n"][148..152], &[128, 128, 255, 255]);
    }

    #[test]
    fn a_non_square_dds_halves_each_side_to_one() {
        let d = dds_rgba8(8, 2, &[[10, 20, 30, 255]; 16], false);
        assert_eq!((u(&d, 16), u(&d, 12), u(&d, 28)), (8, 2, 4)); // width, height, mip count: 8×2, 4×1, 2×1, 1×1
        assert_eq!(u(&d, 20), 32);
        assert_eq!(d.len(), 148 + 4 * (16 + 4 + 2 + 1));
        assert_eq!(&d[d.len() - 4..], &[10, 20, 30, 255]);
    }

    #[test]
    fn ao_goes_to_blue_with_the_missing_map_defaults() {
        let maps = maps_for(&outputs(false, Some(0.5)));
        assert_eq!(&maps["r"][148..152], &[255, 0, 128, 255]); // R rough 255, G metal 0, B AO, A spec 255
    }

    #[test]
    fn only_made_maps_are_bound() {
        let mut keys: Vec<_> = maps_for(&outputs(false, None)).into_keys().collect();
        assert_eq!(keys, vec!["n"]);
        keys = maps_for(&outputs(true, Some(1.0))).into_keys().collect();
        keys.sort();
        assert_eq!(keys, vec!["n", "p", "r"]);
    }
}
