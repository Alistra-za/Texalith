//! What a From normal input is: a normal, a height, or a parallax map (refused as a source).
use crate::marker::{MapKind, Marker};
use engine::image::Rgba32F;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Normal,
    /// The span is known only from a Height data marker.
    Height { span_texels: Option<f64> },
    Parallax,
}

/// RGB spread (max − min) at or below this reads as grey: JPEG chroma noise on a grey image stays within a few
/// levels, while a normal's B sits far from its R and G everywhere except near (0.577, 0.577, 0.577).
const GREY_SPREAD: f32 = 4.0 / 255.0;

/// By marker first, then the `_parallax` name, then whether every texel is grey.
pub fn classify(img: &Rgba32F, path: &Path, marker: Option<&Marker>) -> Kind {
    let parallax_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.to_ascii_lowercase().ends_with("_parallax"));
    match marker.map(|m| m.kind) {
        Some(MapKind::Parallax) => return Kind::Parallax,
        _ if parallax_name => return Kind::Parallax,
        Some(MapKind::HeightData) => return Kind::Height { span_texels: marker.and_then(|m| m.span_texels) },
        _ => {}
    }
    let grey = img
        .px
        .iter()
        .all(|p| p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]) <= GREY_SPREAD);
    if grey {
        Kind::Height { span_texels: None }
    } else {
        Kind::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marker::tests::sample;

    fn flat_img(n: u32, f: impl Fn(usize) -> [f32; 4]) -> Rgba32F {
        Rgba32F { width: n, height: n, px: (0..(n * n) as usize).map(f).collect() }
    }

    #[test]
    fn grey_with_jpeg_drift_is_height_and_bc5_style_is_normal() {
        let grey = flat_img(64, |i| {
            let v = 0.2 + (i % 7) as f32 / 10.0;
            [v, v + 1.5 / 255.0, v - 1.0 / 255.0, 1.0]
        });
        assert_eq!(classify(&grey, Path::new("rock.jpg"), None), Kind::Height { span_texels: None });
        let bc5 = flat_img(64, |i| [0.5 + 0.1 * ((i % 5) as f32 - 2.0) / 2.0, 0.5, 0.0, 1.0]);
        assert_eq!(classify(&bc5, Path::new("rock_n.png"), None), Kind::Normal);
    }

    #[test]
    fn an_rnm_marker_reads_as_a_normal() {
        let n = flat_img(16, |i| [0.5 + 0.2 * ((i % 3) as f32 - 1.0), 0.5, 0.9, 1.0]);
        assert_eq!(classify(&n, Path::new("rock_n.png"), Some(&sample(MapKind::Rnm))), Kind::Normal);
    }

    #[test]
    fn parallax_is_refused_by_marker_or_name() {
        let grey = flat_img(16, |_| [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(classify(&grey, Path::new("x_Parallax.png"), None), Kind::Parallax);
        assert_eq!(classify(&grey, Path::new("x.png"), Some(&sample(MapKind::Parallax))), Kind::Parallax);
        assert_eq!(
            classify(&grey, Path::new("x.png"), Some(&sample(MapKind::HeightData))),
            Kind::Height { span_texels: Some(12.5) }
        );
    }
}
