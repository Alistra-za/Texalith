# Cubemap tab Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Cubemap tab, part 1: open a panorama, `.hdr`, `.exr`, mirror ball, angular map, single photo, cross, strip or six face files; set Source type, Reflection softness and Brightness; see the six faces as a labelled cross; save one marked 16-bit RGB `_e.png` 6:1 strip that Tessalith converts to a cube DDS. A last task tries a vanilla environment-mapped ball in the 3D preview.

**Architecture:** Tasks 1-3 are library code. `src/cube/import.rs` copies Tessalith's cube importers (crosses, strips, six faces, panorama, mirror ball, angular, photo), because Tessalith 3.0 removes them; it keeps using `engine::cube::{Cube, cube_dir, dir_at, dir_to_face, latlong, world}`, `engine::cube::sample::{fetch, mip_chain}` and `engine::ops` game gamma, which stay. `src/cube/make.rs` picks the face size, applies the copied cone blur, then Brightness, and encodes the strip in game gamma. A worker thread where the newest job wins, as in RNM, does the import and the blur. Tasks 4-5 add the column, the "Cubemap" tab and the labelled 2D cross. Task 6 is a spike: the 3D ball switches to a vanilla `ENVMAP` material reading the cube in slot 4. If it fails, the tab ships with the 2D cross only.

**Tech Stack:** Rust 1.99, edition 2021, eframe/egui 0.36.2, egui_kittest 0.36.2, `rfd` 0.17.2, rayon, `engine` (`cube`, `load`, `ops`, `dds`, `config::Format`), `nif::material::MaterialState`, `render3d`, Texalith's `marker`, `save`, `export`, `resample` and `tab` modules.

**Spec:** `reports/Texalith generation modes and settings.md`:
- line 15: decision 3, the preview is 2D labelled faces in v1, with an environment-mapped ball as a spike labelled as not True PBR.
- line 24: the cube strip row.
- lines 165-173: the three main settings, their help text, and the Advanced list (Advanced is plan 6).
- lines 299-301: why True PBR cannot show the cube.

`reports/Rust material completer research.md`:
- lines 59-109: the `_e.png` contract.
- lines 113-124: the tab's inputs, algorithm, what transfers, what is new, what not to build.

User decisions (2026-11-09): the Cubemap tab is split into plan 5 (this plan) and plan 6 (the 10 Advanced items). A source over 8192 on a side is read, then halved until it fits. The vanilla ball is the last task of this plan.

The code this copies is Tessalith's `crates/engine/src/cube/import_dir.rs` (159 lines), `cube/layout.rs:11-161` (detect, cells, crop, rotate, the flat-cell check, import), `cube/sample.rs:62-79` (`build`) and `:135-200` (`blur`), and `cube/plan.rs:14` (`face_tag`). Pins to port come from `crates/engine/tests/cube.rs` (line numbers in each task). `crates/gui/src/model/cube_cross.rs` gives `FACE_LABELS` and `cross_image`.

## Global Constraints

- Never edit anything under `F:\ai\dds`. `git -C F:\ai\dds\converter status --short --untracked-files=no` stays empty.
- Commit only when the user asks. The commit steps below run only on the user's yes; otherwise skip them. No attribution lines.
- Cargo: `--offline -j 2`, tests with `-- --test-threads=2`, at BelowNormal priority, only when no other cargo or rustc process runs. Recreate the `cargo.ps1` helper in this plan's workspace (it waits for other cargo/rustc, runs at BelowNormal, logs to a file and ends the log with `EXIT n`; pass the cargo arguments as one string).
- Never run a GPU test while `toolbag.exe` is running. Scan every new or changed file for NUL bytes before a task is done.
- Each copied file starts with one line naming its Tessalith source file. Copied code keeps its tests, error handling and `shortcut:` comments.
- The strip contract: `<base>_e.png`, one 16-bit RGB PNG (no alpha), exactly 6:1, faces left to right +X −X +Y −Y +Z −Z in D3D addressing as `engine::cube::cube_dir`, values `linear^(1/1.6)` clamped to 0..1 (`engine::ops::linear_to_game_gamma`), face edge a power of two from 4 to 2048, one Texalith iTXt marker.
- Sources: 8- and 16-bit files are decoded with `engine::ops::game_gamma_to_linear` on RGB. Float files (`SourceInfo::is_float`, `.hdr` and `.exr`) are linear already. A source whose larger side is over 8192 is halved with `crate::resample::resample` until it is 8192 or less.
- Settings and defaults:
  - Source type: detected from shape, else none ("Pick one").
  - Reflection softness: stored 0 to 1, shown as the blur cone's half-angle, 0° to 45° (`value · 45`). The default is 0, or 0.6 for Single photo.
  - Brightness: -10 to +10 stops, default 0. It multiplies linear RGB by `2^stops` after the blur.
- Face size (plan 6 adds the Side size override): 128 when softness is below 0.1, else 64. It is capped at the largest power of two not above the source's cell edge, and is never below 4.
- Labels and text, verbatim:
  - Tab: "Cubemap". Open button: "Open source…". Empty state: "Open a panorama, cross, strip, mirror ball, angular map, photo or one of six face files to make a cube."
  - Source type help: "Tells the tool how your picture wraps around the sphere, and a wrong choice scrambles the reflection; change it if the labelled preview looks wrong or the image is square."
  - Reflection softness help: "Blurs the reflection evenly with no seams; raise it for rough metal or a low-detail source, and lower it for mirror-like surfaces."
  - Brightness help: "Makes all reflected light brighter or darker, and +1 doubles it; change it if reflections look dim or overpower the surface."
  - Source type names: "Panorama (2:1)", "Horizontal cross (4:3)", "Vertical cross (3:4)", "Horizontal strip (6:1)", "Vertical strip (1:6)", "Six face files", "Mirror ball", "Angular map", "Single photo". Undetected: "Pick one".
  - Face labels: "+X east", "−X west", "+Y north", "−Y south", "+Z up", "−Z down" (U+2212 minus).
  - Wrong shape: "{file} is {w}x{h}, which does not fit {type name}."
  - Missing faces: "{file} is one of six face files, but {list} is missing." (`{list}` names the missing files, comma-separated.)
  - Unequal faces: "The six face files are not equal squares."
  - Collision: "The output, {file}, would replace the source, so choose another folder."
  - Centre with no output: "Open a source to see its faces here."
  - Task 6 3D label: "Vanilla environment map, not True PBR".
- Not built here: the 10 Advanced items (plan 6), the slider band (the spec's band values were never measured), a GGX mip chain, cube alpha, the 1x1 marker.
- Panels follow plans 2 and 3: left "column" with the tab bar, `ScrollArea::both().id_salt("cube-column")`, `TextWrapMode::Truncate`, `ScrollStyle::thin()`, Save in a bottom sub-panel "cube-save". Errors use `tab::error_line`, warnings `tab::warn`.
- Enter never saves or overwrites. Generated files go only into the selected folder, by default the source's folder.
- New non-trivial logic gets a test. Known limits get a `shortcut: <limit>, <when to upgrade>` comment.

## Review Focus

1. A square image, or a near-miss ratio such as 1025x512: no guess is made, Source type shows "Pick one", and no job runs until one is picked. Tests: Task 1, `detect_takes_exact_ratios_only`; Task 4, `a_square_image_asks_for_a_source_type`.
2. A photo shaped like a cross (4:3 with non-flat corners): it is not taken as a cross. Test: Task 1, `a_photo_shaped_like_a_cross_is_not_detected`.
3. An HDR source with values far above 1: the strip clips at 1 and every value stays finite. Test: Task 2, `bright_sources_clip_and_stay_finite`.
4. A picked type that does not fit the image (Horizontal strip on a 2:1 image): the status names the file and its size, and no job is sent. Test: Task 4, `a_type_that_does_not_fit_is_refused`.
5. Six face files with one missing: the status names the missing file and nothing opens. Test: Task 3, `a_missing_face_file_is_named`.

---

### Task 1: Importing a source into six faces

**Files:**
- Create: `src/cube/mod.rs`, `src/cube/import.rs`
- Modify: `src/lib.rs` (`pub mod cube;`)
- Test: unit tests in `src/cube/import.rs`

**Interfaces:**
- Consumes: `engine::cube::{Cube, dir_at, cube_dir, dir_to_face, latlong::dir_to_uv, world::to_world}`, `engine::image::Rgba32F`.
- Produces, in `src/cube/mod.rs`:
  - `#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize)] #[serde(rename_all = "snake_case")] pub enum SourceType { Panorama, HCross, VCross, HStrip, VStrip, Faces, MirrorBall, Angular, Photo }`, with `ALL` and `pub fn name(self) -> &'static str` (the names in Global Constraints).
- Produces, in `src/cube/import.rs`:
  - `pub fn detect(img: &Rgba32F) -> Option<SourceType>`
  - `pub fn fits(t: SourceType, w: u32, h: u32) -> bool`
  - `pub fn cell(t: SourceType, w: u32, h: u32) -> u32`: the source's cell edge.
  - `pub fn import(img: &Rgba32F, t: SourceType, n: u32) -> Cube`: every type except `Faces`.
  - `pub fn faces(imgs: &[Rgba32F; 6], n: u32) -> Result<Cube, String>`
  - `pub fn face_tag(stem: &str) -> Option<(&str, usize)>`
  - `pub const HCROSS: [(u32, u32); 6]` (copied: Tessalith 3.0 removes `layout.rs`)
  - `pub(crate) fn build(size: u32, f: impl Fn(usize, u32, u32) -> [f32; 4] + Sync) -> Cube`

- [ ] **Step 1: Write the failing tests**

Port these pins from `crates/engine/tests/cube.rs`, with Texalith's names and signatures:

| Texalith test | Ported from | Checks |
|---|---|---|
| `detect_takes_exact_ratios_only` | `:73`, `:1378` | 4:3 HCross, 3:4 VCross, 6:1 HStrip, 1:6 VStrip, 2:1 Panorama; square, 1025x768, 4096x2049, 6145x1024 and 1024x2048 give `None` |
| `every_cross_and_strip_round_trips_the_arrow_cube` | `:91` | six faces with distinct arrows, laid out as each of the 4 layouts, `import` at the cell size gives the same six faces |
| `vcross_negz_is_turned_upright` | `:107` | the −Z cell of a vertical cross is stored upside down and comes out upright |
| `a_photo_shaped_like_a_cross_is_not_detected` | `:168` | a 4:3 image whose unused cells are not flat (channel range over 2/255): `detect` gives `None` |
| `panorama_mapping_matches_the_in_game_check` | `:227` | u .25 is north, .5 east, .75 south, 0 west; row 0 is up |
| `mirror_ball_centre_is_behind_the_camera` | `:1350` | ball centre = south; tolerance 0.03 |
| `angular_centre_is_forward` | `:1367` | centre = north |
| `photo_panorama_is_seamless_and_mirrored` | `:1398` | as ported |
| `face_tags_parse_both_spellings` | `:484` | `_posx`…`_negz` and `_px`…`_nz`, any case, index +X −X +Y −Y +Z −Z |
| `six_unequal_faces_are_refused` | new | `faces` with one 8x8 among 16x16 gives `Err("The six face files are not equal squares.")` |

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --offline -j 2 --lib -- --test-threads=2 cube::`
Expected: compile errors, the `cube` module does not exist.

- [ ] **Step 3: Implement**

- Copy `import_dir.rs` (bilinear, `project` with its 3x3 taps, panorama, `photo_panorama`, mirror ball, angular) and `sample::build`.
- Copy from `layout.rs`: `detect`, `cells`, `crop`, `rotate` and the flat-cell check. `detect` runs the flat-cell check for crosses and returns `None` when it fails.
- `import` for a cross or strip crops the six cells, turns the vertical cross's −Z cell 180°, and resamples each face to `n` with `crate::resample::resample` when the cell is larger.
- `cell`: crosses and strips give the cell edge; Panorama `w/4`; Photo, Mirror ball and Angular `w/2`; Faces the face edge.
- `fits`: crosses and strips need exact cells. Panorama, Mirror ball, Angular and Photo take any non-empty image. Faces never fits a single image.
- Copy `face_tag` from `plan.rs:14`.
- Drop `CubemapSpec` from every signature. `face_rotate`, `face_flip` and the −Z switch are plan 6.

- [ ] **Step 4: Run them to verify they pass**

Run: the Step 2 command.
Expected: 10 passed.

- [ ] **Step 5: Commit (only on the user's yes)**

```bash
git add src/lib.rs src/cube/mod.rs src/cube/import.rs
git commit -m "Texalith: cube importers copied from Tessalith"
```

### Task 2: Making the strip

**Files:**
- Create: `src/cube/make.rs`, `src/cube/files.rs`
- Modify: `src/marker.rs` (`MapKind::Cube`, serialised `"cube"`), `src/cube/mod.rs` (`Settings`), `src/view2d.rs` and `src/from_normal/files.rs`/`settings.rs` (the new `MapKind` arm is unreachable there, as `Rnm` is)
- Test: unit tests in `src/cube/make.rs` and `src/cube/files.rs`; `tests/cube_contract.rs`

**Interfaces:**
- Consumes: Task 1's `SourceType`, `build`; `engine::cube::sample::{fetch, mip_chain}`, `engine::ops::linear_to_game_gamma`.
- Produces:
  - `src/cube/mod.rs`: `#[derive(Clone, PartialEq, Debug, Serialize)] pub struct Settings { pub source: Option<SourceType>, pub softness: f32, pub brightness: f32 }` (Default: `None`, 0, 0) and `pub fn default_softness(t: Option<SourceType>) -> f32` (0.6 for `Photo`, else 0).
  - `src/cube/make.rs`: `pub fn face_size(softness: f32, cell: u32) -> u32`; `pub fn blur(c: &Cube, amount: f64) -> Cube` (copied); `pub fn brighten(c: &mut Cube, stops: f32)`; `pub fn strip(c: &Cube) -> (u32, u32, Vec<[f32; 4]>)` (game gamma, 6n x n).
  - `src/cube/files.rs`: `pub fn out_name(first_source: &Path) -> String`; `pub fn collides(source: &Path, folder: &Path) -> bool`; `pub fn cube_png(c: &Cube, sources: &[(&'static str, &Source)], s: &Settings) -> Vec<u8>`.

- [ ] **Step 1: Write the failing tests**

In `src/cube/make.rs`:
- `face_size_follows_softness_and_the_source` asserts:
  - `face_size(0.0, 4096) == 128`
  - `face_size(0.09, 4096) == 128`
  - `face_size(0.1, 4096) == 64`
  - `face_size(0.0, 100) == 64`
  - `face_size(0.0, 48) == 32`
  - `face_size(0.0, 2) == 4`
- `blur_zero_is_identity_and_blur_one_flattens`: ported from `sample.rs:395`.
- `brightness_one_doubles_linear`: `brighten(c, 1.0)` doubles RGB and keeps alpha.
- `bright_sources_clip_and_stay_finite`: a cube of 1e6, after `brighten(c, 10.0)` and `strip`, gives every value in 0..=1, all finite.
- `the_strip_puts_face_k_at_column_k_n`: a cube whose face `k` is filled with `k/8` gives a 6n x n strip whose texel `(k·n + x, y)` is `linear_to_game_gamma(k/8)`.

In `src/cube/files.rs`:
- `output_names_end_in_e`:
  - `sky.hdr` gives `sky_e.png`
  - `room_posx.png` gives `room_e.png`
  - `room_PX.png` gives `room_e.png`
  - `chrome_e.png` gives `chrome_e.png`
- `a_cube_png_is_a_marked_6_to_1_rgb_strip`: `cube_png` of a 4-texel cube decodes as 24x4, colour type RGB, 16-bit, marker kind `Cube`.

In `tests/cube_contract.rs`, a new test, `the_tabs_own_strip_lands_every_texel_on_its_direction`. It runs `texalith::cube::files::cube_png` on a cube filled with `field` (0.5 + 0.5·`cube_dir`) through the existing `convert` helper, then checks it as `every_texel_lands_on_its_cube_direction_unchanged` does. Replace that file's header `shortcut:` with one line saying this test now covers the app's writer.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --offline -j 2 --lib --test cube_contract -- --test-threads=2 cube`
Expected: compile errors, `make` and `files` do not exist.

- [ ] **Step 3: Implement**

- `blur`: copy `sample.rs:135-200` with its `shortcut: Gaussian cone blur, not GGX` comment. The half-angle is `amount.min(1.0) · π/4`. Its source level comes from `engine::cube::sample::mip_chain`.
- `strip` encodes RGB with `linear_to_game_gamma`, alpha 1.
- `cube_png` uses `marker::encode_rgb16`. Its marker records `source_path` (the first source), `source_sha256`, and `settings` as `{ "source_type", "softness", "brightness", "face_size", "inputs": { key: { "path", "sha256" } } }`. The keys are `"source"`, or `"posx"`…`"negz"` for six faces.
- `out_name`: strip a face tag, else keep the stem; add `_e` unless the stem already ends in `_e` (any case); then `.png`.

- [ ] **Step 4: Run them to verify they pass**

Run: the Step 2 command.
Expected: all pass (`cube_contract` has 10 tests).

- [ ] **Step 5: Run the whole suite**

Run: `cargo test --offline -j 2 -- --test-threads=2`
Expected: all pass, 2 ignored, no warnings.

- [ ] **Step 6: Commit (only on the user's yes)**

```bash
git add src/cube src/marker.rs src/view2d.rs src/from_normal tests/cube_contract.rs
git commit -m "Texalith: cube strip with cone blur, Brightness and the _e.png marker"
```

### Task 3: Opening sources and the cube thread

**Files:**
- Create: `src/cube/open.rs`, `src/cube/worker.rs`
- Test: unit tests in both files

**Interfaces:**
- Consumes: Tasks 1-2; `engine::load::load_image`; `crate::from_normal::files::{source, Source}`.
- Produces:
  - `src/cube/open.rs`:
    - `pub struct Opened { pub sources: Vec<Source>, pub imgs: Vec<Arc<Rgba32F>>, pub detected: Option<SourceType> }`: one source and image, or six in +X…−Z order. `detected` is `Some(Faces)` for six.
    - `pub fn open(path: &Path) -> Result<Opened, String>`
    - `pub fn fit(img: Rgba32F) -> Rgba32F`
  - `src/cube/worker.rs`:
    - `pub struct Job { pub id: u64, pub opened: Arc<Opened>, pub source: SourceType, pub settings: Settings }`
    - `pub struct Output { pub id: u64, pub n: u32, pub cube: Cube }`: linear, after blur and Brightness.
    - `pub fn spawn(repaint: impl Fn() + Send + 'static) -> (Sender<Job>, Receiver<Arc<Output>>)`: newest job wins, as `rnm::worker::spawn`.

- [ ] **Step 1: Write the failing tests**

In `src/cube/open.rs`, using small PNGs in a temp folder:
- `an_8_bit_source_is_decoded_with_game_gamma`: a code of 128 reads as `(128/255)^1.6` within 1e-6.
- `a_source_over_8192_is_halved`: `fit` of 8200x10 gives 4100x5; `fit` of 8192x10 is unchanged.
- `six_face_files_open_together`: open `room_negy.png` with the five others beside it; `sources` is in +X…−Z order and `detected == Some(Faces)`.
- `a_missing_face_file_is_named`: with `room_posz.png` absent, the error is "room_negy.png is one of six face files, but room_posz.png is missing."

In `src/cube/worker.rs`:
- `the_newest_job_wins`: send jobs 1 and 2 quickly; the last output received has id 2.
- `the_output_follows_the_face_size_rule`: a 512x256 panorama at softness 0 gives `n == 128`; at softness 0.5, `n == 64`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --offline -j 2 --lib -- --test-threads=2 cube::open cube::worker`
Expected: compile errors, the modules do not exist.

- [ ] **Step 3: Implement**

- `open`:
  - When the stem has a face tag, find all six siblings by both spellings and open them together; the first missing one gives the missing-faces line.
  - Each file: `source` (hash), `load_image`, gamma decode when not float, then `fit`.
  - `detected` is `import::detect` on the single image.
- `worker`:
  - `n = face_size(softness, cell)`, then import or `faces`, then `blur(c, softness)`, then `brighten`.
  - Keep the last imported cube keyed by `(Arc::as_ptr(&opened), source, n)`, so a Brightness drag only re-blurs.
  - Add `shortcut: blur reruns on every Brightness change; cache the blurred cube by softness if 2048 faces (plan 6) lag`.

- [ ] **Step 4: Run them to verify they pass**

Run: the Step 2 command.
Expected: 6 passed.

- [ ] **Step 5: Commit (only on the user's yes)**

```bash
git add src/cube
git commit -m "Texalith: opening cube sources, six face files, and the cube thread"
```

### Task 4: The Cubemap column

**Files:**
- Create: `src/cube/tab.rs`
- Test: `tests/cube_tab.rs` (the same rig as `tests/rnm_tab.rs`, with the real cube thread)

**Interfaces:**
- Consumes: Tasks 1-3; `crate::tab::{folder_row, help, line, warn, error_line, slider_row, Row, EXTENSIONS}`; `crate::save::{self, Review}`; `crate::export::Planned`.
- Produces: `pub struct CubeTab` with pub fields `opened: Option<Arc<Opened>>`, `settings: Settings`, `outputs: Option<Arc<Output>>`, `folder: Option<PathBuf>`, `status: Option<Result<String, String>>`, `jobs_sent: u64` and `review: Option<Review>`. Methods:
  - `pub fn new(jobs: Sender<Job>, replies: Receiver<Arc<Output>>) -> CubeTab`
  - `pub fn open(&mut self, path: PathBuf, ctx: &egui::Context)` (on a thread, last one opened wins, a panic reported through RNM's `guarded`, made `pub(crate)` for this)
  - `pub fn busy(&self) -> bool`
  - `pub fn drain(&mut self) -> bool` (as RNM's, with "Making the cube stopped. Restart Texalith." when the thread is gone with a job outstanding)
  - `pub fn draw_column(&mut self, ui: &mut egui::Ui)`
  - `pub const OPEN_SOURCE: &str = "Open source…"`

- [ ] **Step 1: Write the failing tests**

In `tests/cube_tab.rs`:
- `an_empty_tab_shows_only_the_open_row`: the empty-state line and "Open source…"; no Save, no slider.
- `a_panorama_is_detected_and_makes_a_cube`: open a 64x32 PNG; the combo shows "Panorama (2:1)", sliders "Reflection softness" and "Brightness" exist, and the output has `n == 16` (the cell is 16).
- `a_square_image_asks_for_a_source_type`: open 32x32; the combo shows "Pick one", `jobs_sent == 0`, and Save is disabled.
- `a_type_that_does_not_fit_is_refused`: on a 64x32 panorama, pick "Horizontal strip (6:1)"; the status is "pano.png is 64x32, which does not fit Horizontal strip (6:1).", and `jobs_sent` is unchanged.
- `single_photo_softens_to_27_degrees`: on a square image, pick "Single photo"; `settings.softness == 0.6`, and a reset button is named "Reset Reflection softness to 27°".
- `save_writes_one_marked_strip`: save into another folder; the file is `pano_e.png`, 6:1, RGB, 16-bit, marker kind `Cube`; the status starts with "Saved pano_e.png to ".
- `a_strip_source_collides_in_its_own_folder`: open `sky_e.png` (a 6:1 strip); the collision line shows and Save is disabled.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --offline -j 2 --test cube_tab -- --test-threads=2`
Expected: compile errors, `cube::tab` does not exist.

- [ ] **Step 3: Implement**

The column, top to bottom:
1. Heading "Cubemap", then "Open source…" with a spinner while loading, then the source file name (full path on hover). With no source: the empty-state line, and any status error.
2. Source type: a `ComboBox` labelled "Source type" listing the names. "Six face files" is enabled only when `detected == Some(Faces)`. Then the help line.
3. "Reflection softness": `Row { range: 0.0..=1.0, default: default_softness(source), scale: 45.0, suffix: "°", decimals: 0, .. }`. Changing Source type moves softness to the new type's default when it is still at the old type's default.
4. "Brightness": `Row { range: -10.0..=10.0, default: 0.0, scale: 1.0, suffix: " stops", decimals: 1, .. }`.
5. Save sub-panel: `folder_row`, the collision warning, Save (enabled only when the newest output arrived, nothing loads, a folder is set and nothing collides), then the status.

Save follows RNM's `save` and `write` (`save::review` against the first source, the shared dialog, the same three messages). It sends a job whenever settings change and the type fits.

- [ ] **Step 4: Run them to verify they pass**

Run: the Step 2 command.
Expected: 7 passed.

- [ ] **Step 5: Commit (only on the user's yes)**

```bash
git add src/cube/tab.rs tests/cube_tab.rs
git commit -m "Texalith: the Cubemap column"
```

### Task 5: The Cubemap tab in the window and the labelled cross

**Files:**
- Create: `src/cube/view.rs`
- Modify: `src/app.rs` (`Mode::Cube`, `App::new` takes a `CubeTab`, tab bar, drain, centre), `src/main.rs` (spawn the cube thread), `examples/window_timing.rs`, `tests/ui.rs` (rig)
- Test: `tests/ui.rs`; unit tests in `src/cube/view.rs`

**Interfaces:**
- Consumes: Task 4's `CubeTab`; Task 1's `cube::import::HCROSS`.
- Produces:
  - `pub const FACE_LABELS: [&str; 6]`
  - `pub fn cross_image(c: &Cube) -> egui::ColorImage`: 4n x 3n, face `k` in cell `HCROSS[k]`, values through `linear_to_game_gamma`, empty cells transparent.
  - `pub fn draw(ui: &mut egui::Ui, c: &Cube, texture: &mut Option<egui::TextureHandle>)`: the cross scaled to fit, and each label as a `Label` placed at its cell's top-left, so AccessKit sees it.

- [ ] **Step 1: Write the failing tests**

In `src/cube/view.rs`: `cross_puts_each_face_in_its_cell`, ported from `cube_cross.rs:54-121`.

In `tests/ui.rs`:
- `the_cubemap_tab_shows_the_labelled_cross`: switch to "Cubemap", deliver a cube output; all six face labels are found, and no "3D" button exists on this tab.
- `the_cubemap_centre_says_what_to_do_when_empty`: "Open a source to see its faces here."
- Widen `the_three_columns_fit_at_the_checked_widths` and `nothing_leaves_the_window_at_the_checked_widths` to the Cubemap tab with a filled column.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --offline -j 2 --lib --test ui -- --test-threads=2 cube::view cubemap three_columns nothing_leaves`
Expected: compile errors.

- [ ] **Step 3: Implement**

- Tab bar: "From normal", "RNM", "Cubemap".
- `draw` drains all three tabs.
- On the Cubemap tab the centre has no 3D/2D switch and shows `view::draw`, or the empty line. The preview gets empty maps (the flat ball) while this tab is active.
- The view's texture is dropped when a new output arrives.

- [ ] **Step 4: Run them to verify they pass**

Run: the Step 2 command.
Expected: all pass.

- [ ] **Step 5: Run the whole suite**

Run: `cargo test --offline -j 2 -- --test-threads=2`
Expected: all pass, 2 ignored, no warnings.

- [ ] **Step 6: Commit (only on the user's yes)**

```bash
git add src/cube/view.rs src/app.rs src/main.rs examples/window_timing.rs tests/ui.rs
git commit -m "Texalith: Cubemap tab with the labelled cross"
```

### Task 6 (spike): A vanilla environment-mapped ball

If any step below cannot be made to work within this task, stop. Revert this task's changes, ledger `Task 6: Ruling: vanilla ball not built — <what failed>`, and finish the plan with the 2D cross only. Tasks 1-5 do not depend on this one.

**Files:**
- Modify: `src/preview.rs` (`SLOTS` gains `("e", 4)`; `Content::material` picks the vanilla material when the maps hold `"e"`), `src/cube/files.rs` (`preview_dds`), `src/app.rs` (Cubemap maps, the 3D/2D switch back on this tab, the 3D label)
- Test: unit tests in `src/preview.rs` and `src/cube/files.rs`; `tests/cube_ball.rs` (ignored, needs a GPU)

**Interfaces:**
- Consumes: Task 3's `Output`; `engine::dds::{dds_bytes_cube, DdsDesc, ResolvedHeader}`, `engine::config::Format::Rgba8`, `engine::cube::sample::mip_chain`; `nif::props::LightingTrailer`.
- Produces:
  - `pub fn vanilla_envmap() -> MaterialState`: `builtin_default()`, `shader_type = 1`, `slsf1 |= 1 << 7` (environment mapping), `params.trailer = LightingTrailer::EnvMap { scale: 1.0 }`, and no True PBR bit.
  - `pub fn preview_dds(c: &Cube) -> Vec<u8>`: a full mip chain from `mip_chain` in linear light, each level through `linear_to_game_gamma` to RGBA8 UNORM, packed by `dds_bytes_cube` with a DX10 header.

- [ ] **Step 1: Write the failing tests**

In `src/preview.rs`, `a_cube_map_makes_a_vanilla_envmap_material`. With maps `{"e"}`, `material()` has:
- `shader_type == 1`
- `!is_pbr()`
- slot 4 equal to `textures\e.dds`

With maps `{"n"}`, it is True PBR as before.

In `src/cube/files.rs`, `the_preview_dds_is_a_cube_with_all_mips`. `preview_dds` of a 16-texel cube:
- starts with `DDS `
- has a DX10 header
- has caps2 `0xFE00`
- has 5 mips

In `tests/cube_ball.rs`, `#[ignore = "needs a GPU; never run while toolbag.exe runs"]` `the_ball_reflects_up_at_its_top`:
- Render the Sphere with the bundled shaders (located as `tests/bundle.rs` does), using a cube whose +Z face is red and whose −Z face is blue (others grey).
- At the ball's top-centre pixel, red is above blue; at its bottom-centre pixel, blue is above red.
- The test prints both pixels.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --offline -j 2 --lib -- --test-threads=2 vanilla_envmap preview_dds`
Expected: compile errors.

- [ ] **Step 3: Implement**

- `Content::material`: the vanilla material when `"e"` is present, else `pbr(displacement)`.
- `App::active_maps` for the Cubemap tab: `{"e": preview_dds(&o.cube)}`.
- The 3D/2D switch shows on the Cubemap tab. In 3D, the label "Vanilla environment map, not True PBR" sits above the frame; in 2D, the labelled cross.
- Update the Task 5 test that asserts no "3D" button on the Cubemap tab: it now asserts the label shows in 3D.

- [ ] **Step 4: Run the unit tests, then the GPU test**

Run: the Step 2 command. Expected: pass.
Check that `Get-Process toolbag -ErrorAction SilentlyContinue` is empty. Then run `cargo test --offline -j 2 --test cube_ball -- --ignored --nocapture --test-threads=1`.
Expected: PASS. A shader build failure, a black ball, or red at the bottom triggers the stop rule above. Up and down only are checked here; the in-game direction stays unverified (spike 5).

- [ ] **Step 5: Run the whole suite**

Run: `cargo test --offline -j 2 -- --test-threads=2`
Expected: all pass, 3 ignored, no warnings.

- [ ] **Step 6: Commit (only on the user's yes)**

```bash
git add src/preview.rs src/cube/files.rs src/app.rs tests/cube_ball.rs tests/ui.rs
git commit -m "Texalith: vanilla environment-mapped ball for the Cubemap tab"
```
