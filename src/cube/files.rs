//! The cube's output name and the marked `_e.png`.
use super::import::face_tag;
use super::make::strip;
use super::view::PREVIEW_MAX;
use super::Settings;
use crate::from_normal::files::Source;
use crate::marker::{encode_rgb16, MapKind, Marker};
use engine::config::Format;
use engine::cube::sample::mip_chain;
use engine::cube::Cube;
use engine::dds::{dds_bytes_cube, DdsDesc, ResolvedHeader};
use engine::image::Rgba32F;
use engine::ops::linear_to_game_gamma;
use serde_json::{json, Map, Value};
use std::path::Path;

/// `<stem>_e.png`: the face tag of six `faces` files is dropped (a picture opened alone keeps its whole name), and
/// a stem that already ends in `_e` (any case) is kept as it is.
pub fn out_name(first_source: &Path, faces: bool) -> String {
    let stem = first_source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = face_tag(&stem).filter(|_| faces).map_or(stem.as_str(), |(s, _)| s);
    if stem.to_ascii_lowercase().ends_with("_e") {
        format!("{stem}.png")
    } else {
        format!("{stem}_e.png")
    }
}

/// The marked 16-bit RGB strip of `c`. `sources` names each input: `"source"`, or `"posx"` .. `"negz"`.
pub fn cube_png(c: &Cube, sources: &[(&'static str, &Source)], s: &Settings) -> Vec<u8> {
    let (w, h, px) = strip(c);
    let inputs: Map<String, Value> = sources
        .iter()
        .map(|(k, src)| (k.to_string(), json!({ "path": src.path.to_string_lossy(), "sha256": src.sha256 })))
        .collect();
    let first = sources.first().map(|(_, s)| *s);
    let m = Marker {
        tool: String::new(),
        version: env!("CARGO_PKG_VERSION").into(),
        kind: MapKind::Cube,
        pixels_sha256: String::new(),
        source_path: first.map(|s| s.path.to_string_lossy().into_owned()).unwrap_or_default(),
        source_sha256: first.map(|s| s.sha256.clone()).unwrap_or_default(),
        settings: {
            let mut v = json!(s);
            v["face_size"] = json!(c.size);
            v["inputs"] = Value::Object(inputs);
            v
        },
        span_texels: None,
        master_sha256: None,
    };
    encode_rgb16(w, h, &px, &m)
}

fn code(v: f32) -> u8 {
    (linear_to_game_gamma(v) * 255.0).round() as u8
}

/// The cube as a DDS for the 3D preview: faces cut to about `PREVIEW_MAX` by taking every `k`-th texel, as the cross
/// is, with a full mip chain made in linear light and stored in game gamma.
pub fn preview_dds(c: &Cube) -> Vec<u8> {
    let (size, k) = (c.size as usize, (c.size / PREVIEW_MAX).max(1) as usize);
    let n = size / k;
    let face = |f: usize| Rgba32F {
        width: n as u32,
        height: n as u32,
        px: (0..n * n).map(|i| c.faces[f].px[(i / n) * k * size + (i % n) * k]).collect(),
    };
    let chain = mip_chain(Cube { size: n as u32, faces: std::array::from_fn(face) }, usize::MAX);
    // Face-major. The engine writes Rgba8 as DXGI 87, which is B8G8R8A8 (Tessalith Note S4): blue goes first.
    let bgra = |l: &Cube, f: usize| l.faces[f].px.iter().flat_map(|p| [code(p[2]), code(p[1]), code(p[0]), 255]).collect();
    let levels: Vec<Vec<u8>> = (0..6).flat_map(|f| chain.iter().map(move |l| bgra(l, f))).collect();
    let desc = DdsDesc { width: n as u32, height: n as u32, format: Format::Rgba8, srgb: false, header: ResolvedHeader::Dx10 };
    dds_bytes_cube(&desc, &levels).expect("six equal chains of square faces")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::{Settings, SourceType};
    use crate::from_normal::files::Source;
    use crate::marker::{read, MapKind};
    use engine::cube::Cube;
    use engine::image::Rgba32F;
    use std::path::{Path, PathBuf};

    #[test]
    fn output_names_end_in_e() {
        assert_eq!(out_name(Path::new("C:/in/sky.hdr"), false), "sky_e.png");
        assert_eq!(out_name(Path::new("C:/in/room_posx.png"), true), "room_e.png");
        assert_eq!(out_name(Path::new("C:/in/room_PX.png"), true), "room_e.png");
        assert_eq!(out_name(Path::new("C:/in/city_ny.hdr"), false), "city_ny_e.png");
        assert_eq!(out_name(Path::new("C:/in/chrome_e.png"), false), "chrome_e.png");
        assert_eq!(out_name(Path::new("C:/in/chrome_E.png"), false), "chrome_E.png");
    }

    #[test]
    fn the_preview_dds_is_a_bgra_cube_with_all_mips() {
        // +Z red, the rest grey.
        let colour = |i| if i == 4 { [1.0, 0.0, 0.0, 1.0] } else { [0.2, 0.2, 0.2, 1.0] };
        let d = preview_dds(&Cube { size: 16, faces: std::array::from_fn(|i| Rgba32F::new(16, 16, colour(i))) });
        let u32_at = |at: usize| u32::from_le_bytes(d[at..at + 4].try_into().unwrap());
        assert_eq!((&d[..4], &d[84..88]), (&b"DDS "[..], &b"DX10"[..]));
        assert_eq!(u32_at(28), 5); // 16, 8, 4, 2, 1
        assert_eq!(u32_at(112) & 0xFE00, 0xFE00); // all six faces
        // The engine writes Rgba8 as DXGI 87, B8G8R8A8 (Tessalith Note S4): a red texel is stored B, G, R, A.
        assert_eq!(u32_at(128), 87);
        let chain: usize = [16usize, 8, 4, 2, 1].iter().map(|s| s * s * 4).sum();
        let posz = 148 + 4 * chain;
        assert_eq!(d[posz..posz + 4], [0, 0, 255, 255]);
        assert_eq!(d.len(), 148 + 6 * chain);
    }

    #[test]
    fn a_big_cube_previews_at_512_a_face() {
        let d = preview_dds(&Cube { size: 1024, faces: std::array::from_fn(|_| Rgba32F::new(1024, 1024, [0.5; 4])) });
        assert_eq!(u32::from_le_bytes(d[16..20].try_into().unwrap()), 512);
    }

    #[test]
    fn a_cube_png_is_a_marked_6_to_1_rgb_strip() {
        let c = Cube { size: 4, faces: std::array::from_fn(|_| Rgba32F::new(4, 4, [0.25, 0.5, 0.75, 1.0])) };
        let src = Source { path: PathBuf::from("C:/in/sky.hdr"), sha256: "ab".into() };
        let s = Settings { source: Some(SourceType::Panorama), softness: 0.2, brightness: 1.5, ..Settings::default() };
        let png = cube_png(&c, &[("source", &src)], &s);
        let r = png::Decoder::new(std::io::Cursor::new(&png)).read_info().unwrap();
        assert_eq!((r.info().width, r.info().height), (24, 4));
        assert_eq!(r.info().color_type, png::ColorType::Rgb);
        assert_eq!(r.info().bit_depth, png::BitDepth::Sixteen);
        let m = read(&png).unwrap();
        assert_eq!(m.kind, MapKind::Cube);
        assert_eq!((m.source_path.as_str(), m.source_sha256.as_str()), ("C:/in/sky.hdr", "ab"));
        assert_eq!(m.settings["source_type"], "panorama");
        assert_eq!(m.settings["face_size"], 4);
        assert_eq!(m.settings["brightness"], 1.5);
        assert_eq!(m.settings["inputs"]["source"]["sha256"], "ab");
        assert!(crate::marker::is_ours(&png));
    }

    #[test]
    fn a_cube_png_records_every_setting() {
        let c = Cube { size: 4, faces: std::array::from_fn(|_| Rgba32F::new(4, 4, [0.25, 0.5, 0.75, 1.0])) };
        let src = Source { path: PathBuf::from("C:/in/sky.hdr"), sha256: "ab".into() };
        let mut s = Settings {
            source: Some(SourceType::Panorama),
            turn: 30.0,
            ground: crate::cube::Ground::Colour([1, 2, 3]),
            ..Settings::default()
        };
        s.face_turn[2] = 90;
        let m = read(&cube_png(&c, &[("source", &src)], &s)).unwrap();
        assert_eq!(m.settings["turn"], 30.0);
        assert_eq!(m.settings["ground"], serde_json::json!({ "colour": [1, 2, 3] }));
        assert_eq!(m.settings["face_turn"][2], 90);
        assert!(m.settings["side"].is_null());
        assert_eq!(m.settings["turn_bottom"], true);
        assert_eq!(m.settings["source_type"], "panorama");
        assert_eq!(m.settings["face_size"], 4);
        assert_eq!(m.settings["inputs"]["source"]["sha256"], "ab");
    }
}
