//! Pins `rnm::blend::apply` to Tessalith's `engine::rnm::blend::apply` on the same floats.
use engine::config::{LayerId as E, Subtype, Target};
use engine::image::Rgba32F;
use engine::presets::PresetRegistry;
use engine::rnm::{RnmAo, RnmFile, RnmJob, RnmLayer};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use texalith::rnm::{blend, Input, Kind, Layer};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("texalith-rnm-engine-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A 16-bit RGBA PNG of `w`×`h` from `f` (0..1 per channel).
fn png(d: &Path, name: &str, w: u32, h: u32, f: impl Fn(u32, u32) -> [f32; 4]) -> PathBuf {
    let path = d.join(name);
    let im = image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::from_fn(w, h, |x, y| {
        image::Rgba(f(x, y).map(|v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16))
    });
    im.save(&path).unwrap();
    path
}

fn load(p: &Path) -> Arc<Rgba32F> {
    Arc::new(engine::load::load_image(p).unwrap().0)
}

fn wave(x: u32, y: u32, a: f32, b: f32) -> [f32; 4] {
    let (x, y) = (x as f32, y as f32);
    [0.5 + 0.3 * (x * a).sin(), 0.5 + 0.25 * (y * b).cos(), 0.85 + 0.1 * (x * b).cos(), 1.0]
}

fn check(use_ao: bool) {
    let d = dir(if use_ao { "ao" } else { "no-ao" });
    let base = png(&d, "base.png", 32, 32, |x, y| {
        let n = wave(x, y, 0.2, 0.3);
        [n[0], n[1], n[2], (x + y) as f32 / 62.0]
    });
    let bump = png(&d, "bump.png", 24, 24, |x, y| {
        let v = 0.5 + 0.4 * (x as f32 * 0.4).sin() * (y as f32 * 0.3).cos();
        [v, v, v, 1.0]
    });
    // A colour mask: only its red counts.
    let bump_mask = png(&d, "bump_mask.png", 32, 32, |x, y| [y as f32 / 31.0, 1.0 - x as f32 / 31.0, 0.2, 1.0]);
    let bump2 = png(&d, "bump2.png", 32, 32, |x, y| {
        let v = ((x * 7 + y * 3) % 32) as f32 / 31.0;
        [v, v, v, 1.0]
    });
    let detail = png(&d, "detail.png", 64, 64, |x, y| wave(x, y, 0.5, 0.7));
    let detail_mask = png(&d, "detail_mask.png", 32, 32, |x, y| [x as f32 / 31.0, y as f32 / 31.0, 0.2, 1.0]);
    let detail2 = png(&d, "detail2.png", 16, 16, |x, y| wave(x, y, 0.9, 0.4));
    let bent = png(&d, "bent.png", 32, 32, |x, y| wave(x, y, 0.15, 0.25));
    let bent_mask = png(&d, "bent_mask.png", 32, 32, |x, _| {
        let v = (x as f32 / 31.0).powf(0.5);
        [v, v, v, 1.0]
    });
    let ao = png(&d, "ao.png", 32, 32, |x, y| {
        let v = ((x as f32 - 16.0).hypot(y as f32 - 16.0) / 22.0).min(1.0);
        [v, 0.0, 1.0, 1.0]
    });

    let work = blend::decode_base(&load(&base));

    let file = |p: &PathBuf| RnmFile { path: p.clone(), stamp: None };
    let layer = |id, p: &PathBuf, m: Option<&PathBuf>| RnmLayer { id, file: file(p), mask: m.map(file) };
    let job = RnmJob {
        layers: vec![
            layer(E::Bent, &bent, Some(&bent_mask)),
            layer(E::Bump, &bump, Some(&bump_mask)),
            layer(E::Bump2, &bump2, None),
            layer(E::Detail, &detail, Some(&detail_mask)),
            layer(E::Detail2, &detail2, None),
        ],
        ao: Some(RnmAo { file: file(&ao), channel: 0 }),
    };
    let mut p = PresetRegistry::builtin().unwrap().profile("cs-quality", Target::Cs, Subtype::Normal).unwrap();
    p.rnm.order = Some(vec![E::Detail, E::Bent, E::Bump2, E::Detail2, E::Bump]);
    p.rnm.bump.strength = Some(0.7);
    p.rnm.bump.flip_green = Some(false);
    p.rnm.bump2.strength = Some(0.25);
    p.rnm.bump2.flip_green = Some(false);
    p.rnm.detail.strength = Some(0.8);
    p.rnm.detail.flip_green = Some(true);
    p.rnm.detail2.strength = Some(0.45);
    p.rnm.detail2.flip_green = Some(false);
    p.rnm.bent.strength = Some(0.55);
    p.rnm.bent.flip_green = Some(false);
    p.rnm.bent.ao_power = Some(2.5);
    p.rnm.bent.ao = use_ao;
    let mut theirs = work.clone();
    engine::rnm::blend::apply(&mut theirs, &job, &p).unwrap();

    let layer = |kind, strength, flip, img: &PathBuf, mask: Option<&PathBuf>| Input {
        set: Layer { strength, flip, use_ao, ao_power: 2.5, ..Layer::new(kind) },
        img: load(img),
        mask: mask.map(|m| Arc::new(blend::red(&load(m)))),
        ao: None,
    };
    // Tessalith's order for this job: detail, bent, bump2, detail2, bump.
    let mut layers = vec![
        layer(Kind::Detail, 0.8, true, &detail, Some(&detail_mask)),
        layer(Kind::Bent, 0.55, false, &bent, Some(&bent_mask)),
        layer(Kind::Bump, 0.25, false, &bump2, None),
        layer(Kind::Detail, 0.45, false, &detail2, None),
        layer(Kind::Bump, 0.7, false, &bump, Some(&bump_mask)),
    ];
    layers[1].ao = Some(Arc::new(blend::red(&load(&ao))));
    let mut ours = work.clone();
    blend::apply(&mut ours, &layers);

    let diff = ours.px.iter().zip(&theirs.px).flat_map(|(a, b)| (0..4).map(move |i| (a[i] - b[i]).abs())).fold(0.0f32, f32::max);
    assert!(diff <= 1e-6, "largest difference {diff}");
    let moved = ours.px.iter().zip(&work.px).flat_map(|(a, b)| (0..3).map(move |i| (a[i] - b[i]).abs())).fold(0.0f32, f32::max);
    assert!(moved > 1e-2, "the layers must change the base");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn apply_matches_tessalith_with_ao() {
    check(true);
}

#[test]
fn apply_matches_tessalith_without_ao() {
    check(false);
}
