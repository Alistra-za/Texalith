//! Whole frames: the bundle and pinned settings against the owner's installed Community Shaders and settings.
//! Needs the install on G:. Run with: cargo test --release --test frame_diff -- --ignored --nocapture
use render3d::cs_settings::{read_settings, CsSettings, SettingsPaths};
use render3d::scene::{AUTO_PITCH, AUTO_YAW};
use render3d::{Builtin, Renderer};
use std::path::{Path, PathBuf};
use texalith::preview::{load, scene, Content, View};
use texalith::{bundle, testmaps};

const INSTALL: &str = r"G:\Acherus\mods\CommunityShaders_AIO-2026-10-03T06-46Z\Shaders";
const OVERWRITE: &str = r"G:\Acherus\overwrite";
const SIZE: (u32, u32) = (800, 600);

struct Case {
    name: String,
    px: Vec<u8>,
}

fn materials() -> Vec<(&'static str, Content)> {
    let full = testmaps::maps(512, false);
    let metal = testmaps::maps(512, true);
    let with = |displacement: f32, maps: std::collections::HashMap<&'static str, Vec<u8>>| Content {
        form: Builtin::Sphere,
        displacement,
        maps,
        white: false,
    };
    let mut flat = full.clone();
    flat.remove("p");
    let only_p: std::collections::HashMap<_, _> = [("p", full["p"].clone())].into();
    vec![
        ("maps_parallax1", with(1.0, full.clone())),
        ("maps_parallax3", with(3.0, full)),
        ("maps_no_parallax", with(1.0, flat)),
        ("metal", with(1.0, metal)),
        ("parallax_only", with(1.0, only_p)),
        ("no_maps", with(1.0, Default::default())),
    ]
}

fn render_all(shaders: &Path, settings: CsSettings, cache: &Path) -> (Vec<String>, Vec<Case>) {
    let fp = render3d::locate::fingerprint(shaders, None).unwrap();
    let mut r = Renderer::new(fp, cache.to_path_buf()).unwrap();
    r.set_settings(settings);
    let defines = r.features().to_vec();
    let mut cases = vec![];
    for (name, content) in materials() {
        for form in Builtin::ALL {
            let c = Content { form, ..content.clone() };
            load(&mut r, &c).unwrap_or_else(|e| panic!("{name} {form:?}: {e}"));
            for (cam, yaw, pitch, zoom) in [("front", AUTO_YAW, AUTO_PITCH, 1.0), ("close", AUTO_YAW + 0.5, 0.5, 0.55)] {
                let v = View { yaw, pitch, zoom, w: SIZE.0, h: SIZE.1 };
                let px = r.render(&scene(&v, r.bounds().unwrap()), v.w, v.h).unwrap();
                cases.push(Case { name: format!("{form:?}_{name}_{cam}"), px });
            }
        }
    }
    (defines, cases)
}

#[derive(Default, Debug)]
struct Diff {
    frames: usize,
    frames_differing: usize,
    worst: u32,
    worst_frame: String,
    pixels_over_2: u64,
    sum: u64,
    bytes: u64,
}

fn diff(a: &[Case], b: &[Case]) -> Diff {
    let mut d = Diff::default();
    for (x, y) in a.iter().zip(b) {
        assert_eq!(x.name, y.name);
        let mut frame_worst = 0;
        for (p, q) in x.px.as_chunks::<4>().0.iter().zip(y.px.as_chunks::<4>().0) {
            let w = (0..3).map(|i| p[i].abs_diff(q[i]) as u32).max().unwrap();
            frame_worst = frame_worst.max(w);
            d.pixels_over_2 += u64::from(w > 2);
            d.sum += (0..3).map(|i| p[i].abs_diff(q[i]) as u64).sum::<u64>();
            d.bytes += 3;
        }
        d.frames += 1;
        d.frames_differing += usize::from(frame_worst > 0);
        if frame_worst > d.worst {
            d.worst = frame_worst;
            d.worst_frame.clone_from(&x.name);
        }
    }
    d
}

fn report(name: &str, d: &Diff) {
    println!(
        "{name:<34} frames {:>3}, differing {:>3}, worst {:>3}/255, pixels over 2/255 {:>8}, mean {:.4}, worst frame {}",
        d.frames,
        d.frames_differing,
        d.worst,
        d.pixels_over_2,
        d.sum as f64 / d.bytes.max(1) as f64,
        d.worst_frame
    );
}

fn copy_tree(from: &Path, to: &Path, skip: &str) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_tree(&e.path(), &dest, skip);
        } else if e.file_name() != skip {
            std::fs::copy(e.path(), dest).unwrap();
        }
    }
}

fn save(dir: &Path, tag: &str, cases: &[Case], names: &[&str]) {
    for c in cases.iter().filter(|c| names.contains(&c.name.as_str())) {
        let path = dir.join(format!("{tag}_{}.png", c.name));
        image::save_buffer(path, &c.px, SIZE.0, SIZE.1, image::ColorType::Rgba8).unwrap();
    }
}

#[test]
#[ignore = "needs the owner's Community Shaders install on G:"]
fn bundle_matches_the_install() {
    let install = PathBuf::from(INSTALL);
    let overwrite = PathBuf::from(OVERWRITE);
    let dir = bundle::dir();
    let bundled = bundle::shaders(&dir);
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("frame_diff");
    let out = std::env::var_os("FRAME_DIFF_OUT").map_or_else(|| tmp.join("png"), PathBuf::from);
    let cache = tmp.join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::create_dir_all(&out).unwrap();

    let owner = read_settings(&SettingsPaths {
        user: Some(overwrite.join(r"SKSE\Plugins\CommunityShaders\SettingsUser.json")),
        default: None,
        info_ini: Some(overwrite.join(r"ShaderCache\Info.ini")),
        features_dir: Some(install.join("Features")),
    });
    let pinned = bundle::settings(&dir);
    assert!(pinned.notes.is_empty(), "{:?}", pinned.notes);

    let (defines_a, a) = render_all(&install, owner.clone(), &cache);
    let (_, a2) = render_all(&install, owner.clone(), &cache);
    let (defines_b, b) = render_all(&install, pinned.clone(), &cache);
    let (defines_c, c) = render_all(&bundled, pinned.clone(), &cache);
    println!("defines: owner {defines_a:?}\n         pinned {defines_b:?}\n         bundle {defines_c:?}");

    let (rerun, full_vs_bundle, owner_vs_pinned, shipped) = (diff(&a, &a2), diff(&b, &c), diff(&a, &b), diff(&a, &c));
    report("noise: install rerun", &rerun);
    report("bundle vs full tree, same settings", &full_vs_bundle);
    report("pinned vs owner settings", &owner_vs_pinned);
    report("shipped (bundle+pinned) vs owner", &shipped);

    // Controls: each must differ, or a zero above proves nothing.
    let (_, no_settings) = render_all(&bundled, CsSettings::default(), &cache);
    let d_settings = diff(&c, &no_settings);
    report("control: no settings files", &d_settings);
    let cut = tmp.join("no-prefilter");
    let _ = std::fs::remove_dir_all(&cut);
    copy_tree(&bundled, &cut, "SpecularIrradianceCS.hlsl");
    let (_, no_prefilter) = render_all(&cut, pinned.clone(), &tmp.join("cache-cut"));
    let d_prefilter = diff(&c, &no_prefilter);
    report("control: prefilter file removed", &d_prefilter);

    // What each pinned key does, and proof that a value in the file reaches the shading at all.
    let variants = [
        ("EnableParallax removed", r#"{"Extended Materials":{"EnableShadows":1,"EnableParallaxWarpingFix":1}}"#),
        ("EnableShadows removed", r#"{"Extended Materials":{"EnableParallax":1,"EnableParallaxWarpingFix":1}}"#),
        ("EnableParallaxWarpingFix removed", r#"{"Extended Materials":{"EnableParallax":1,"EnableShadows":1}}"#),
        ("Linear Lighting section removed", r#"{"Extended Materials":{"EnableParallax":1,"EnableShadows":1,"EnableParallaxWarpingFix":1}}"#),
        ("directionalLightMult 2", r#"{"Extended Materials":{"EnableParallax":1,"EnableShadows":1,"EnableParallaxWarpingFix":1},"Linear Lighting":{"enableLinearLighting":1,"directionalLightMult":2.0}}"#),
    ];
    let mut reached_shading = false;
    for (name, json) in variants {
        let file = tmp.join("variant.json");
        std::fs::write(&file, json).unwrap();
        let settings = read_settings(&SettingsPaths {
            user: Some(file),
            default: None,
            info_ini: Some(dir.join("settings/Info.ini")),
            features_dir: None,
        });
        let (_, v) = render_all(&bundled, settings, &cache);
        let d = diff(&c, &v);
        report(&format!("key: {name}"), &d);
        reached_shading |= name.starts_with("directional") && d.frames_differing > 0;
    }
    assert!(reached_shading, "a changed setting must change frames, or the comparison proves nothing");

    let samples = ["MaterialBall_maps_parallax1_front", "Plane_maps_parallax3_close", "Sphere_metal_front"];
    save(&out, "owner", &a, &samples);
    save(&out, "shipped", &c, &samples);

    assert_eq!(defines_c, defines_a, "defines must match");
    assert!(d_settings.frames_differing > 0, "the no-settings control must differ");
    assert_eq!(full_vs_bundle.frames_differing, 0, "trimming the tree changed frames");
    assert_eq!(shipped.frames_differing, 0, "the shipped setup differs from the install");
}
