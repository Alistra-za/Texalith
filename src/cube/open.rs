//! Opening a cube source: one picture, or six face files found together from any one of them.
use super::import::{detect, face_tag, UNEQUAL};
use super::SourceType;
use crate::from_normal::files::{source, Source};
use engine::image::Rgba32F;
use engine::ops::game_gamma_to_linear;
use std::path::Path;
use std::sync::Arc;

/// The largest side a source keeps; a larger one is halved until it fits.
const MAX_SIDE: u32 = 8192;

/// One source and its image, or six in +X -X +Y -Y +Z -Z order. Images are linear light.
pub struct Opened {
    pub sources: Vec<Source>,
    pub imgs: Vec<Arc<Rgba32F>>,
    /// `Some(Faces)` for six face files, else what the picture's shape says.
    pub detected: Option<SourceType>,
}

/// `img` halved until no side is over 8192.
pub fn fit(mut img: Rgba32F) -> Rgba32F {
    while img.width > MAX_SIDE || img.height > MAX_SIDE {
        let size = ((img.width / 2).max(1), (img.height / 2).max(1));
        img = crate::resample::resample(img, size);
    }
    img
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `path` hashed and decoded to linear light: 8- and 16-bit codes through the game gamma, float files as they are.
fn load(path: &Path) -> Result<(Source, Arc<Rgba32F>), String> {
    let fail = |e: &dyn std::fmt::Display| format!("Could not read {}: {e}", file_name(path));
    let src = source(path).map_err(|e| fail(&e))?;
    let (mut img, info) = engine::load::load_image(path).map_err(|e| fail(&e))?;
    if !info.is_float {
        for p in &mut img.px {
            for v in &mut p[..3] {
                *v = game_gamma_to_linear(*v);
            }
        }
    }
    Ok((src, Arc::new(fit(img))))
}

/// `path` opened. A name with a face tag (`_posx`, `_nz`, ...) opens all six face files beside it.
pub fn open(path: &Path) -> Result<Opened, String> {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let Some((base, _)) = face_tag(&stem) else {
        let (src, img) = load(path)?;
        let detected = detect(&img);
        return Ok(Opened { sources: vec![src], imgs: vec![img], detected });
    };
    let dir = path.parent().unwrap_or(Path::new("."));
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let long = stem.len() - base.len() == 5;
    let siblings: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("Could not read {}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(&ext)))
        .collect();
    let found: Vec<_> = (0..6)
        .map(|k| {
            siblings.iter().find(|p| {
                let s = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                face_tag(&s).is_some_and(|(b, i)| i == k && b.eq_ignore_ascii_case(base))
            })
        })
        .collect();
    let tags = if long { ["posx", "negx", "posy", "negy", "posz", "negz"] } else { ["px", "nx", "py", "ny", "pz", "nz"] };
    let missing: Vec<String> =
        (0..6).filter(|&k| found[k].is_none()).map(|k| format!("{base}_{}.{ext}", tags[k])).collect();
    if !missing.is_empty() {
        return Err(format!("{} is one of six face files, but {} is missing.", file_name(path), missing.join(", ")));
    }
    let mut sources = Vec::new();
    let mut imgs = Vec::new();
    for p in found.into_iter().flatten() {
        let (src, img) = load(p)?;
        sources.push(src);
        imgs.push(img);
    }
    let e = imgs[0].width;
    if imgs.iter().any(|i| i.width != e || i.height != e) {
        return Err(UNEQUAL.into());
    }
    Ok(Opened { sources, imgs, detected: Some(SourceType::Faces) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::SourceType;
    use std::path::PathBuf;

    fn folder(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("texalith-cube-open-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// An 8-bit RGB PNG of `w` x `h` filled with `code`.
    fn png8(path: &Path, w: u32, h: u32, code: u8) {
        let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        let mut e = png::Encoder::new(file, w, h);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&vec![code; (w * h * 3) as usize]).unwrap();
    }

    const TAGS: [&str; 6] = ["posx", "negx", "posy", "negy", "posz", "negz"];

    #[test]
    fn an_8_bit_source_is_decoded_with_game_gamma() {
        let d = folder("gamma");
        let p = d.join("sky.png");
        png8(&p, 8, 4, 128);
        let o = open(&p).unwrap();
        let want = (128.0f32 / 255.0).powf(1.6);
        let px = o.imgs[0].at(3, 2);
        for k in 0..3 {
            assert!((px[k] - want).abs() < 1e-6, "{px:?} vs {want}");
        }
        assert_eq!(px[3], 1.0);
        assert_eq!(o.detected, Some(SourceType::Panorama));
        assert_eq!(o.sources.len(), 1);
        assert_eq!(o.sources[0].path, p);
    }

    #[test]
    fn a_source_over_8192_is_halved() {
        let big = fit(Rgba32F::new(8200, 10, [0.5; 4]));
        assert_eq!((big.width, big.height), (4100, 5));
        let edge = fit(Rgba32F::new(8192, 10, [0.5; 4]));
        assert_eq!((edge.width, edge.height), (8192, 10));
    }

    #[test]
    fn six_face_files_open_together() {
        let d = folder("six");
        for (k, t) in TAGS.iter().enumerate() {
            png8(&d.join(format!("room_{t}.png")), 4, 4, 40 * k as u8);
        }
        let o = open(&d.join("room_negy.png")).unwrap();
        assert_eq!(o.detected, Some(SourceType::Faces));
        let names: Vec<String> = o.sources.iter().map(|s| s.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, TAGS.map(|t| format!("room_{t}.png")));
        assert_eq!(o.imgs.len(), 6);
        assert!(o.imgs[5].at(0, 0)[0] > o.imgs[4].at(0, 0)[0], "the images follow the sources' order");
    }

    #[test]
    fn a_missing_face_file_is_named() {
        let d = folder("missing");
        for t in TAGS.iter().filter(|t| **t != "posz") {
            png8(&d.join(format!("room_{t}.png")), 4, 4, 100);
        }
        let e = open(&d.join("room_negy.png")).err().unwrap();
        assert_eq!(e, "room_negy.png is one of six face files, but room_posz.png is missing.");
    }
}
