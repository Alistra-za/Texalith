//! RNM output names, the layer files found beside a base, and the bases RNM refuses.
use super::{Kind, Layer};
use crate::from_normal::files::{base_name, Source};
use crate::marker::{encode_rgba16, MapKind, Marker, MAX_SIDE};
use crate::tab::EXTENSIONS;
use engine::image::Rgba32F;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A file put in a layer: where it came from and its pixels.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub source: Source,
    pub img: Arc<Rgba32F>,
}

/// One blended layer as the marker records it.
pub struct Recorded<'a> {
    pub set: Layer,
    pub file: &'a Loaded,
    pub mask: Option<&'a Loaded>,
    pub ao: Option<&'a Loaded>,
}

/// The marked RGBA16 PNG of a blend. Its settings list `layers` in blend order; a layer's AO file is written only
/// for a Bent layer with `use_ao` on, because the blend reads it only then.
pub fn rnm_png(width: u32, height: u32, px: &[[f32; 4]], base: &Source, layers: &[Recorded]) -> Vec<u8> {
    let file = |l: &Loaded| json!({ "path": l.source.path.to_string_lossy(), "sha256": l.source.sha256 });
    let layers: Vec<Value> = layers
        .iter()
        .map(|l| {
            let mut v = json!({ "kind": l.set.kind, "strength": l.set.strength, "flip": l.set.flip, "file": file(l.file) });
            if let Some(m) = l.mask {
                v["mask"] = file(m);
            }
            if l.set.kind == Kind::Bent {
                v["use_ao"] = json!(l.set.use_ao);
                v["ao_power"] = json!(l.set.ao_power);
                if let (true, Some(a)) = (l.set.use_ao, l.ao) {
                    v["ao"] = file(a);
                }
            }
            v
        })
        .collect();
    let m = Marker {
        tool: String::new(),
        version: env!("CARGO_PKG_VERSION").into(),
        kind: MapKind::Rnm,
        pixels_sha256: String::new(),
        source_path: base.path.to_string_lossy().into_owned(),
        source_sha256: base.sha256.clone(),
        settings: json!({ "layers": layers }),
        span_texels: None,
        master_sha256: None,
    };
    encode_rgba16(width, height, px, &m)
}

fn file_stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `stem` without a trailing `_cnr` (any case), when it has one and something before it.
fn strip_cnr(stem: &str) -> Option<&str> {
    let lower = stem.to_ascii_lowercase();
    (lower.ends_with("_cnr") && lower.len() > 4).then(|| &stem[..stem.len() - 4])
}

/// The base suffix the output keeps.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Suffix {
    N,
    Cnr,
}

pub fn suffix(base: &Path) -> Suffix {
    if strip_cnr(&file_stem(base)).is_some() {
        Suffix::Cnr
    } else {
        Suffix::N
    }
}

/// The stem that `out_name` keeps, used to find siblings.
pub fn stem(base: &Path) -> String {
    let s = file_stem(base);
    strip_cnr(&s).map_or_else(|| base_name(base), str::to_string)
}

/// `<stem>_cnr.png` for a `_cnr` base, otherwise `<stem>_n.png`.
pub fn out_name(base: &Path) -> String {
    match suffix(base) {
        Suffix::Cnr => format!("{}_cnr.png", stem(base)),
        Suffix::N => format!("{}_n.png", stem(base)),
    }
}

/// The five places Tessalith's file names know, in its blend order (bent, bump, bump2, detail, detail2): the kind a
/// file found for the place opens as, and its strength.
const PLACES: [(Kind, f32); 5] = [(Kind::Bent, 0.35), (Kind::Bump, 0.4), (Kind::Bump, 0.4), (Kind::Detail, 0.6), (Kind::Detail, 0.3)];

/// Tessalith's RNM layer vocabulary (`crates/engine/src/rnm/mod.rs:66-82`): term, place in `PLACES`, whether it is
/// a mask.
const TERMS: [(&str, usize, bool); 17] = [
    ("bump", 1, false),
    ("bump2", 2, false),
    ("bumpmask", 1, true),
    ("bumpmask2", 2, true),
    ("detailnorm", 3, false),
    ("detailnorm2", 4, false),
    ("detailnormmask", 3, true),
    ("detailnormmask2", 4, true),
    ("bent", 0, false),
    ("bentmask", 0, true),
    ("detail", 3, false),
    ("detail2", 4, false),
    ("detail_mask", 3, true),
    ("detail2_mask", 4, true),
    ("bump_mask", 1, true),
    ("bump2_mask", 2, true),
    ("bent_mask", 0, true),
];

/// The layer term `stem` ends in (case-insensitive, longest first, each also with an `n_` or `cnr_` prefix): the
/// base stem before it, the place in `PLACES`, whether it is a mask, and the prefix. A stem that is only a term
/// gives `None`.
pub fn layer_of(stem: &str) -> Option<(String, usize, bool, Option<Suffix>)> {
    let lower = stem.to_ascii_lowercase();
    [("", None), ("n_", Some(Suffix::N)), ("cnr_", Some(Suffix::Cnr))]
        .into_iter()
        .flat_map(|(pre, sfx)| TERMS.iter().map(move |&(t, place, mask)| (format!("_{pre}{t}"), place, mask, sfx)))
        .filter(|(tail, ..)| lower.ends_with(tail.as_str()) && lower.len() > tail.len())
        .max_by_key(|(tail, ..)| tail.len())
        // The tail is ASCII, so the cut falls on a char boundary even in non-ASCII stems.
        .map(|(tail, place, mask, sfx)| (stem[..stem.len() - tail.len()].to_string(), place, mask, sfx))
}

/// A layer file found beside a base, with the mask found for it.
#[derive(Debug, PartialEq)]
pub struct FoundLayer {
    pub kind: Kind,
    pub strength: f32,
    pub file: PathBuf,
    pub mask: Option<PathBuf>,
}

/// What `siblings` found: the layers in the order bent, bump, bump2, detail, detail2, and the `_ao` file.
#[derive(Default, Debug, PartialEq)]
pub struct Found {
    pub layers: Vec<FoundLayer>,
    pub ao: Option<PathBuf>,
}

/// The image files in the base's folder named for its stem. A prefixed term counts only when it matches the base's
/// suffix and beats the unprefixed one; between two extensions the earlier in `EXTENSIONS` wins. A mask whose layer
/// file is missing is left out.
pub fn siblings(base: &Path) -> Found {
    let want = stem(base).to_lowercase();
    let own = suffix(base);
    let ao_stem = format!("{want}_ao");
    // Per slot (five layers, five masks, AO), the best candidate so far: (rank, path), lower rank wins.
    type Ranked = ((u8, usize, String), PathBuf);
    let mut best: [Option<Ranked>; 11] = Default::default();
    let entries = base.parent().and_then(|d| std::fs::read_dir(d).ok());
    for path in entries.into_iter().flatten().flatten().map(|e| e.path()) {
        let Some(ext) = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()) else { continue };
        let Some(ext_rank) = EXTENSIONS.iter().position(|e| *e == ext) else { continue };
        if !path.is_file() || path == base {
            continue;
        }
        let s = file_stem(&path);
        let (slot, prefixed) = if s.to_lowercase() == ao_stem {
            (10, false)
        } else {
            match layer_of(&s) {
                Some((st, place, mask, sfx)) if st.to_lowercase() == want && sfx.is_none_or(|x| x == own) => {
                    (place + if mask { 5 } else { 0 }, sfx.is_some())
                }
                _ => continue,
            }
        };
        let rank = (u8::from(!prefixed), ext_rank, file_name(&path));
        if best[slot].as_ref().is_none_or(|(r, _)| rank < *r) {
            best[slot] = Some((rank, path));
        }
    }
    let mut take = best.map(|b| b.map(|(_, p)| p));
    let layers = (0..5)
        .filter_map(|i| {
            let file = take[i].take()?;
            Some(FoundLayer { kind: PLACES[i].0, strength: PLACES[i].1, file, mask: take[i + 5].take() })
        })
        .collect();
    Found { layers, ao: take[10].take() }
}

/// Why RNM will not open `path` as a base, if it will not: a model-space `_msn` name, or a side above `MAX_SIDE`. A blend is one pass
/// over the texels, so it goes to 8192 where the Material tab's solve stops at 4096.
pub fn refuse(path: &Path, size: (u32, u32)) -> Option<String> {
    let name = file_name(path);
    let lower = file_stem(path).to_ascii_lowercase();
    if lower.ends_with("_msn") {
        Some(format!("{name} is a model-space normal (_msn); RNM blends tangent-space normals only."))
    } else if size.0 > MAX_SIDE || size.1 > MAX_SIDE {
        Some(format!("{name} is {}x{}; the largest RNM output is {MAX_SIDE}, so resize it first.", size.0, size.1))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::{Path, PathBuf};

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("texalith-rnm-files-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn out_names_keep_the_base_suffix() {
        assert_eq!(out_name(Path::new("rock_n.png")), "rock_n.png");
        assert_eq!(out_name(Path::new("Rock_CNR.dds")), "Rock_cnr.png");
        assert_eq!(out_name(Path::new("rock_normal.tga")), "rock_n.png");
        assert_eq!(out_name(Path::new("rock.png")), "rock_n.png");
    }

    #[test]
    fn layer_terms_match_longest_first_and_need_a_stem() {
        let l = |s: &str| layer_of(s);
        assert_eq!(l("rock_bump2"), Some(("rock".into(), 2, false, None)));
        assert_eq!(l("rock_bumpmask2"), Some(("rock".into(), 2, true, None)));
        assert_eq!(l("rock_n_detailnorm"), Some(("rock".into(), 3, false, Some(Suffix::N))));
        assert_eq!(l("Rock_CNR_Bent_Mask"), Some(("Rock".into(), 0, true, Some(Suffix::Cnr))));
        assert_eq!(l("rock_detail2_mask"), Some(("rock".into(), 4, true, None)));
        assert_eq!(l("bump"), None);
        assert_eq!(l("rock_albedo"), None);
    }

    #[test]
    fn siblings_list_the_layers_in_the_old_order() {
        let d = dir("siblings");
        for f in [
            "rock_n.png",
            "rock_bump.png",
            "rock_detail.png",
            "rock_n_detail.png",
            "rock_cnr_bump2.png",
            "rock_detail_mask.png",
            "rock_ao.png",
            "rock_bent.dds",
            "rock_bent.png",
            "other_bump.png",
        ] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        let found = siblings(&d.join("rock_n.png"));
        let got: Vec<_> = found.layers.iter().map(|l| (l.kind, l.strength, l.file.clone(), l.mask.clone())).collect();
        assert_eq!(
            got,
            vec![
                (Kind::Bent, 0.35, d.join("rock_bent.png"), None),
                (Kind::Bump, 0.4, d.join("rock_bump.png"), None),
                (Kind::Detail, 0.6, d.join("rock_n_detail.png"), Some(d.join("rock_detail_mask.png"))),
            ]
        );
        assert_eq!(found.ao, Some(d.join("rock_ao.png")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_second_detail_keeps_its_strength_and_a_lone_mask_is_left_out() {
        let d = dir("second-detail");
        for f in ["rock_n.png", "rock_detail2.png", "rock_bump_mask.png"] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        let found = siblings(&d.join("rock_n.png"));
        assert_eq!(found.layers, vec![FoundLayer { kind: Kind::Detail, strength: 0.3, file: d.join("rock_detail2.png"), mask: None }]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_rnm_png_records_its_layers() {
        use crate::from_normal::files::Source;
        use crate::marker::{read, MapKind};
        let src = |p: &str, sha: &str| Source { path: PathBuf::from(p), sha256: sha.into() };
        let loaded = |p: &str, sha: &str| Loaded { source: src(p, sha), img: Arc::new(Rgba32F::new(2, 2, [0.5; 4])) };
        let (bump, mask) = (loaded("C:/t/rock_bump.png", "b1"), loaded("C:/t/rock_bump_mask.png", "m1"));
        let (bent, ao) = (loaded("C:/t/rock_bent.png", "n1"), loaded("C:/t/rock_ao.png", "a1"));
        let off = Layer { use_ao: false, ..Layer::new(Kind::Bent) };
        let layers = [
            Recorded { set: Layer::new(Kind::Bump), file: &bump, mask: Some(&mask), ao: None },
            Recorded { set: Layer::new(Kind::Bent), file: &bent, mask: None, ao: Some(&ao) },
            Recorded { set: off, file: &bent, mask: None, ao: Some(&ao) },
        ];
        let png = rnm_png(2, 1, &[[0.5, 0.5, 1.0, 0.3], [0.5, 0.5, 1.0, 1.0]], &src("C:/t/rock_n.png", "base"), &layers);
        let m = read(&png).unwrap();
        assert_eq!((m.kind, m.source_sha256.as_str()), (MapKind::Rnm, "base"));
        let l = &m.settings["layers"];
        assert_eq!(l.as_array().unwrap().len(), 3);
        assert_eq!((&l[0]["kind"], &l[0]["file"]["sha256"], &l[0]["mask"]["sha256"]), (&json!("bump"), &json!("b1"), &json!("m1")));
        assert!(l[0].get("ao").is_none() && l[0].get("use_ao").is_none());
        assert_eq!((&l[1]["kind"], &l[1]["ao"]["sha256"], &l[1]["ao_power"]), (&json!("bent"), &json!("a1"), &json!(1.5f32)));
        assert_eq!(l[2]["use_ao"], false);
        assert!(l[2].get("ao").is_none());
        assert!(m.settings.get("order").is_none() && m.settings.get("inputs").is_none());
    }

    #[test]
    fn msn_and_oversized_bases_are_refused() {
        assert_eq!(
            refuse(Path::new("C:/t/rock_msn.png"), (512, 512)).as_deref(),
            Some("rock_msn.png is a model-space normal (_msn); RNM blends tangent-space normals only.")
        );
        assert_eq!(
            refuse(Path::new("C:/t/rock_n.png"), (8193, 16)).as_deref(),
            Some("rock_n.png is 8193x16; the largest RNM output is 8192, so resize it first.")
        );
        assert_eq!(refuse(Path::new("C:/t/rock_n.png"), (8192, 8192)), None);
    }
}
