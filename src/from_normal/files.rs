//! From normal output names and the marked PNG bytes written for each output.
use super::maps::Gray;
use crate::marker::{encode_gray16, encode_rgb16, MapKind, Marker};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Input suffixes that name the map type, removed once from the stem.
const SUFFIXES: [&str; 6] = ["_n", "_normal", "_nrm", "_height", "_disp", "_displacement"];

/// The input's file stem with one trailing [`SUFFIXES`] entry removed (ASCII case-insensitive).
pub fn base_name(input: &Path) -> String {
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let lower = stem.to_ascii_lowercase();
    SUFFIXES
        .iter()
        .find(|s| lower.ends_with(*s) && lower.len() > s.len())
        .map_or(stem.clone(), |s| stem[..stem.len() - s.len()].to_string())
}

/// The output file name for `kind`.
pub fn out_name(base: &str, kind: MapKind) -> String {
    let suffix = match kind {
        MapKind::HeightData => "height",
        MapKind::Parallax => "parallax",
        MapKind::Normal => "n",
        MapKind::Ao => "ao",
        MapKind::Curvature => "curvature",
        MapKind::Cavity => "cavity",
        MapKind::Rnm | MapKind::Cube => unreachable!("RNM and Cube are not From normal outputs"),
    };
    format!("{base}_{suffix}.png")
}

/// The input file an output was made from, as its marker records it.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub path: PathBuf,
    pub sha256: String,
}

/// `path` with the SHA-256 of its bytes.
pub fn source(path: &Path) -> std::io::Result<Source> {
    let bytes = std::fs::read(path)?;
    Ok(Source { path: path.to_path_buf(), sha256: hex::encode(Sha256::digest(&bytes)) })
}

fn marker(kind: MapKind, src: &Source, settings: serde_json::Value) -> Marker {
    Marker {
        tool: String::new(),
        version: env!("CARGO_PKG_VERSION").into(),
        kind,
        pixels_sha256: String::new(),
        source_path: src.path.to_string_lossy().into_owned(),
        source_sha256: src.sha256.clone(),
        settings,
        span_texels: None,
        master_sha256: None,
    }
}

/// A 16-bit grey PNG of `g` with its marker. `span_texels` is for Height data; `master_sha256` (the Height data's
/// pixel hash) ties a Parallax map to its master.
pub fn gray_png(
    g: &Gray,
    kind: MapKind,
    src: &Source,
    settings: serde_json::Value,
    span_texels: Option<f64>,
    master_sha256: Option<String>,
) -> Vec<u8> {
    let m = Marker { span_texels, master_sha256, ..marker(kind, src, settings) };
    encode_gray16(g.width, g.height, &g.px, &m)
}

/// A 16-bit RGB PNG of a normal map with its marker.
pub fn normal_png(width: u32, height: u32, px: &[[f32; 4]], src: &Source, settings: serde_json::Value) -> Vec<u8> {
    encode_rgb16(width, height, px, &marker(MapKind::Normal, src, settings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marker;
    use serde_json::json;
    use std::path::Path;

    fn src() -> Source {
        Source { path: "C:/m/rock_n.png".into(), sha256: "00".into() }
    }

    #[test]
    fn names_strip_one_known_suffix() {
        assert_eq!(base_name(Path::new("C:/m/Rock_N.png")), "Rock");
        assert_eq!(base_name(Path::new("bark_height.tif")), "bark");
        assert_eq!(base_name(Path::new("brick_n_n.png")), "brick_n");
        assert_eq!(base_name(Path::new("rock.png")), "rock");
        assert_eq!(base_name(Path::new("wall_Displacement.exr")), "wall");
        assert_eq!(out_name("bark", MapKind::Parallax), "bark_parallax.png");
        assert_eq!(out_name("bark", MapKind::HeightData), "bark_height.png");
        assert_eq!(out_name("bark", MapKind::Normal), "bark_n.png");
    }

    #[test]
    fn a_height_png_carries_its_span_and_parallax_its_master() {
        let g = Gray { width: 2, height: 1, px: vec![0.0, 1.0] };
        let h = gray_png(&g, MapKind::HeightData, &src(), json!({}), Some(7.5), None);
        let mh = marker::read(&h).unwrap();
        assert_eq!(mh.span_texels, Some(7.5));
        assert_eq!((mh.kind, mh.source_path.as_str(), mh.source_sha256.as_str()), (MapKind::HeightData, "C:/m/rock_n.png", "00"));
        let p = gray_png(&g, MapKind::Parallax, &src(), json!({}), None, Some(mh.pixels_sha256.clone()));
        assert_eq!(marker::read(&p).unwrap().master_sha256, Some(mh.pixels_sha256));
        let n = normal_png(1, 1, &[[0.5, 0.5, 1.0, 1.0]], &src(), json!({ "green": "directx" }));
        assert_eq!(marker::read(&n).unwrap().kind, MapKind::Normal);
    }

    #[test]
    fn a_source_is_hashed_from_its_bytes() {
        let f = std::env::temp_dir().join(format!("texalith-files-{}.bin", std::process::id()));
        std::fs::write(&f, b"abc").unwrap();
        let s = source(&f).unwrap();
        std::fs::remove_file(&f).unwrap();
        assert_eq!(s.sha256, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert!(source(&f).is_err());
    }
}
