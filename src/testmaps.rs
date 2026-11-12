//! Synthetic maps for frame_diff and the window_timing example; the window no longer loads them at startup.
//! shortcut: fixed stand-ins, replace with a checked-in generated set if the timing run should use real maps.
use crate::preview::Content;
use render3d::Builtin;
use std::collections::HashMap;

fn hash(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

/// Tiling surface with broad shapes and per-texel grain, so parallax and mip selection both have work to do.
fn height(s: u32) -> Vec<f32> {
    let tau = std::f32::consts::TAU;
    let mut out = Vec::with_capacity((s * s) as usize);
    for y in 0..s {
        let v = y as f32 / s as f32;
        for x in 0..s {
            let u = x as f32 / s as f32;
            let b = (u * tau * 6.0).sin() * (v * tau * 6.0).sin() * 0.3
                + (u * tau * 23.0).sin() * (v * tau * 17.0).cos() * 0.12;
            out.push((0.5 + b + (hash(x, y) - 0.5) * 0.08).clamp(0.0, 1.0));
        }
    }
    out
}

/// RGBA8 DX10 DDS with a full box-filtered mip chain, what Texalith hands `render3d` for a generated map.
pub fn dds(s: u32, base: &[[u8; 4]], srgb: bool) -> Vec<u8> {
    crate::preview_maps::dds_rgba8(s, s, base, srgb)
}

/// Albedo `a`, normal `n`, RMAOS `r` and parallax `p`, `s`² texels. `metal` sets the RMAOS metal channel.
pub fn maps(s: u32, metal: bool) -> HashMap<&'static str, Vec<u8>> {
    let h = height(s);
    let n = s as usize;
    let at = |x: isize, y: isize| {
        let q = |v: isize| v.rem_euclid(n as isize) as usize;
        h[q(y) * n + q(x)]
    };
    let cap = n * n;
    let (mut albedo, mut normal, mut rmaos, mut parallax) = (
        Vec::with_capacity(cap),
        Vec::with_capacity(cap),
        Vec::with_capacity(cap),
        Vec::with_capacity(cap),
    );
    let k = s as f32 / 64.0;
    for y in 0..n as isize {
        for x in 0..n as isize {
            let v = at(x, y);
            let (dx, dy) = (
                (at(x + 1, y) - at(x - 1, y)) * k,
                (at(x, y + 1) - at(x, y - 1)) * k,
            );
            let l = (dx * dx + dy * dy + 1.0).sqrt();
            let enc = |c: f32| ((c * 0.5 + 0.5) * 255.0) as u8;
            let g = (v * 255.0) as u8;
            albedo.push([g, (v * 200.0) as u8, (v * 150.0) as u8, 255]);
            normal.push([enc(-dx / l), enc(-dy / l), enc(1.0 / l), 255]);
            rmaos.push([
                (255.0 - v * 160.0) as u8,
                if metal { 255 } else { 0 },
                (120.0 + v * 135.0) as u8,
                255,
            ]);
            parallax.push([g, g, g, 255]);
        }
    }
    HashMap::from([
        ("a", dds(s, &albedo, true)),
        ("n", dds(s, &normal, false)),
        ("r", dds(s, &rmaos, false)),
        ("p", dds(s, &parallax, false)),
    ])
}

pub fn content(s: u32) -> Content {
    Content {
        form: Builtin::MaterialBall,
        displacement: 1.0,
        maps: maps(s, false),
        white: false,
    }
}
