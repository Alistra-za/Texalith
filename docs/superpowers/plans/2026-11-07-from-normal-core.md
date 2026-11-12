# From normal core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Texalith's From normal outputs as a tested library: input classification, the master height at any power-of-two size up to 4096, Height data, Parallax map, Normal from height, AO, curvature, cavity, marked 16-bit PNGs and the overwrite check. No UI; plan 2 builds the tab on these functions.

**Architecture:** One `Master` per input and setting set holds the zero-mean height in texels, its slopes and the edge mode. Every output is a pure function of the master plus its own options, so an output's own slider (Depth, Reach, Line thickness and the rest) never re-solves. Flatten, Edges, Green and Output size are `MasterOpts` and rebuild the master. Plan 2 rebuilds at 1K while one of those is dragged and at full size on release, because spike 6 measured a full 4K solve at 2.1 s at most. The report's cached slope spectrum (one inverse FFT per Flatten change) is not built: `shortcut:` it if a 4K mirror drag feels slow. Files are bytes first (PNG with an iTXt marker), then checked against disk with Tessalith's `may_replace` rule and written atomically.

**Tech Stack:** Rust 1.99, edition 2021. `engine` (Tessalith, path dep) for `load::load_image`, `image::Rgba32F`, `resize::pot_dims`, `dds::{may_replace, write_atomic, Kept}`. `rayon` 1.12, `png` 0.18, `serde`/`serde_json`, `sha2` 0.10, `hex` 0.4. All are already in `Cargo.lock`.

**Spec:** `reports/Texalith generation modes and settings.md` (sections "What each output does", "Settings per tab → From normal", "Suffixes"), `reports/Rust material completer research.md` ("Overwrite and provenance"), `research_notes/Texalith generation modes and settings/settings_and_explanations.md:441-517` (Height data and Parallax rules), `research_notes/Texalith generation modes and settings/spike6_from_normal_timing/README.md` (size ceiling).

## Global Constraints

- Never edit anything under `F:\ai\dds`. `git -C F:\ai\dds\converter status --short --untracked-files=no` stays empty.
- Commit only when the user asks. The commit steps below run only on the user's yes; otherwise skip them.
- Builds and tests: `-j 2`, `--test-threads=2`, `--offline`, and only when no other `cargo` or `rustc` is running.
- One size ceiling for every output: `MAX_SIZE = 4096`. No `SOLVE_MAX`, `AO_MAX` or `HEIGHT_MAX`.
- Output sizes are powers of two, never above the source's power-of-two longer side.
- Every grey map is 16-bit linear PNG. Height data: true min to black, true max to white, no clipping.
- AO: GTAO, fixed 8 slices and 16 steps. Cavity: 8 slices, steps `min(radius, 8)`.
- Edges: Repeats (wrap) is the default. No silent auto choice; the seam detector is a hint only (seam ratio above 2).
- Normal maps: DirectX green unless the user picks OpenGL.
- Suffixes: `_height.png`, `_parallax.png`, `_n.png`, `_ao.png`, `_curvature.png`, `_cavity.png`. Never `_generated`.
- Marker: one iTXt chunk, keyword `Texalith`, JSON. Overwrite only files whose marker hash matches their pixels; anything else goes to the caller as foreign.
- New non-trivial logic gets a test. Known limits get a `shortcut: <limit>, <when to upgrade>` comment.

## Review Focus

1. A non-square, non-power-of-two source (1000×700): the solve still gets power-of-two sides, the aspect holds, and nothing panics. Test in Task 3.
2. A tiny source (1×1, 8×8): `sizes` returns one size, and the master builds without dividing by zero. Tests in Tasks 1 and 3.
3. A flat input (a constant normal, or a constant grey): span 0, Height data all 0.5, a Parallax map all at the flat level, curvature 0.5, AO 1, no NaN. Tests in Tasks 4 and 5.
4. A grey JPEG with small channel drift, and a BC5-style normal with B = 0: the first reads as height, the second as a normal. Test in Task 2.
5. A marked file someone edited (pixels changed, marker kept), and one with its marker stripped: both are foreign, never silently overwritten. Test in Task 7.

**Guessed tolerances.** These were not run: the AO downsample MAE `< 0.03`, the bump centre `> corner + 1.0`, the fit `< 2.0°`, the groove floor's cavity `< 0.9`, and `5.12 ± 0.1` for the height span. Tune each on its first run and write the measured value in a comment beside it. Never loosen a bound without that comment.

**Test helpers.** Names such as `bump_normal`, `noisy_normal`, `opengl`, `grey_ramp`, `half_cosine_normal`, `pits`, `groove`, `spiky_ramp`, `sine`, `master_of` and `no()` (an `AtomicBool` that is false) are small helpers the task writes in its own test module. Build on what `src/from_normal/solve.rs`'s `tests` module already has: `lcg`, `torus`, `bump(hf, w, h, k, wrap) -> Rgba32F` (a normal from a height), `range`, and the private `half_cosine` and `x_slopes`, which become `pub(crate)` when a task needs them.

---

### Task 1: Resampling copy and sizes

**Files:**
- Create: `src/resample.rs`
- Modify: `src/lib.rs` (add `pub mod resample;`), `src/from_normal/mod.rs`, `Cargo.toml`
- Test: in-file `#[cfg(test)]` modules

**Interfaces:**
- Produces: `resample::resample(img: Rgba32F, size: (u32, u32)) -> Rgba32F`; `from_normal::MAX_SIZE: u32 = 4096`; `from_normal::sizes(source: (u32, u32)) -> Vec<u32>`; `from_normal::dims(source: (u32, u32), long: u32) -> (u32, u32)`.

- [ ] **Step 1: Move `engine` to `[dependencies]` and add the direct deps**

In `Cargo.toml`, move the `engine` line from `[dev-dependencies]` to `[dependencies]` with the comment `# load_image, Rgba32F, pot_dims, may_replace, write_atomic (report: "What Texalith still needs from engine").` Add `png = "0.18"` (and drop it from dev-dependencies), `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `sha2 = "0.10"`, `hex = "0.4"`. Run `cargo check --offline -j 2`; expected: no new crates downloaded.

- [ ] **Step 2: Copy `area_resample` and `resample`**

Copy `axis_weights` and `area_resample` (`F:\ai\dds\converter\crates\engine\src\mips.rs:101-161`, the `weighted = false` path only) and `resample` (`crates\engine\src\rnm\blend.rs:62-100`) into `src/resample.rs` unchanged, with the header `//! Copied from Tessalith at 36384560 (GPL-3.0): engine's area_resample is crate-private, and rnm::blend goes in 3.0.`

- [ ] **Step 3: Write the failing tests**

```rust
#[test]
fn a_box_down_keeps_the_mean() { // 4×4 ramp 0..15 → 2×2: each texel is its 2×2 block mean
    let img = Rgba32F { width: 4, height: 4, px: (0..16).map(|i| [i as f32; 4]).collect() };
    let r = resample(img, (2, 2));
    assert_eq!(r.px.iter().map(|p| p[0]).collect::<Vec<_>>(), vec![2.5, 4.5, 10.5, 12.5]);
}
#[test]
fn sizes_are_powers_of_two_up_to_the_source_and_4096() {
    assert_eq!(sizes((1000, 700)), vec![256, 512, 1024]);
    assert_eq!(sizes((8192, 8192)), vec![256, 512, 1024, 2048, 4096]);
    assert_eq!(sizes((8, 8)), vec![8]);
    assert_eq!(sizes((1, 1)), vec![1]);
}
#[test]
fn dims_keep_the_power_of_two_aspect() {
    assert_eq!(dims((1000, 700), 1024), (1024, 512)); // pot_dims((1000,700)) = (1024, 512)
    assert_eq!(dims((2048, 512), 256), (256, 64));
    assert_eq!(dims((4096, 1), 256), (256, 1));
}
```

Check `engine::resize::pot_dims((1000, 700))` first and fix the expected value in the test if it differs; the test pins whatever `pot_dims` gives.

- [ ] **Step 4: Run, expect FAIL** — `cargo test --offline --lib -j 2 sizes_ dims_ a_box -- --test-threads=2`: `sizes` and `dims` not found.

- [ ] **Step 5: Implement `sizes` and `dims` in `src/from_normal/mod.rs`**

`sizes`: take `pot_dims(source)`'s longer side `L`; return `[256, 512, 1024, 2048, 4096]` filtered to `≤ L`, or `vec![L.min(MAX_SIZE)]` when that is empty. `dims`: Tessalith's `dims` (`engine/src/from_normal/mod.rs:218-222`) applied to `pot_dims(source)`.

- [ ] **Step 6: Run, expect PASS**, then the whole lib: `cargo test --offline --lib -j 2 -- --test-threads=2`.

- [ ] **Step 7: Commit (only on the user's yes)** — `git add Cargo.toml Cargo.lock src/resample.rs src/lib.rs src/from_normal/mod.rs` / `git commit -m "Add resampling and output sizes for From normal"`

---

### Task 2: Marker and input classification

**Files:**
- Create: `src/marker.rs`, `src/from_normal/input.rs`
- Modify: `src/lib.rs` (`pub mod marker;`), `src/from_normal/mod.rs` (`pub mod input;`)

**Interfaces:**
- Produces:
  - `marker::KEYWORD: &str = "Texalith"`
  - `#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)] #[serde(rename_all = "snake_case")] pub enum MapKind { HeightData, Parallax, Normal, Ao, Curvature, Cavity }`
  - `#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)] pub struct Marker { pub tool: String, pub version: String, pub kind: MapKind, pub pixels_sha256: String, pub source_path: String, pub source_sha256: String, pub settings: serde_json::Value, pub span_texels: Option<f64>, pub master_sha256: Option<String> }`
  - `marker::encode_gray16(width: u32, height: u32, px: &[f32], m: &Marker) -> Vec<u8>` and `marker::encode_rgb16(width: u32, height: u32, px: &[[f32; 4]], m: &Marker) -> Vec<u8>`: codes `round(clamp(v, 0, 1)·65535)`; they set `pixels_sha256` to the SHA-256 (lower-case hex) of the big-endian sample bytes before writing the chunk.
  - `marker::read(png: &[u8]) -> Option<Marker>`: the first iTXt chunk with `KEYWORD`, parsed; `None` on any failure.
  - `marker::is_ours(png: &[u8]) -> bool`: a marker exists and its `pixels_sha256` equals the hash of the decoded samples.
  - `input::Kind { Normal, Height { span_texels: Option<f64> }, Parallax }` and `input::classify(img: &Rgba32F, path: &Path, marker: Option<&Marker>) -> Kind`.

- [ ] **Step 1: Write the failing tests**

```rust
// marker.rs
#[test]
fn a_marker_round_trips_and_proves_the_pixels() {
    let png = encode_gray16(2, 2, &[0.0, 0.25, 0.5, 1.0], &sample(MapKind::HeightData));
    let m = read(&png).unwrap();
    assert_eq!(m.kind, MapKind::HeightData);
    assert_eq!(m.span_texels, Some(12.5));
    assert!(is_ours(&png));
}
#[test]
fn an_edited_or_stripped_file_is_not_ours() {
    let png = encode_gray16(2, 2, &[0.0, 0.25, 0.5, 1.0], &sample(MapKind::HeightData));
    let edited = encode_gray16(2, 2, &[0.0, 0.25, 0.5, 0.9], &sample(MapKind::HeightData));
    let forged = splice_marker(&edited, &read(&png).unwrap()); // test helper: re-writes `edited` with png's marker
    assert!(!is_ours(&forged));
    assert!(!is_ours(&strip_text_chunks(&png))); // test helper: decode and re-encode with the png crate, no text
}
// input.rs
#[test]
fn grey_with_jpeg_drift_is_height_and_bc5_style_is_normal() {
    let grey = flat_img(64, |i| { let v = (i % 7) as f32 / 7.0; [v, v + 1.5 / 255.0, v - 1.0 / 255.0, 1.0] });
    assert_eq!(classify(&grey, Path::new("rock.jpg"), None), Kind::Height { span_texels: None });
    let bc5 = flat_img(64, |i| [0.5 + 0.1 * ((i % 5) as f32 - 2.0) / 2.0, 0.5, 0.0, 1.0]);
    assert_eq!(classify(&bc5, Path::new("rock_n.png"), None), Kind::Normal);
}
#[test]
fn parallax_is_refused_by_marker_or_name() {
    let grey = flat_img(16, |_| [0.5, 0.5, 0.5, 1.0]);
    assert_eq!(classify(&grey, Path::new("x_parallax.png"), None), Kind::Parallax);
    assert_eq!(classify(&grey, Path::new("x.png"), Some(&sample(MapKind::Parallax))), Kind::Parallax);
    assert_eq!(classify(&grey, Path::new("x.png"), Some(&sample(MapKind::HeightData))), Kind::Height { span_texels: Some(12.5) });
}
```

`sample(kind)` builds a `Marker` with `span_texels: Some(12.5)` and fixed strings. `flat_img(n, f)` builds an `n×n` `Rgba32F` from `f(index)`.

- [ ] **Step 2: Run, expect FAIL** — `cargo test --offline --lib -j 2 marker:: input:: -- --test-threads=2`.

- [ ] **Step 3: Implement `marker.rs`** with the `png` crate's `Encoder::add_itxt_chunk` and `Decoder` `info().utf8_text` (16-bit `ColorType::Grayscale` / `Rgb`, `BitDepth::Sixteen`). `tool` is `"Texalith"`, `version` is `env!("CARGO_PKG_VERSION")`.

- [ ] **Step 4: Implement `classify`** in this order: marker kind `Parallax` or a file stem ending `_parallax` (ASCII case-insensitive) → `Parallax`. Marker kind `HeightData` → `Height { span_texels: marker.span_texels }`. Every texel's `max(|R−G|, |G−B|) ≤ 2/255` → `Height { span_texels: None }`. Otherwise → `Normal`.

- [ ] **Step 5: Run, expect PASS.**

- [ ] **Step 6: Commit (only on the user's yes)** — `git commit -m "Add the PNG marker and From normal input classification"`

---

### Task 3: The master height

**Files:**
- Create: `src/from_normal/master.rs`, `src/from_normal/detect.rs`
- Modify: `src/from_normal/mod.rs` (`mod detect; pub mod master;`), `src/from_normal/ao.rs` (make `central` `pub(super)`)

**Interfaces:**
- Consumes: `slopes`, `solve`, `fit`, `tilt_deg` (existing), `resample::resample`, `dims`, `input::Kind`.
- Produces:
  - `pub enum Edges { Repeats, DoesNotRepeat }` (`Repeats` is `Default`), `pub enum Green { Directx, Opengl }`
  - `pub struct MasterOpts { pub size: u32, pub edges: Edges, pub green: Option<Green>, pub flatten: f32, pub normal_depth_pct: f32 }`. Defaults: `flatten 0.3`, `normal_depth_pct 1.0`, `green None` (guess), `edges Repeats`.
  - `pub struct Master { pub width: u32, pub height: u32, pub wrap: bool, pub h: Vec<f64>, pub slopes: Slopes, pub from_normal: bool, pub report: MasterReport }`. `h` is zero-mean, in texels at `width`.
  - `pub struct MasterReport { pub green: Green, pub green_guessed: bool, pub green_sure: bool, pub seam_ratio: f32, pub seam_hint: bool, pub tilt_deg: f32, pub fit_mean_deg: Option<f32>, pub fit_p95_deg: Option<f32>, pub span_known: bool }`
  - `pub enum BuildError { Parallax, BadOpts(&'static str), Cancelled }`. `Parallax`'s `Display` is the exact copy: "This is a parallax map, which is made from height data and cannot be turned back. Load its height data, or build it again from the normal or image."
  - `pub fn build(img: &Rgba32F, kind: &Kind, o: &MasterOpts, cancel: &AtomicBool) -> Result<Master, BuildError>`
  - `detect::seam_ratio(s: &Slopes) -> f64` (copy of Tessalith's `detect_tiling` ratio, `engine/src/from_normal/solve.rs:9-37`) and `detect::guess_green(s: &Slopes, wrap: bool) -> (Green, bool /*sure*/)` (Tessalith's block at `engine/src/from_normal/mod.rs:461-482`: relief 0, `GREEN_MARGIN 1.25`, DirectX when unsure).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_bump_normal_builds_a_bump_at_the_asked_size() {
    let n = bump_normal(256); // test helper from solve::tests, DirectX
    let m = build(&n, &Kind::Normal, &MasterOpts { size: 128, ..Default::default() }, &no()).unwrap();
    assert_eq!((m.width, m.height), (128, 128));
    let c = m.h[64 * 128 + 64];
    assert!(c > m.h[0] + 1.0, "centre {c} must stand above the corner {}", m.h[0]); // texels, at the asked size
    assert!(m.report.fit_mean_deg.unwrap() < 2.0);
}
#[test]
fn an_odd_sized_source_solves_on_power_of_two_sides() {
    let n = noisy_normal(1000, 700);
    let m = build(&n, &Kind::Normal, &MasterOpts { size: 512, ..Default::default() }, &no()).unwrap();
    assert_eq!((m.width, m.height), (512, 256));
    assert!(m.h.iter().all(|v| v.is_finite()));
}
#[test]
fn a_tiny_source_builds() {
    for side in [1, 2, 8] {
        let n = noisy_normal(side, side);
        let m = build(&n, &Kind::Normal, &MasterOpts { size: side, ..Default::default() }, &no()).unwrap();
        assert!(m.h.iter().all(|v| v.is_finite()));
    }
}
#[test]
fn an_opengl_normal_is_guessed_and_flipped() {
    let m = build(&opengl(&bump_normal(256)), &Kind::Normal, &MasterOpts { size: 256, ..Default::default() }, &no()).unwrap();
    assert_eq!((m.report.green, m.report.green_guessed, m.report.green_sure), (Green::Opengl, true, true));
    assert!(m.h[128 * 256 + 128] > m.h[0]);
}
#[test]
fn a_height_input_uses_its_span_or_normal_depth() {
    let g = grey_ramp(256); // 0 at the left column to 1 at the right
    let m = build(&g, &Kind::Height { span_texels: Some(20.0) }, &MasterOpts { size: 128, ..Default::default() }, &no()).unwrap();
    assert!((range(&m.h) - 10.0).abs() < 0.2, "a 20-texel span at 256 is 10 texels at 128");
    let m = build(&g, &Kind::Height { span_texels: None }, &MasterOpts { size: 256, normal_depth_pct: 2.0, ..Default::default() }, &no()).unwrap();
    assert!((range(&m.h) - 5.12).abs() < 0.1, "2 % of 256 texels");
    assert!(!m.report.span_known);
}
#[test]
fn a_parallax_input_is_refused() {
    assert!(matches!(build(&grey_ramp(16), &Kind::Parallax, &MasterOpts::default(), &no()), Err(BuildError::Parallax)));
}
#[test]
fn a_cut_tile_raises_the_seam_hint() {
    let n = half_cosine_normal(256); // solve::tests::half_cosine: does not tile
    let m = build(&n, &Kind::Normal, &MasterOpts { size: 256, ..Default::default() }, &no()).unwrap();
    assert!(m.wrap, "Repeats stays the choice");
    assert!(m.report.seam_hint && m.report.seam_ratio > 2.0);
}
```

- [ ] **Step 2: Run, expect FAIL** — `cargo test --offline --lib -j 2 master:: -- --test-threads=2`.

- [ ] **Step 3: Implement `build`**

Validate: `size` a power of two in `1..=MAX_SIZE` and `≤ sizes(source)`'s maximum; `flatten` in `0..=1`; `normal_depth_pct` in `0.05..=5`. Otherwise `BadOpts`.

The normal path:
1. Slopes at full size.
2. Area- or bilinear-resample `p` and `q` to `dims(source, size)` with `resample::resample`, packing `[p, q, 0, 0]` as Tessalith's `resize` does (`engine/src/from_normal/mod.rs:225-247`). Keep the full-size tilt.
3. Seam ratio at `dims(source, 1024.min(longer))`.
4. Green guess at `dims(source, 512.min(longer))`, unless `green` is given.
5. Flip `q` for OpenGL.
6. `solve(&s, wrap, flatten)` and `fit`.

The height path:
1. Take R, resample to `dims`.
2. `span = span_texels·(width / source_width)` when known, else `normal_depth_pct / 100 · width`.
3. `h = (D − mean D)·span`.
4. `slopes = ao::central(&h, …, wrap)`. Seam ratio on those. Green is `Directx`, not guessed.

Check `cancel` between stages.

- [ ] **Step 4: Run, expect PASS**, then the whole lib.

- [ ] **Step 5: Commit (only on the user's yes)** — `git commit -m "Build the From normal master at any power-of-two size up to 4096"`

---

### Task 4: Height data, Parallax map, Normal from height

**Files:**
- Create: `src/from_normal/maps.rs`
- Modify: `src/from_normal/mod.rs` (`pub mod maps;`)

**Interfaces:**
- Consumes: `master::{Master, Green}`.
- Produces:
  - `pub struct Gray { pub width: u32, pub height: u32, pub px: Vec<f32> }`
  - `pub fn height_data(m: &Master) -> (Gray, f64)`: `D = (h − min)/(max − min)` and the span `max − min` in texels. When the span is below `1e-12`, every texel is 0.5 and the span is 0.
  - `pub enum FlatLevel { MidGrey, White }` (`MidGrey` is `Default`), `pub const MARGIN: f32 = 8.0 / 255.0`
  - `pub struct ParallaxOpts { pub depth: f32, pub flat: FlatLevel, pub smooth_texels: f32, pub invert: bool }`. Defaults: `depth 0.9`, `MidGrey`, `0.0`, `false`. Ranges: `depth 0.1..=0.9`, `smooth_texels 0..=8`.
  - `pub struct ParallaxReport { pub times_normal: f32, pub shader_depth_pct: f32, pub levels_used: u32, pub clipped_share: f32 }`
  - `pub fn parallax(m: &Master, o: &ParallaxOpts) -> (Gray, ParallaxReport)`
  - `pub fn match_depth(m: &Master) -> f32`: the depth whose gain is `10 / width` (the preview's full range is 0.1 UV at HeightScale 1), clamped to `0.1..=0.9`.
  - `pub fn normal_from_height(m: &Master, green: Green) -> Vec<[f32; 4]>`: Scharr 3-10-3 over `m.h` (wrapped when `m.wrap`, else clamped edges), `n ∝ (−p, −q, 1)`, encoded `n·0.5 + 0.5`, G flipped for OpenGL, alpha 1.

The Parallax map (only this algorithm is spelled out, because the spec leaves the knee open):

```
h' = m.h, Gaussian-blurred with σ = smooth_texels when that is above 0 (wrap-aware)
med = median(h'), r = p99.5(|h' − med|); if r < 1e-12: every texel at the flat level (0.5 or 1.0), report levels_used 1
g = depth · 0.5 / r                                    // depth 0.9 puts p99.5 at ±0.45, as Tessalith
knee(x, a, l) = x                          when x ≤ a
              = a + (l − a)·tanh((x − a)/(l − a))   otherwise
MidGrey: x = g·(h' − med);  P = 0.5 + sign(x)·knee(|x|, 0.5·depth, 0.5 − MARGIN)
White:   top = p99.5(h');  y = max(g·(top − h'), 0);  P = 1 − knee(y, depth, 1 − MARGIN)
invert:  P = 1 − P
clipped_share  = share of texels where the knee changed x (or y), or y was raised to 0
levels_used    = distinct round(P·255)
times_normal   = g / (10 / width)
shader_depth_pct = 100 / (g · width)                    // black-to-white span as % of width
```

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn height_data_spans_black_to_white_and_reports_texels() {
    let (g, span) = height_data(&master_of(&[-2.0, 0.0, 1.0, 3.0], 2, 2));
    assert_eq!(g.px, vec![0.0, 0.4, 0.6, 1.0]);
    assert_eq!(span, 5.0);
}
#[test]
fn a_flat_master_gives_flat_maps() {
    let m = master_of(&[0.0; 16], 4, 4);
    assert!(height_data(&m).0.px.iter().all(|&v| v == 0.5));
    let (p, r) = parallax(&m, &ParallaxOpts::default());
    assert!(p.px.iter().all(|&v| v == 0.5) && r.levels_used == 1);
    let (w, _) = parallax(&m, &ParallaxOpts { flat: FlatLevel::White, ..Default::default() });
    assert!(w.px.iter().all(|&v| v == 1.0));
}
#[test]
fn parallax_pivots_on_the_median_and_stays_in_the_margin() {
    let m = master_of(&spiky_ramp(64), 64, 64); // a ramp plus 0.2 % spikes of 50x the ramp
    let (p, r) = parallax(&m, &ParallaxOpts::default());
    let med = median_f32(&p.px);
    assert!((med - 0.5).abs() < 1.0 / 255.0);
    assert!(p.px.iter().all(|&v| (MARGIN..=1.0 - MARGIN).contains(&v)));
    assert!(r.clipped_share > 0.0 && r.clipped_share < 0.01);
}
#[test]
fn white_puts_the_top_at_one_and_sinks_the_rest() {
    let (p, _) = parallax(&master_of(&spiky_ramp(64), 64, 64), &ParallaxOpts { flat: FlatLevel::White, ..Default::default() });
    assert!(p.px.iter().all(|&v| v <= 1.0 && v >= MARGIN));
    assert!(p.px.iter().filter(|&&v| v == 1.0).count() as f32 / p.px.len() as f32 >= 0.005);
}
#[test]
fn match_depth_gives_one_times_the_normal() {
    let m = master_of(&sine(256, 4.0 /*texels amplitude*/), 256, 256);
    let d = match_depth(&m);
    let (_, r) = parallax(&m, &ParallaxOpts { depth: d, ..Default::default() });
    if (0.1..0.9).contains(&d) { assert!((r.times_normal - 1.0).abs() < 1e-3); }
    assert!((r.shader_depth_pct - 100.0 / (r.times_normal * 10.0)).abs() < 1e-3);
}
#[test]
fn normal_from_height_round_trips_through_slopes() {
    let m = master_of(&sine(256, 3.0), 256, 256);
    let n = normal_from_height(&m, Green::Directx);
    let back = crate::from_normal::slopes(&n, 256, 256, false);
    // Scharr keeps cos(ω/2) of the slope (report, spike 4): at period 64 that is 0.9988.
    assert!(max_abs_diff(&back.p, &scharr_p(&m)) < 1e-3);
    let gl = normal_from_height(&m, Green::Opengl);
    assert!((gl[300][1] - (1.0 - n[300][1])).abs() < 1e-6);
}
```

`master_of(h, w, hgt)` builds a `Master` with `wrap: true`, slopes from `ao::central`, and a default report. `scharr_p` is the test's own Scharr x-derivative.

- [ ] **Step 2: Run, expect FAIL.**
- [ ] **Step 3: Implement**, using `percentile` from `mod.rs` for p99.5 and the median (q 0.5).
- [ ] **Step 4: Run, expect PASS.**
- [ ] **Step 5: Commit (only on the user's yes)** — `git commit -m "Add Height data, Parallax map and Normal from height"`

---

### Task 5: AO, cavity and curvature with progress

**Files:**
- Modify: `src/from_normal/ao.rs` (progress), `src/from_normal/solve.rs` (curvature σ and contrast), `src/from_normal/maps.rs`, `src/from_normal/spike4.rs` and `examples/from_normal_timing.rs` (new `march` argument)

**Interfaces:**
- Consumes: `Master`, `Gray`.
- Produces:
  - `ao::march(h: &[f64], width: u32, height: u32, wrap: bool, o: &AoOpts, progress: &AtomicU32, cancel: &AtomicBool) -> Option<Vec<f32>>`: `progress` holds the finished share in per mille, stored once per slice row batch. This replaces the `shortcut:` at `ao.rs:4`.
  - `solve::curvature_lines(s: &Slopes, wrap: bool, sigma: f64, contrast: f64) -> Vec<f32>`: `curvature_with` at relief 0, with `sigma` in place of `CURVATURE_SIGMA` and `0.5 − 0.5·clamp(contrast·κ/Rκ, −1, 1)`. At relief 0 this is the divergence of the slopes, which is the report's "divergence of n.xy" (the curl part has no divergence). `curvature(s, wrap, relief)` keeps its behaviour, so the engine-equivalence test still passes.
  - `pub struct AoSettings { pub reach: f32, pub darkness: f32, pub surface_depth: f32 }`. Defaults `0.0625, 1.0, 1.0`. Ranges `0.015625..=0.25`, `0.5..=3`, `0.25..=4`.
  - `maps::ao(m: &Master, o: &AoSettings, long: u32, progress: &AtomicU32, cancel: &AtomicBool) -> Option<Gray>`: 8 slices, 16 steps. When `long < m.width.max(m.height)`, `m.h` is resampled to `dims((m.width, m.height), long)` and scaled by the size ratio first. That is the 1K preview while a slider moves.
  - `maps::cavity(m: &Master, crevice_texels: f32, surface_depth: f32, progress: &AtomicU32, cancel: &AtomicBool) -> Option<Gray>`: radius `crevice_texels / width`, 8 slices, steps `min(round(crevice_texels), 8)`. `crevice_texels` lies in `2..=16`; the default is `(width / 128).clamp(2, 16)`, exposed as `maps::default_crevice(width: u32) -> f32`.
  - `pub struct CurvatureOpts { pub line_texels: f32, pub contrast: f32 }`. Defaults `2.0, 1.0`. Ranges `1..=8`, `0.5..=2`.
  - `maps::curvature(m: &Master, o: &CurvatureOpts) -> Gray`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn ao_reports_progress_to_1000_and_stops_on_cancel() {
    let m = master_of(&pits(128), 128, 128);
    let p = AtomicU32::new(0);
    assert!(ao(&m, &AoSettings::default(), 128, &p, &no()).is_some());
    assert_eq!(p.load(Relaxed), 1000);
    assert!(ao(&m, &AoSettings::default(), 128, &AtomicU32::new(0), &AtomicBool::new(true)).is_none());
}
#[test]
fn a_small_ao_matches_the_full_one_downsampled() {
    let m = master_of(&pits(256), 256, 256);
    let full = ao(&m, &AoSettings::default(), 256, &AtomicU32::new(0), &no()).unwrap();
    let small = ao(&m, &AoSettings::default(), 128, &AtomicU32::new(0), &no()).unwrap();
    assert_eq!((small.width, small.height), (128, 128));
    assert!(mae(&box_down(&full), &small) < 0.03);
}
#[test]
fn flat_gives_white_ao_and_cavity_and_grey_curvature() {
    let m = master_of(&[0.0; 64 * 64], 64, 64);
    assert!(ao(&m, &AoSettings::default(), 64, &AtomicU32::new(0), &no()).unwrap().px.iter().all(|&v| v == 1.0));
    assert!(cavity(&m, 2.0, 1.0, &AtomicU32::new(0), &no()).unwrap().px.iter().all(|&v| v == 1.0));
    assert!(curvature(&m, &CurvatureOpts::default()).px.iter().all(|&v| v == 0.5));
}
#[test]
fn cavity_darkens_a_flat_floored_groove_that_curvature_leaves_grey() {
    // Floor centre 12 texels from each wall: inside the 16-texel cavity radius, past 3σ of curvature's σ 2.
    let m = master_of(&groove(128, 24 /*texels wide*/, 12.0 /*deep*/), 128, 128);
    let c = cavity(&m, 16.0, 1.0, &AtomicU32::new(0), &no()).unwrap();
    let k = curvature(&m, &CurvatureOpts::default());
    let floor = 64 * 128 + 64; // groove centre
    assert!(c.px[floor] < 0.9);
    assert!((k.px[floor] - 0.5).abs() < 0.05);
}
#[test]
fn curvature_lines_at_relief_0_is_the_slope_divergence() {
    let s = x_slopes(256, 256, |x| (x * 2.0 * PI / 32.0).sin()); // solve::tests helper
    let c = curvature_lines(&s, true, 2.0, 1.0);
    let peak = 16; // h = −cos(ωx)/ω peaks at ωx = π, x = 16 for period 32
    assert!(c[peak] > 0.9, "peaks bright");
    let more = curvature_lines(&s, true, 2.0, 2.0);
    assert!((more[peak + 4] - 0.5).abs() >= (c[peak + 4] - 0.5).abs());
}
```

- [ ] **Step 2: Run, expect FAIL.**
- [ ] **Step 3: Implement**, then update the `march` calls in `spike4.rs`, `examples/from_normal_timing.rs` and the `ao.rs` tests to pass `&AtomicU32::new(0)`.
- [ ] **Step 4: Run the whole lib and `cargo build --offline -j 2 --example from_normal_timing`, expect PASS.** `the_port_matches_tessalith_before_the_fix_and_the_march_change` must still pass.
- [ ] **Step 5: Commit (only on the user's yes)** — `git commit -m "Add AO, cavity and curvature outputs with AO progress"`

---

### Task 6: Output names and file bytes

**Files:**
- Create: `src/from_normal/files.rs`
- Modify: `src/from_normal/mod.rs` (`pub mod files;`)

**Interfaces:**
- Consumes: `marker::{encode_gray16, encode_rgb16, Marker, MapKind}`, `maps::Gray`.
- Produces:
  - `files::base_name(input: &Path) -> String`: the file stem with one trailing suffix removed, matched ASCII case-insensitively. The suffixes are `_n`, `_normal`, `_nrm`, `_height`, `_disp`, `_displacement`. `rock_N.png` gives `rock`, and `rock.png` gives `rock`.
  - `files::out_name(base: &str, kind: MapKind) -> String`: `{base}_height.png`, `_parallax.png`, `_n.png`, `_ao.png`, `_curvature.png`, `_cavity.png`.
  - `files::Source { pub path: PathBuf, pub sha256: String }` and `files::source(path: &Path) -> std::io::Result<Source>` (the SHA-256 of the file bytes).
  - `files::gray_png(g: &Gray, kind: MapKind, src: &Source, settings: serde_json::Value, span_texels: Option<f64>, master_sha256: Option<String>) -> Vec<u8>` and `files::normal_png(width: u32, height: u32, px: &[[f32; 4]], src: &Source, settings: serde_json::Value) -> Vec<u8>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn names_strip_one_known_suffix() {
    assert_eq!(base_name(Path::new("C:/m/Rock_N.png")), "Rock");
    assert_eq!(base_name(Path::new("bark_height.tif")), "bark");
    assert_eq!(base_name(Path::new("brick_n_n.png")), "brick_n");
    assert_eq!(out_name("bark", MapKind::Parallax), "bark_parallax.png");
}
#[test]
fn a_height_png_carries_its_span_and_parallax_its_master() {
    let g = Gray { width: 2, height: 1, px: vec![0.0, 1.0] };
    let h = gray_png(&g, MapKind::HeightData, &src(), json!({}), Some(7.5), None);
    assert_eq!(marker::read(&h).unwrap().span_texels, Some(7.5));
    let master = marker::read(&h).unwrap().pixels_sha256;
    let p = gray_png(&g, MapKind::Parallax, &src(), json!({}), None, Some(master.clone()));
    assert_eq!(marker::read(&p).unwrap().master_sha256, Some(master));
}
```

- [ ] **Step 2: Run, expect FAIL.** **Step 3: Implement.** **Step 4: Run, expect PASS.**
- [ ] **Step 5: Commit (only on the user's yes)** — `git commit -m "Add From normal output names and marked PNG bytes"`

---

### Task 7: Overwrite check and all-or-nothing write

**Files:**
- Create: `src/export.rs`
- Modify: `src/lib.rs` (`pub mod export;`)
- Test: `tests/export.rs` (uses a temp folder under `std::env::temp_dir()`, removed at the end)

**Interfaces:**
- Consumes: `engine::dds::{may_replace, write_atomic, Kept}`, `marker::is_ours`.
- Produces:
  - `pub struct Planned { pub path: PathBuf, pub bytes: Vec<u8> }`
  - `pub enum Verdict { New, Same, Ours, Foreign, Unreadable(String) }`
  - `pub fn check(p: &Planned) -> Verdict`. `New` means the path is absent and `Same` means equal bytes; both come before `may_replace`. Otherwise `may_replace(path, bytes, is_ours)`: `Ok` gives `Ours`, `Kept::Foreign` gives `Foreign`, and `Kept::Unreadable(e)` gives `Unreadable(e.to_string())`.
  - `pub fn write(plan: &[Planned], overwrite_foreign: &[bool], backup: bool) -> Result<Vec<PathBuf>, String>`. Rules:
    - `overwrite_foreign[i]` is read only for `Foreign` entries; `false` keeps the file and skips it.
    - `Same` is skipped, and `Unreadable` is always kept.
    - With `backup`, each foreign file being overwritten is first copied to `<dir>/_sdc/texalith-backup/<name>`.
    - Each file goes through `write_atomic`. If a later file fails, every file already written this call is restored from its previous bytes (or removed when it was `New`), and the error names the failing path.
    - The result lists the paths written.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn the_check_follows_the_marker_rule() {
    let d = temp("check");
    let ours = files::gray_png(&gray(), MapKind::Ao, &src(), json!({}), None, None);
    let p = |name: &str| d.join(name);
    assert!(matches!(check(&Planned { path: p("a_ao.png"), bytes: ours.clone() }), Verdict::New));
    fs::write(p("a_ao.png"), &ours).unwrap();
    assert!(matches!(check(&Planned { path: p("a_ao.png"), bytes: ours.clone() }), Verdict::Same));
    let newer = files::gray_png(&gray_other(), MapKind::Ao, &src(), json!({}), None, None);
    assert!(matches!(check(&Planned { path: p("a_ao.png"), bytes: newer.clone() }), Verdict::Ours));
    fs::write(p("a_ao.png"), edited_keeping_marker(&ours)).unwrap();
    assert!(matches!(check(&Planned { path: p("a_ao.png"), bytes: newer.clone() }), Verdict::Foreign));
    fs::write(p("a_ao.png"), plain_png()).unwrap();
    assert!(matches!(check(&Planned { path: p("a_ao.png"), bytes: newer }), Verdict::Foreign));
}
#[test]
fn foreign_is_kept_unless_asked_and_backed_up_when_overwritten() {
    let d = temp("keep");
    fs::write(d.join("a_n.png"), plain_png()).unwrap();
    let plan = [Planned { path: d.join("a_n.png"), bytes: marked() }];
    assert!(write(&plan, &[false], true).unwrap().is_empty());
    assert_eq!(fs::read(d.join("a_n.png")).unwrap(), plain_png());
    write(&plan, &[true], true).unwrap();
    assert_eq!(fs::read(d.join("_sdc/texalith-backup/a_n.png")).unwrap(), plain_png());
}
#[test]
fn a_failed_write_rolls_back_the_files_before_it() {
    let d = temp("roll");
    fs::write(d.join("a_ao.png"), marked()).unwrap();
    fs::create_dir(d.join("a_cavity.png")).unwrap(); // a directory: the rename fails
    let plan = [
        Planned { path: d.join("a_ao.png"), bytes: marked_other() },
        Planned { path: d.join("a_curvature.png"), bytes: marked() },
        Planned { path: d.join("a_cavity.png"), bytes: marked() },
    ];
    let err = write(&plan, &[false; 3], false).unwrap_err();
    assert!(err.contains("a_cavity.png"));
    assert_eq!(fs::read(d.join("a_ao.png")).unwrap(), marked());
    assert!(!d.join("a_curvature.png").exists());
}
```

- [ ] **Step 2: Run, expect FAIL** — `cargo test --offline -j 2 --test export -- --test-threads=2`.
- [ ] **Step 3: Implement.** Copy the rollback idea from Tessalith's `write_all` (`crates\gui\src\model\from_normal.rs:939-988`); read it first, then write it fresh.
- [ ] **Step 4: Run, expect PASS**, then the full suite: `cargo test --offline -j 2 -- --test-threads=2`.
- [ ] **Step 5: Commit (only on the user's yes)** — `git commit -m "Add the overwrite check and all-or-nothing export"`

---

## Not in this plan (plan 2: the From normal tab)

Opening a file, the tab layout and help text, the Output size dropdown and the other controls, the 1K-during-drag AO switch, the Repeat 2x2 view, presets and Compare, the overwrite dialog (`egui::Modal`, Keep is the default, Enter never overwrites), progress and cancel in the UI, "Out of date" on the Parallax row, the Normal output's Advanced options (Add broad shape, Smooth the source, Use real size) and its 8-bit source warning, and feeding the generated maps into the preview slots (`p` to slot 3; AO into t5's B with R 255, G 0, A 255).
