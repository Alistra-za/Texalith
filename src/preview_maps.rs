//! The 3D preview's maps from the From normal outputs: RGBA8 DDS files with full mip chains for the `n`, `p` and `r`
//! slots (spec: "Values to pin", Maps).
use crate::from_normal::maps::Gray;
use crate::from_normal::worker::Outputs;
use std::collections::HashMap;

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
        level = next;
        (lw, lh) = (nw, nh);
    }
    b
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

/// The RNM tab's preview maps: the `n` slot only, with the blend's alpha.
pub fn normal_only(size: (u32, u32), px: &[[f32; 4]]) -> HashMap<&'static str, Vec<u8>> {
    let n: Vec<[u8; 4]> = px.iter().map(|p| p.map(byte)).collect();
    HashMap::from([("n", dds_rgba8(size.0, size.1, &n, false))])
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

    /// `testmaps::dds` before it became a call to `dds_rgba8`, for the unchanged-bytes check.
    fn old_dds(s: u32, base: &[[u8; 4]], srgb: bool) -> Vec<u8> {
        let mips = 32 - s.leading_zeros();
        let mut b = vec![0u8; 148];
        let mut put = |at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        put(0, u32::from_le_bytes(*b"DDS "));
        put(4, 124);
        put(8, 0x1 | 0x2 | 0x4 | 0x8 | 0x1000 | 0x20000);
        put(12, s);
        put(16, s);
        put(20, s * 4);
        put(28, mips);
        put(76, 32);
        put(80, 0x4);
        put(84, u32::from_le_bytes(*b"DX10"));
        put(108, 0x1000 | 0x8 | 0x400000);
        put(128, if srgb { 29 } else { 28 });
        put(132, 3);
        put(140, 1);
        let mut level = base.to_vec();
        let mut n = s as usize;
        loop {
            b.extend(level.iter().flatten());
            if n == 1 {
                break;
            }
            let m = n / 2;
            let mut next = Vec::with_capacity(m * m);
            for y in 0..m {
                for x in 0..m {
                    let t = |i: usize, j: usize| level[(y * 2 + j) * n + x * 2 + i];
                    let (a, c, d, e) = (t(0, 0), t(1, 0), t(0, 1), t(1, 1));
                    next.push(std::array::from_fn(|k| ((a[k] as u32 + c[k] as u32 + d[k] as u32 + e[k] as u32 + 2) / 4) as u8));
                }
            }
            level = next;
            n = m;
        }
        b
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
        }
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
    fn the_square_test_maps_are_unchanged() {
        let base: Vec<[u8; 4]> = (0..16u8).map(|i| [i * 13, 255 - i * 7, i * 3 + 1, 255]).collect();
        assert_eq!(crate::testmaps::dds(4, &base, true), old_dds(4, &base, true));
        assert_eq!(dds_rgba8(4, 4, &base, false), old_dds(4, &base, false));
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
