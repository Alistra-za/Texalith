//! The contract between Texalith's `<base>_e.png` and Tessalith's converter, run through Tessalith's real plan and
//! convert. Written down in `reports/Rust material completer research.md`, "The cubemap contract". No GPU.
//!
//! `the_tabs_own_strip_lands_every_texel_on_its_direction` runs the Cubemap tab's own writer, `cube::files::cube_png`.
use engine::api::*;
use engine::codes::Code;
use engine::config::*;
use engine::cube::checks::inspect_issues;
use engine::cube::cube_dir;
use engine::dds::{read_dds, DdsFile};
use engine::decode::{decode_level, Decoded};
use engine::events::*;
use engine::overrides::parse_set;
use engine::policy::{AcceptSuggestions, JobStatus, ResolvedJob};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const NO_MARKER: Option<&str> = None;

/// What a stored pixel of `face` holds: its cube direction as a colour, so any wrong face, turn or flip shows.
fn field(face: usize, x: u32, y: u32, n: u32) -> [f32; 3] {
    cube_dir(face, x, y, n).map(|d| 0.5 + 0.5 * d)
}

fn code8(v: f32) -> u8 {
    (v * 255.0).round() as u8
}

#[derive(Clone, Copy)]
enum Depth {
    Eight,
    Sixteen,
}

/// How a strip is laid out. `CONTRACT` is what Texalith must write; the others are the mistakes the tests must catch.
#[derive(Clone, Copy)]
struct Strip {
    order: [usize; 6],
    upside_down: bool,
}

const CONTRACT: Strip = Strip { order: [0, 1, 2, 3, 4, 5], upside_down: false };

fn strip_bytes(n: u32, depth: Depth, s: Strip) -> Vec<u8> {
    let mut out = vec![];
    for y in 0..n {
        for &face in &s.order {
            for x in 0..n {
                let row = if s.upside_down { n - 1 - y } else { y };
                for v in field(face, x, row, n) {
                    match depth {
                        Depth::Eight => out.push(code8(v)),
                        Depth::Sixteen => out.extend(((v * 65535.0).round() as u16).to_be_bytes()),
                    }
                }
            }
        }
    }
    out
}

/// Written with the `png` crate, as the app will: `image` cannot write the marker chunk.
fn write_png(path: &Path, (w, h): (u32, u32), color: png::ColorType, depth: Depth, px: &[u8], marker: Option<&str>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut e = png::Encoder::new(file, w, h);
    e.set_color(color);
    e.set_depth(match depth {
        Depth::Eight => png::BitDepth::Eight,
        Depth::Sixteen => png::BitDepth::Sixteen,
    });
    if let Some(text) = marker {
        e.add_itxt_chunk("Texalith".into(), text.into()).unwrap();
    }
    let mut w = e.write_header().unwrap();
    w.write_image_data(px).unwrap();
}

fn write_strip(path: &Path, n: u32, depth: Depth, s: Strip, marker: Option<&str>) {
    write_png(path, (6 * n, n), png::ColorType::Rgb, depth, &strip_bytes(n, depth, s), marker);
}

/// A clean folder for one test, with `in` holding its inputs.
fn folder(test: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cube_contract").join(test);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("in")).unwrap();
    d
}

struct Converted {
    job: ResolvedJob,
    summary: BatchSummary,
    log: Vec<EngineEvent>,
}

/// Tessalith's whole path over `dir/in`, as `sdc convert --recursive --accept-suggestions high`; the job writing `out`.
fn convert(dir: &Path, out: &str, target: Target, sets: &[&str]) -> Converted {
    let mut r = BatchRequest::new(vec![dir.join("in")], dir.join("out"), target);
    r.recursive = true;
    r.accept = AcceptSuggestions::High;
    r.overrides = sets.iter().map(|s| parse_set(s).unwrap()).collect();
    let sink = Arc::new(VecSink::default());
    let rep = Reporter::new(sink.clone(), "cube contract");
    let engine = Engine::new().unwrap();
    let plan = engine.plan(r, &rep, &CancelToken::default()).unwrap();
    let job = plan
        .jobs
        .iter()
        .find(|j| j.output.file_name().is_some_and(|f| f == out))
        .unwrap_or_else(|| panic!("no job writes {out}: {:#?}", plan.jobs))
        .clone();
    let summary = engine.run(plan, &rep, &CancelToken::default()).unwrap();
    Converted { job, summary, log: sink.take() }
}

fn dds(job: &ResolvedJob) -> DdsFile {
    read_dds(&std::fs::read(&job.output).unwrap()).unwrap()
}

/// Mip `mip` of `face`, decoded.
fn texels(f: &DdsFile, face: usize, mip: usize) -> Vec<[u8; 4]> {
    let (w, h, r) = f.surfaces()[face * f.mip_count as usize + mip].clone();
    match decode_level(f.format, w, h, &f.data[r]).unwrap() {
        Decoded::Ldr(px) => px,
        Decoded::Hdr(_) => panic!("a cube from a PNG is not HDR"),
    }
}

/// The largest channel difference between mip 0 of `f` and the contract's picture.
fn worst_error(f: &DdsFile, n: u32) -> i32 {
    let mut worst = 0;
    for face in 0..6 {
        let got = texels(f, face, 0);
        for y in 0..n {
            for x in 0..n {
                let want = field(face, x, y, n).map(code8);
                let px = got[(y * n + x) as usize];
                for k in 0..3 {
                    worst = worst.max((i32::from(px[k]) - i32::from(want[k])).abs());
                }
            }
        }
    }
    worst
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

#[test]
fn a_strip_in_any_folder_becomes_a_bc7_unorm_cube_with_all_its_mips() {
    for target in [Target::Cs, Target::Vanilla] {
        let d = folder("default");
        // Not under `textures`, and no other map beside it.
        write_strip(&d.join("in/anything/chrome_e.png"), 64, Depth::Eight, CONTRACT, NO_MARKER);
        let c = convert(&d, "chrome_e.dds", target, &[]);
        assert_eq!(c.job.status, JobStatus::Ready, "{:#?}", c.job.warnings);
        assert_eq!(c.job.cube.as_ref().unwrap().layout, CubeLayout::Hstrip);
        assert_eq!((c.summary.converted, c.summary.failed, c.summary.validation_failed), (1, 0, 0), "{:#?}", c.log);

        let bytes = std::fs::read(&c.job.output).unwrap();
        let f = read_dds(&bytes).unwrap();
        assert!(f.is_cubemap);
        assert_eq!((f.width, f.height, f.mip_count), (64, 64, 7));
        assert_eq!(f.caps2, 0xFE00, "all six faces");
        assert_eq!((u32_at(&bytes, 128), u32_at(&bytes, 136)), (98, 4), "DXGI 98 is BC7 UNORM, misc 4 is a cube");
        assert_eq!(inspect_issues("chrome_e.dds", &f), Vec::<String>::new());
        // The default path encodes, so close instead of exact, but enough to catch a face in the wrong place.
        let worst = worst_error(&f, 64);
        assert!(worst <= 3, "{target:?}: BC7 worst channel error {worst}");
    }
}

#[test]
fn every_texel_lands_on_its_cube_direction_unchanged() {
    for (depth, name) in [(Depth::Eight, "8"), (Depth::Sixteen, "16")] {
        let d = folder(&format!("exact{name}"));
        write_strip(&d.join("in/chrome_e.png"), 32, depth, CONTRACT, NO_MARKER);
        let c = convert(&d, "chrome_e.dds", Target::Cs, &["format=rgba8"]);
        assert_eq!(c.summary.converted, 1, "{:#?}", c.log);
        assert_eq!(worst_error(&dds(&c.job), 32), 0, "{name}-bit");
    }
}

#[test]
fn the_tabs_own_strip_lands_every_texel_on_its_direction() {
    let n = 32;
    // The tab holds linear light and writes game gamma, so the cube holds the field in linear light.
    let faces = std::array::from_fn(|f| {
        let mut img = engine::image::Rgba32F::new(n, n, [0.0; 4]);
        for y in 0..n {
            for x in 0..n {
                let [r, g, b] = field(f, x, y, n).map(engine::ops::game_gamma_to_linear);
                *img.at_mut(x, y) = [r, g, b, 1.0];
            }
        }
        img
    });
    let c = engine::cube::Cube { size: n, faces };
    let src = texalith::from_normal::files::Source { path: "C:/in/chrome.hdr".into(), sha256: "00".into() };
    let s = texalith::cube::Settings { source: Some(texalith::cube::SourceType::Panorama), ..Default::default() };
    let d = folder("tab");
    std::fs::write(d.join("in/chrome_e.png"), texalith::cube::files::cube_png(&c, &[("source", &src)], &s)).unwrap();
    let c = convert(&d, "chrome_e.dds", Target::Cs, &["format=rgba8"]);
    assert_eq!(c.summary.converted, 1, "{:#?}", c.log);
    assert_eq!(worst_error(&dds(&c.job), n), 0);
}

/// The check above is only worth something if these fail it.
#[test]
fn a_strip_laid_out_any_other_way_does_not_pass() {
    let wrong = [
        ("x swapped", Strip { order: [1, 0, 2, 3, 4, 5], upside_down: false }),
        ("y and z swapped", Strip { order: [0, 1, 4, 5, 2, 3], upside_down: false }),
        ("upside down", Strip { order: [0, 1, 2, 3, 4, 5], upside_down: true }),
    ];
    for (name, s) in wrong {
        let d = folder("wrong");
        write_strip(&d.join("in/chrome_e.png"), 32, Depth::Eight, s, NO_MARKER);
        let c = convert(&d, "chrome_e.dds", Target::Cs, &["format=rgba8"]);
        assert_eq!(c.summary.converted, 1, "{:#?}", c.log);
        assert!(worst_error(&dds(&c.job), 32) > 16, "{name} was not noticed");
    }
}

/// Mip 0 cannot tell gammas apart, as decode and encode cancel; a mip can. 0 and 255 average to
/// round(255 * 0.5^(1/1.6)) = 165 in game gamma, and would be 188 in sRGB or 128 with no decode.
#[test]
fn mips_average_in_linear_light_under_the_game_gamma() {
    let n = 16u32;
    let px: Vec<u8> = (0..n)
        .flat_map(|y| (0..6 * n).map(move |x| if (x + y) % 2 == 0 { 255 } else { 0 }))
        .flat_map(|v| [v, v, v])
        .collect();
    let d = folder("gamma");
    write_png(&d.join("in/chrome_e.png"), (6 * n, n), png::ColorType::Rgb, Depth::Eight, &px, NO_MARKER);
    let c = convert(&d, "chrome_e.dds", Target::Cs, &["format=rgba8"]);
    assert_eq!(c.summary.converted, 1, "{:#?}", c.log);
    let f = dds(&c.job);
    assert_eq!(f.mip_count, 5);
    for face in 0..6 {
        for p in texels(&f, face, 1) {
            assert!((i32::from(p[0]) - 165).abs() <= 3, "face {face}: {p:?}");
        }
    }
}

#[test]
fn the_marker_chunk_changes_nothing_in_the_output() {
    let run = |test: &str, marker: Option<&str>| {
        let d = folder(test);
        let input = d.join("in/chrome_e.png");
        write_strip(&input, 32, Depth::Eight, CONTRACT, marker);
        let has_chunk = std::fs::read(&input).unwrap().windows(4).any(|w| w == b"iTXt");
        assert_eq!(has_chunk, marker.is_some(), "the input must really carry the chunk");
        let c = convert(&d, "chrome_e.dds", Target::Cs, &["format=rgba8"]);
        assert_eq!(c.summary.converted, 1, "{:#?}", c.log);
        std::fs::read(&c.job.output).unwrap()
    };
    let text = r#"{"tool":"Texalith","version":"0.0.0","map":"cube","pixels":"0123","settings":{"softness":0.6}}"#;
    assert!(run("plain", NO_MARKER) == run("marked", Some(text)));
}

#[test]
fn opaque_alpha_converts_and_real_alpha_stops_the_file() {
    let n = 16;
    let rgba = |alpha: &dyn Fn(usize) -> u8| -> Vec<u8> {
        let rgb = strip_bytes(n, Depth::Eight, CONTRACT);
        rgb.chunks(3).enumerate().flat_map(|(i, p)| [p[0], p[1], p[2], alpha(i)]).collect()
    };

    let d = folder("opaque");
    let px = rgba(&|_| 255);
    write_png(&d.join("in/chrome_e.png"), (6 * n, n), png::ColorType::Rgba, Depth::Eight, &px, NO_MARKER);
    let c = convert(&d, "chrome_e.dds", Target::Cs, &["format=rgba8"]);
    assert_eq!(c.summary.converted, 1, "{:#?}", c.log);
    assert_eq!(worst_error(&dds(&c.job), n), 0);

    let d = folder("alpha");
    let px = rgba(&|i| (i % 256) as u8);
    write_png(&d.join("in/chrome_e.png"), (6 * n, n), png::ColorType::Rgba, Depth::Eight, &px, NO_MARKER);
    let c = convert(&d, "chrome_e.dds", Target::Cs, &[]);
    assert!(
        matches!(&c.job.status, JobStatus::Blocked { codes } if codes.contains(&Code::LossAlphaData)),
        "{:?}",
        c.job.status
    );
    assert_eq!(c.summary.converted, 0);
}

#[test]
fn the_side_size_range_is_what_the_tab_offers() {
    let flat = |test: &str, n: u32| {
        let d = folder(test);
        let px = vec![128u8; (6 * n * n * 3) as usize];
        write_png(&d.join("in/chrome_e.png"), (6 * n, n), png::ColorType::Rgb, Depth::Eight, &px, NO_MARKER);
        convert(&d, "chrome_e.dds", Target::Cs, &["format=rgba8"])
    };
    for n in [4, 2048] {
        let c = flat(&format!("side{n}"), n);
        assert_eq!(c.summary.converted, 1, "{n}: {:#?}", c.log);
        assert_eq!(dds(&c.job).width, n);
    }

    // Below the range: refused, not written.
    let c = flat("side2", 2);
    assert_eq!(c.summary.converted, 0, "{:#?}", c.log);
    assert!(!c.job.output.exists());

    // Not a power of two: converts, but smaller than was asked for, so the tab must offer powers of two only.
    let c = flat("side48", 48);
    assert_eq!(c.summary.converted, 1, "{:#?}", c.log);
    assert_eq!(dds(&c.job).width, 32);
    assert!(c.job.warnings.iter().any(|w| w.code == Code::DimResized), "{:#?}", c.job.warnings);
}

/// All of Texalith's exports go in one folder, so the strip has to survive its neighbours.
#[test]
fn the_other_exports_beside_the_strip_leave_it_a_cube() {
    let d = folder("neighbours");
    write_strip(&d.join("in/chrome_e.png"), 32, Depth::Eight, CONTRACT, NO_MARKER);
    for name in ["chrome_n", "chrome_height", "chrome_parallax", "chrome_ao", "chrome_curvature", "chrome_cavity"] {
        let px = vec![128u8; 32 * 32 * 3];
        write_png(&d.join(format!("in/{name}.png")), (32, 32), png::ColorType::Rgb, Depth::Eight, &px, NO_MARKER);
    }
    let c = convert(&d, "chrome_e.dds", Target::Cs, &["format=rgba8"]);
    assert_eq!(c.job.cube.as_ref().unwrap().layout, CubeLayout::Hstrip, "{:#?}", c.job.warnings);
    assert_eq!(worst_error(&dds(&c.job), 32), 0, "{:#?}", c.log);
}

#[test]
fn the_base_name_decides_nothing_but_the_output_name() {
    let d = folder("names");
    write_strip(&d.join("in/My Odd.Name 01_e.png"), 16, Depth::Eight, CONTRACT, NO_MARKER);
    let c = convert(&d, "My Odd.Name 01_e.dds", Target::Cs, &[]);
    assert_eq!(c.summary.converted, 1, "{:#?}", c.log);
    assert!(dds(&c.job).is_cubemap);
}
