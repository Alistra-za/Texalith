//! Opening a From normal source: the pixels, its marker, what it is, its hash and the sizes it can make.
use super::files::Source;
use super::input::{classify, Kind};
use super::master::BuildError;
use super::sizes;
use crate::marker::{self, Marker};
use engine::image::Rgba32F;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Opened {
    pub path: PathBuf,
    pub img: Arc<Rgba32F>,
    pub kind: Kind,
    pub source: Source,
    pub marker: Option<Marker>,
    pub sizes: Vec<u32>,
}

/// `path` read once, decoded, classified and hashed. A parallax map is refused; the `Err` text is shown as is.
pub fn open(path: &Path) -> Result<Opened, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let is_png = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png"));
    let marker = if is_png { marker::read(&bytes) } else { None };
    let (img, _) = engine::load::load_image(path).map_err(|e| e.to_string())?;
    let kind = classify(&img, path, marker.as_ref());
    if kind == Kind::Parallax {
        return Err(BuildError::Parallax.to_string());
    }
    let source = Source { path: path.to_path_buf(), sha256: hex::encode(Sha256::digest(&bytes)) };
    let sizes = sizes((img.width, img.height));
    Ok(Opened { path: path.to_path_buf(), img: Arc::new(img), kind, source, marker, sizes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::from_normal::input::Kind;
    use crate::from_normal::master::BuildError;
    use crate::marker::tests::sample;
    use crate::marker::{encode_gray16, encode_rgb16, MapKind};
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("texalith-open-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_normal_opens_as_a_normal_with_its_sizes() {
        let px: Vec<[f32; 4]> = (0..64 * 32).map(|i| [0.5 + 0.2 * ((i % 7) as f32 / 7.0 - 0.5), 0.5, 0.9, 1.0]).collect();
        let p = temp("normal").join("rock_n.png");
        std::fs::write(&p, encode_rgb16(64, 32, &px, &sample(MapKind::Normal))).unwrap();
        let o = open(&p).unwrap();
        assert_eq!(o.kind, Kind::Normal);
        assert_eq!(o.sizes, vec![64]);
        assert_eq!((o.img.width, o.img.height), (64, 32));
        assert_eq!(o.source.sha256.len(), 64);
        assert_eq!(o.marker.map(|m| m.kind), Some(MapKind::Normal));
    }

    #[test]
    fn a_marked_height_keeps_its_span() {
        let p = temp("height").join("rock_height.png");
        std::fs::write(&p, encode_gray16(2, 2, &[0.0, 0.25, 0.5, 1.0], &sample(MapKind::HeightData))).unwrap();
        assert_eq!(open(&p).unwrap().kind, Kind::Height { span_texels: Some(12.5) });
    }

    #[test]
    fn a_parallax_map_is_refused_with_the_library_text() {
        let p = temp("parallax").join("x_parallax.png");
        std::fs::write(&p, encode_gray16(2, 2, &[0.5; 4], &sample(MapKind::HeightData))).unwrap();
        assert_eq!(open(&p).err(), Some(BuildError::Parallax.to_string()));
    }

    #[test]
    fn a_missing_file_says_so() {
        let e = open(&temp("missing").join("gone_n.png")).err().unwrap();
        assert!(e.contains("gone_n.png"), "{e}");
    }
}
