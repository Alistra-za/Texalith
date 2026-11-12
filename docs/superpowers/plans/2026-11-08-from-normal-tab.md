# From normal tab Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The From normal tab: open a normal or height file, set the outputs and their controls, see them in a 2D view and in the 3D preview, and save them as marked PNGs with the overwrite dialog.

**Architecture:** Generation runs on its own thread, in the same pattern as `src/preview.rs`. The tab sends a `Job` and takes the newest `Reply`. Each job carries its own cancel flag and progress counter. The worker caches the `Master` and each output by its own options, so changing an output's own slider never re-solves. While a slider that rebuilds the master is dragged, jobs run as drafts at most 1024 on the longer side. Release sends a full job. Save is refused while the newest outputs are a draft. Saving plans every file, checks each one with `export::check`, and opens an `egui::Modal` only when a file will be kept or needs a decision. The 3D preview gets RGBA8 DDS files built from the outputs: `n`, `p` and `r` (AO in B).

**Tech Stack:** Rust 1.99, edition 2021, eframe/egui 0.36.2, egui_kittest 0.36.2, `rfd` 0.17.2 (new; in the local cargo cache, the version Tessalith pins), `engine::load::load_image`, and plan 1's `from_normal`, `marker` and `export` modules.

**Spec:** `reports/Texalith generation modes and settings.md`:
- lines 113-153: the From normal settings, help text, readouts, Advanced groups and the refusal rule;
- lines 175-198: presets, slider pattern and Compare;
- lines 266-285: the preview's Height data and parallax handling, and its slot values.

`reports/Rust material completer research.md`:
- lines 139-155: the tab, including the "undetermined" green and the fit readout;
- lines 188-207: overwrite and provenance, and the dialog.

The library this builds on is `docs/superpowers/plans/2026-11-07-from-normal-core.md`, committed as `ea71228`.

## Global Constraints

- Never edit anything under `F:\ai\dds`. `git -C F:\ai\dds\converter status --short --untracked-files=no` stays empty.
- Commit only when the user asks. The commit steps below run only on the user's yes; otherwise skip them. No attribution lines.
- Cargo: `--offline -j 2`, tests with `-- --test-threads=2`, at BelowNormal priority. Run only when `Get-Process cargo,rustc -ErrorAction SilentlyContinue` is empty, because another session's Tessalith builds keep starting. Run long commands in the background.
- The UI is the only clamp. The library does not clamp its options, so every slider's range below is a hard bound.
- Help text is one visible sentence under each control, never a tooltip only. Copy it verbatim from the spec line given.
- Every slider row has a reset button. Its accessible name says the default, for example "Reset Flatten broad shapes to 0.3".
- Panels are drawn in a fixed order: left (From normal), right (Preview), then centre. The left column uses `ScrollArea::both` and `TextWrapMode::Truncate`. Every truncated value shows in full on hover.
- Enter never overwrites, never saves and never deletes.
- A draft is never written. A job's outputs carry `draft: bool`, and Save is disabled while the newest outputs are a draft or a job runs.
- Generated files go only into the selected folder. Its default is the opened file's folder.
- New non-trivial logic gets a test. Known limits get a `shortcut: <limit>, <when to upgrade>` comment.
- Each output's tick has one purpose sentence. The spec gives none, so this plan fixes them:
  - Height data: "The true heights, lowest black and highest white; the source for normals and other tools."
  - Parallax: "A display map for parallax shaders, made from Height data."
  - Normal: "A normal map made from this height."
  - AO: "Soft shadow that gathers in crevices; white is open."
  - Curvature: "Bright edges and dark creases on grey."
  - Cavity: "Dark only in narrow cracks; white is open."

## Review Focus

1. **A 4K source with a fast drag on Flatten.** Many jobs are queued and cancelled. Only the newest reply reaches the screen, nothing panics, and the last reply is a full-size job. Tests: Task 3 (newest-wins drain, cancel), Task 6 (stale reply ignored).
2. **A non-square source (1000×700) all the way to the preview.** The master is 1024×512, the DDS mip chain halves each side down to 1×1, and nothing indexes out of range. Tests: Tasks 3 and 4.
3. **Output size changed while AO runs.** The old job is cancelled through its own flag. The new job's progress starts at 0, not at the old job's 1000. Test: Task 3.
4. **A foreign file at an output path, with its marker stripped or its pixels edited.** It is listed as kept, with its reason, and is written only when the row says Overwrite. A huge foreign PNG is not decoded. Tests: Task 5.
5. **Saving to a folder where a write fails.** The rollback message from `export::write` is shown in the tab, and no partial set is left. Test: Task 5.

Not tested in game: spike 5 was skipped, so the Green and Depth defaults are untried in Skyrim. Nothing in this plan changes that.

## Not in this plan

These are named so the review can object. Each would be a later plan:
- the histogram (spec line 272);
- the "Looks wrong?" list (spec line 198). Its entries are drafted in the notes, not in the spec;
- slider bands, for example Darkness .7 to 1;
- Normal's Advanced options: Add broad shape, Smooth the source and Use real size. Plan 1 has no functions for them;
- the RNM-marker warning. `MapKind` has no RNM kind until the RNM tab exists;
- reopening with the recipe from a marker;
- keeping the open file across a device-loss restart;
- the 2K texture cap in the preview during drags (spec line 262).

---

### Task 1: Open a source

**Files:**
- Create: `src/from_normal/open.rs`
- Modify: `src/from_normal/mod.rs` (`pub mod open;`)
- Test: in-file `#[cfg(test)]`

**Interfaces:**
- Consumes: `engine::load::load_image`, `marker::read`, `input::{classify, Kind}`, `files::{source, Source}`, `sizes`, `BuildError::Parallax`'s `Display`.
- Produces:
  - `open::Opened { path: PathBuf, img: Arc<Rgba32F>, kind: Kind, source: Source, marker: Option<Marker>, sizes: Vec<u32> }`
  - `open::open(path: &Path) -> Result<Opened, String>`. The `Err` text is shown as is.

- [ ] **Step 1: Write the failing tests**

Write PNGs into a temp folder (`std::env::temp_dir().join("texalith-open-<test name>")`, removed first), using `marker::encode_rgb16` and `encode_gray16` with `marker::tests::sample`.

```rust
#[test]
fn a_normal_opens_as_a_normal_with_its_sizes() // a 64×32 bump normal → Kind::Normal, sizes == [64], source.sha256 is 64 hex chars
#[test]
fn a_marked_height_keeps_its_span() // encode_gray16 with sample(HeightData) → Kind::Height { span_texels: Some(12.5) }
#[test]
fn a_parallax_map_is_refused_with_the_library_text() // `x_parallax.png` → Err(e) where e == BuildError::Parallax.to_string()
#[test]
fn a_missing_file_says_so() // Err(e) where e contains the file name
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --offline -j 2 --lib open -- --test-threads=2`
Expected: FAIL, `open` not found.

- [ ] **Step 3: Implement `open`**

Read the bytes once. Take `marker::read(&bytes)` only when the extension is `png` (ASCII, any case). Then `load_image`, `classify` and `files::source`. A `Kind::Parallax` result is `Err(BuildError::Parallax.to_string())`. A load error is `Err(e.to_string())`, which names the path.

- [ ] **Step 4: Run them to see them pass**

Run: the same command. Expected: 4 passed.

- [ ] **Step 5: Commit (only on the user's yes)**

`git add src/from_normal/open.rs src/from_normal/mod.rs`, then `git commit -m "From normal: open a source"`.

---

### Task 2: Settings and the marker JSON

**Files:**
- Create: `src/from_normal/settings.rs`
- Modify: `src/from_normal/mod.rs` (`pub mod settings;`)
- Test: in-file

**Interfaces:**
- Consumes: `MasterOpts`, `ParallaxOpts`, `AoSettings`, `CurvatureOpts`, `Green`, `MasterReport`, `Kind`, `MapKind`, `default_crevice`, `sizes`.
- Produces:
  - `Ticks { height: bool, parallax: bool, normal: bool, ao: bool, curvature: bool, cavity: bool }`
  - `Settings { master: MasterOpts, parallax: ParallaxOpts, normal_green: Green, ao: AoSettings, crevice: Option<f32>, curvature: CurvatureOpts, ticks: Ticks }`, where `crevice: None` means `default_crevice(width)`
  - `Settings::for_source(kind: &Kind, src: (u32, u32)) -> Settings`
  - `Settings::job_kind(kind: &Kind) -> Kind`
  - `Settings::json(&self, out: MapKind, report: &MasterReport) -> serde_json::Value`
  - `fade_pct(flatten: f32) -> Option<f32>`
  - `PRESETS: [(&str, f32); 4]`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn defaults_follow_the_source() {
    let n = Settings::for_source(&Kind::Normal, (4096, 4096));
    assert_eq!(n.master.size, 2048); // the source's size, at most 2048: a full 4K AO takes 5 to 12 s (spike 6)
    assert_eq!((n.master.flatten, n.master.edges), (0.3, Edges::Repeats));
    assert!(n.ticks.height && n.ticks.parallax && n.ticks.ao && !n.ticks.normal && !n.ticks.curvature && !n.ticks.cavity);
    assert_eq!(Settings::for_source(&Kind::Normal, (8, 8)).master.size, 8);
    let h = Settings::for_source(&Kind::Height { span_texels: Some(20.48) }, (2048, 2048));
    assert!(h.ticks.normal && !h.ticks.height); // a height file is already the master
    assert!((h.master.normal_depth_pct - 1.0).abs() < 1e-6); // 20.48 / 2048 = 1 %
    let unmarked = Settings::for_source(&Kind::Height { span_texels: None }, (512, 512));
    assert_eq!(unmarked.master.normal_depth_pct, 1.0);
}
#[test]
fn a_height_job_always_takes_its_depth_from_the_slider() {
    assert_eq!(Settings::job_kind(&Kind::Height { span_texels: Some(9.0) }), Kind::Height { span_texels: None });
    assert_eq!(Settings::job_kind(&Kind::Normal), Kind::Normal);
}
#[test]
fn the_flatten_readout_matches_the_spec() {
    assert_eq!(fade_pct(0.3).map(|p| p.round()), Some(42.0)); // spec line 121; solve.rs:28 damps below 8·flatten cycles
    assert_eq!(fade_pct(0.0), None);
    assert_eq!(fade_pct(1.0), Some(12.5));
}
#[test]
fn the_marker_json_holds_the_master_and_that_output_only() {
    let s = Settings::for_source(&Kind::Normal, (1024, 1024));
    let r = MasterReport { green: Green::Opengl, green_guessed: true, ..Default::default() };
    let j = s.json(MapKind::Parallax, &r);
    assert_eq!(j["size"], 1024);
    assert_eq!(j["edges"], "repeats");
    assert_eq!(j["green"], "opengl");
    assert_eq!(j["green_guessed"], true);
    assert_eq!(j["output"]["depth"], serde_json::json!(0.9f32));
    assert_eq!(j["output"]["flat"], "mid-grey");
    assert!(s.json(MapKind::Ao, &r)["output"].get("depth").is_none());
    assert_eq!(s.json(MapKind::Cavity, &r)["output"]["crevice_texels"], serde_json::json!(8.0f32)); // 1024/128
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --offline -j 2 --lib settings -- --test-threads=2`. Expected: FAIL, the module is missing.

- [ ] **Step 3: Implement**

The JSON shape is an interface; later plans read it back.
- The top level is `{size, edges: "repeats"|"does-not-repeat", green: "directx"|"opengl" (report.green), green_guessed, flatten, normal_depth_pct, output}`.
- `output` per kind:
  - `HeightData`: `{}`
  - `Parallax`: `{depth, flat: "mid-grey"|"white", smooth_texels, invert}`
  - `Normal`: `{green}`
  - `Ao`: `{reach, darkness, surface_depth}`
  - `Curvature`: `{line_texels, contrast}`
  - `Cavity`: `{crevice_texels, surface_depth}`, with the crevice resolved against `size`.

`fade_pct(f)` is `12.5 / f` for `f > 0`.

`PRESETS` copies spec lines 179-182: `("Rough stone and ground", 0.0)`, `("Brick and tile", 1.0)`, `("Bark, moss, weave", 1.0)`, `("Carved parts", 1.0)`.

- [ ] **Step 4: Run them to see them pass**

Expected: 4 passed.

- [ ] **Step 5: Commit (only on the user's yes)**

`git commit -m "From normal: settings and marker JSON"`.

---

### Task 3: Generation worker

**Files:**
- Create: `src/from_normal/worker.rs`
- Modify: `src/from_normal/mod.rs` (`pub mod worker;`)
- Test: in-file

**Interfaces:**
- Consumes: Task 1's `Opened.img` and `Kind`, Task 2's `Settings`, `master::build`, every `maps::*` output, `match_depth`, and `resample`.
- Produces:
  - `DRAFT_SIZE: u32 = 1024`
  - `COMPARE_SIZE: u32 = 128`
  - `Job { id: u64, img: Arc<Rgba32F>, kind: Kind, settings: Settings, draft: bool, compare: bool, cancel: Arc<AtomicBool>, progress: Arc<AtomicU32> }`
  - `Outputs { id: u64, draft: bool, size: (u32, u32), report: MasterReport, match_depth: f32, height: (Gray, f64), parallax: Option<(Gray, ParallaxReport)>, normal: Option<Vec<[f32; 4]>>, preview_normal: Vec<[f32; 4]>, ao: Option<Gray>, cavity: Option<Gray>, curvature: Option<Gray>, compare: Option<[Gray; 3]> }`
  - `Reply { Done(Box<Outputs>), Failed { id: u64, message: String } }`. A cancelled job sends nothing.
  - `Cache` (private fields), `Cache::default()`, and the public counters `master_builds: u32` and `ao_runs: u32`.
  - `compute(cache: &mut Cache, job: &Job) -> Result<Option<Outputs>, String>`. `Ok(None)` means the job was cancelled.
  - `spawn(repaint: impl Fn() + Send + 'static) -> (Sender<Job>, Receiver<Reply>)`

- [ ] **Step 1: Write the failing tests**

Build inputs with the `master::tests::bump_normal(n)` helper.

```rust
#[test]
fn an_output_slider_never_rebuilds_the_master_or_reruns_ao() {
    // job(depth .9) then job(depth .5), same MasterOpts: master_builds == 1, ao_runs == 1, parallax differs
}
#[test]
fn a_flatten_change_rebuilds_the_master_and_reruns_ao() // master_builds == 2, ao_runs == 2
#[test]
fn a_draft_runs_at_1024_and_never_above_the_chosen_size() {
    // 2048² source, size 2048, draft → Outputs.size == (1024, 1024) and draft == true;
    // size 512, draft → (512, 512); not draft → (2048, 2048)
}
#[test]
fn unticked_outputs_are_not_made_but_height_data_always_is() {
    // ticks all false → height present, parallax/normal/ao/cavity/curvature None
}
#[test]
fn a_cancelled_job_returns_none_and_each_job_counts_its_own_progress() {
    // job A: AO ticked, cancel set before compute → Ok(None)
    // job B: fresh Arc<AtomicU32> → 1000 after compute, A's counter untouched by B
}
#[test]
fn the_preview_normal_is_always_directx() {
    // normal input, settings.master.green = Some(Opengl): preview_normal[i][1] ≈ 1 − source[i][1] (± 1e-3)
    // height input: preview_normal == normal_from_height(master, Directx)
}
#[test]
fn compare_builds_three_heights_at_128() // compare: true on a 512² source → three Gray of 128×128, the middle at the job's flatten
#[test]
fn the_thread_answers_the_newest_job() {
    // spawn, send ids 1..=5 back to back (cancel each earlier one's flag as the tab does), then
    // recv_timeout(30 s) until Done(id 5); no reply after it carries a lower id
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --offline -j 2 --lib worker -- --test-threads=2`. Expected: FAIL, the module is missing.

- [ ] **Step 3: Implement**

- The master key is `(Arc::as_ptr(&job.img) as usize, job_kind, MasterOpts with size = effective size)`. The effective size is `settings.master.size.min(DRAFT_SIZE)` for a draft. Each output's key is `(master generation, its own options)`. A cache hit clones the stored result.
- AO runs at the master's longer side with the job's `progress` and `cancel`. Cavity uses the same flags. Its crevice is `settings.crevice.unwrap_or(default_crevice(width))`.
- `preview_normal`:
  - For a normal input, the source resampled to the master's dims. Green is flipped (`g → 1 − g`) when `report.green == Opengl`.
  - For a height input, `normal_from_height(&m, Green::Directx)`.
- `compare`: three `master::build` calls at `COMPARE_SIZE.min(largest)`, with flatten 0, the job's value and 1, each through `height_data`.
- `spawn`: one named thread `"from-normal"`. It drains `try_iter`, keeps only the last job, runs `compute`, sends a reply and calls `repaint`.
- `BuildError::Cancelled` maps to `Ok(None)`. Other errors map to `Err(e.to_string())`.

- [ ] **Step 4: Run them to see them pass**

Expected: 8 passed.

- [ ] **Step 5: Commit (only on the user's yes)**

`git commit -m "From normal: generation worker with cache, drafts and cancel"`.

---

### Task 4: Preview maps from the outputs

**Files:**
- Create: `src/preview_maps.rs`
- Modify: `src/lib.rs` (`pub mod preview_maps;`), and `src/testmaps.rs` (`dds` calls `preview_maps::dds_rgba8(s, s, ..)`)
- Test: in-file

**Interfaces:**
- Consumes: Task 3's `Outputs`, `maps::Gray`.
- Produces:
  - `dds_rgba8(w: u32, h: u32, base: &[[u8; 4]], srgb: bool) -> Vec<u8>`: a DX10 RGBA8 DDS with a full box-filtered mip chain. Each side halves to at least 1.
  - `maps_for(o: &Outputs) -> HashMap<&'static str, Vec<u8>>`: `n` is always present. `p` is present when `o.parallax` is `Some`. `r` is present when `o.ao` is `Some`. There is never an `a`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_non_square_dds_halves_each_side_to_one() {
    // dds_rgba8(8, 2, ..): header width 8, height 2, mip count 4 (8×2, 4×1, 2×1, 1×1),
    // total length == 148 + 4 * (16 + 4 + 2 + 1)
}
#[test]
fn the_square_test_maps_are_unchanged() // testmaps::dds(4, ..) bytes == the pre-change bytes for a fixed 4×4 input (capture them first)
#[test]
fn ao_goes_to_blue_with_the_missing_map_defaults() {
    // ao px 0.5 → r slot texel [255, 0, 128, 255] (spec line 285)
}
#[test]
fn only_made_maps_are_bound() // parallax None, ao None → keys == {"n"}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --offline -j 2 --lib preview_maps -- --test-threads=2`. Expected: FAIL.

- [ ] **Step 3: Implement**

Move `testmaps::dds`'s body to `dds_rgba8`, with `(w, h)` in place of `s`. A level with an odd or 1-texel side averages the texels that exist. For example, a 2×1 level averages 2 texels into 1×1.

Grey becomes `[v, v, v, 255]` with `v = (px·255).round()`. The normal is RGB × 255, alpha 255. `n` and `p` are linear. Nothing here is sRGB, because there is no albedo.

- [ ] **Step 4: Run them to see them pass**

Run: the same command, plus `cargo test --offline -j 2 --test frame_diff -- --test-threads=2`. Expected: 4 passed. frame_diff is unchanged (ignored).

- [ ] **Step 5: Commit (only on the user's yes)**

`git commit -m "Preview maps from From normal outputs, non-square DDS"`.

---

### Task 5: Save: plan, review, write

**Files:**
- Create: `src/save.rs`, `tests/save.rs`
- Modify: `src/lib.rs` (`pub mod save;`), `src/marker.rs` (`is_ours` size guard)
- Test: `tests/save.rs`, `marker.rs` in-file

**Interfaces:**
- Consumes: `Opened`, `Settings`, `Outputs`, `files::{base_name, out_name, gray_png, normal_png}`, `marker::{read, MapKind}`, `export::{check, write, Planned, Verdict}`.
- Produces:
  - `save::plan(o: &Opened, s: &Settings, out: &Outputs, folder: &Path) -> Vec<(MapKind, Planned)>`: one entry per ticked output, in the order Height data, Parallax, Normal, AO, Curvature, Cavity.
  - `Row { kind: MapKind, planned: Planned, verdict: Verdict, reason: Option<String>, overwrite: bool }`
  - `Review { rows: Vec<Row>, backup: bool }`
  - `save::review(plan: Vec<(MapKind, Planned)>, source: &Path) -> Review`
  - `Review::needs_dialog(&self) -> bool`
  - `Review::write(&self, source: &Path) -> Result<Vec<PathBuf>, String>`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn the_plan_names_and_marks_each_ticked_output() {
    // Opened from a temp `rock_n.png` normal, Outputs from worker::compute, ticks height+parallax+ao:
    // paths == folder/rock_height.png, rock_parallax.png, rock_ao.png;
    // each marker's settings == s.json(kind, &out.report);
    // height marker span_texels == Some(out.height.1)
}
#[test]
fn an_unticked_height_still_ties_the_parallax_to_its_master() {
    // ticks parallax only: one entry; its master_sha256 == the pixels_sha256 of the Height data PNG plan() would write
}
#[test]
fn only_foreign_unreadable_or_source_rows_open_the_dialog() {
    // empty folder → needs_dialog false; a plain PNG at rock_ao.png → true, that row's overwrite false,
    // reason "Not made by Texalith, or its marker was removed"; a marked file with edited pixels →
    // reason "Made by Texalith, then changed"
}
#[test]
fn a_kept_row_is_not_written_and_an_overwritten_one_is_backed_up() {
    // foreign rock_ao.png: write() with overwrite false leaves its bytes; with overwrite true and backup true,
    // _sdc/texalith-backup/rock_ao.png holds the old bytes
}
#[test]
fn a_failed_write_reports_the_path_and_leaves_no_partial_set() // a plain, read-only rock_ao.png (Windows attribute) whose row is set to overwrite: Err names rock_ao.png; rock_height.png and rock_parallax.png absent afterwards
```

In `marker.rs`:

```rust
#[test]
fn a_png_larger_than_any_output_is_never_ours() // a valid marked 4097×1 gray PNG → is_ours false
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --offline -j 2 --test save --lib marker -- --test-threads=2`. Expected: FAIL.

- [ ] **Step 3: Implement**

- `plan` encodes Height data first, even when it is unticked, and passes its `marker::read(..).pixels_sha256` to the Parallax PNG. It keeps the Height data entry only when it is ticked.
- `review`:
  - It sets `overwrite` false on every row.
  - `reason` per verdict:
    - `Foreign`: with a marker, "Made by Texalith, then changed"; without one, "Not made by Texalith, or its marker was removed".
    - `Unreadable(e)`: "Cannot be read, so it is kept: {e}".
    - `Source`: "This is the file you opened, so it is kept".
  - `backup` is true.
- `needs_dialog` is true when any row is `Foreign`, `Unreadable` or `Source`.
- `write` passes the rows' `overwrite` flags in plan order to `export::write`.
- `is_ours` reads the PNG header first and returns false when either side exceeds `from_normal::MAX_SIZE`, before any decode. This closes plan 1's "is_ours has no decode size cap" minor.

- [ ] **Step 4: Run them to see them pass**

Expected: 6 passed.

- [ ] **Step 5: Commit (only on the user's yes)**

`git commit -m "Save: plan, review and write the From normal outputs"`.

---

### Task 6: The From normal column

**Files:**
- Create: `src/tab.rs`, `tests/tab.rs`
- Modify: `src/lib.rs` (`pub mod tab;`), `Cargo.toml` (`rfd = "=0.17.2"` under `[dependencies]` with the comment `# Open and folder dialogs, the version Tessalith pins.`)
- Test: `tests/tab.rs` (egui_kittest)

**Interfaces:**
- Consumes: Tasks 1, 2, 3 and 5.
- Produces:
  - `Tab::new(jobs: Sender<Job>, replies: Receiver<Reply>) -> Tab`
  - `Tab::accept(&mut self, o: Opened)`: sets the settings from `for_source` and the folder to the source's folder, then submits a full job.
  - `Tab::draw_column(&mut self, ui: &mut egui::Ui)`
  - `Tab::drain(&mut self) -> bool`: true when new outputs arrived.
  - Public fields: `settings: Option<Settings>`, `outputs: Option<Arc<Outputs>>`, `opened: Option<Opened>`, `folder: Option<PathBuf>`, `status: Option<Result<String, String>>`, `jobs_sent: u64`.
  - `pub const COLUMN_LABEL: &str = "From normal"` (the column's accessible name).

- [ ] **Step 1: Write the failing tests**

The tests use a rig like `tests/ui.rs`: `Tab` plus job and reply channels. No real solve runs. Replies are hand-built `Outputs` from a 4×4 `Gray`.

```rust
#[test]
fn the_empty_tab_has_one_action() // "Open a normal or height map…" button present; "Save" button disabled; no slider present
#[test]
fn opening_sends_one_full_job() // accept(opened 64×64 normal) → exactly one Job, draft false, settings == for_source
#[test]
fn dragging_flatten_sends_drafts_and_release_sends_a_full_job() // drag the "Flatten broad shapes" slider: ≥1 Job with draft true; drop: the last Job has draft false and the dragged value
#[test]
fn an_output_slider_sends_a_job_with_the_same_master_options() // Parallax Depth click-change: Job.settings.master == before
#[test]
fn a_stale_reply_is_ignored() // reply Done(id 1) after Job id 2 was sent: outputs stay None
#[test]
fn every_help_sentence_is_visible() // after a reply: each spec help sentence (lines 119-132) for the shown rows is found by label
#[test]
fn an_unticked_height_with_parallax_says_it_will_not_be_saved() // the line "Height data is still made for the Parallax map, but it will not be saved." appears only in that state
#[test]
fn a_height_source_shows_scale_unknown_and_the_normal_row() // Kind::Height { span_texels: None }: "Scale unknown" label and the "Green direction" combo present
#[test]
fn reset_buttons_name_their_default() // a Button named "Reset Flatten broad shapes to 0.3" exists and restores 0.3
#[test]
fn the_column_fits_at_the_checked_widths() // at 1100, 1280, 1600: every widget's interact_rect inside the column's PanelState rect, and nothing past the window
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --offline -j 2 --test tab -- --test-threads=2`. Expected: FAIL.

- [ ] **Step 3: Implement the column, top to bottom**

1. **Source.** The file name (truncated, full path on hover) and the kind. "Open…" uses `rfd::FileDialog` with the filter png, tga, tif, tiff, bmp, jpg, jpeg, webp, exr, hdr, psd, dds. Dropped files are read with `ctx.input(|i| i.raw.dropped_files.clone())`, first path only. Opening runs on a thread, and its result comes back through a channel. Show `Err` text in the error colour.
2. **Shared.**
   - Output size: a combo of `opened.sizes`, help from line 119.
   - Edges: a combo, help from line 120.
   - When `report.seam_hint` is true, show the seam text from spec line 60 verbatim in amber.
   - Green: "DirectX (guessed)", "OpenGL (guessed)" or "undetermined" when `!green_sure`.
   - The fit line: "Rebuilds the normal to within {mean:.1}° (95 % within {p95:.1}°)".
3. **Height data.** A tick and its purpose sentence.
   - Flatten broad shapes: 0 to 1. End labels "Keep large forms" and "Surface detail only". Readout "fades shapes wider than {fade_pct:.0} % of the width", or "keeps every shape" at 0. Help from line 121.
   - A "Material" combo from `PRESETS`.
   - A "Compare" toggle. When it is on, three 96 px thumbnails from `outputs.compare`, labelled "0", "Current", "1". Clicking one sets Flatten.
   - The span readout from spec line 139.
   - Advanced: Green direction (input), with Guess, DirectX and OpenGL.
4. **Parallax**, indented under Height data. A tick and its purpose sentence.
   - Depth: 10 to 90 %. A "Match the normal" button sets `outputs.match_depth`.
   - Flat level: Mid-grey or White, help from lines 123-124.
   - Readouts: "{times_normal:.1}× deeper than the normal implies", "Levels used: {n} of 256" (amber under 60) and "Clipped: {share:.1} %".
   - With White, the line "This preview pivots on mid-grey, so a White map looks shifted in 3D."
   - Advanced: Smooth sharp steps, 0 to 8 texels, and Invert.
5. **Normal** (height source only). Depth uses a log slider, .05 to 5 % of width, with "Scale unknown" when the opened kind has no span. Green direction. Help from lines 125-126.
6. **AO.**
   - Reach: 1.5625 to 25 %, with a texel readout.
   - Darkness: .5 to 3.
   - Surface depth: .25 to 4.
   - Help from lines 127-129.
   - The two fixed lines from spec line 138.
   - While a job runs with AO ticked, a `ProgressBar` from the job's `progress / 1000`.
7. **Curvature.** Line thickness 1 to 8 and Contrast .5 to 2, help from lines 130-131.
8. **Cavity.** Crevice width 2 to 16, help from line 132. Advanced: Surface depth, shared with AO.
9. **Save row.**
   - "Save to": the folder's tail, full path on hover, and "Change…" with `rfd::FileDialog::pick_folder`.
   - "Save {n} maps" is enabled only when outputs exist, are not a draft, no job runs and n > 0.
   - The status line. Task 7 adds the dialog.

Slider handling:
- Every slider goes through one helper: `slider_row(ui, name, value, range, default, help) -> SliderEvent { None, Dragging, Released, Changed }`. It adds the reset button.
- `Dragging` on a `MasterOpts` slider, or on Reach or Surface depth, submits a draft. `Released` and `Changed` submit a full job.
- `submit` sets the previous job's `cancel`, creates a fresh `progress`, increments the id and sends. The job's kind is `Settings::job_kind(&opened.kind)`.
- `Reply::Done` is kept only when its id is the newest sent. It is stored as `Arc::from(boxed)`.

Only the left column uses `TextWrapMode::Truncate`. Help lines wrap in a `ui.scope` as `app.rs` does now.

- [ ] **Step 4: Run them to see them pass**

Expected: 10 passed.

- [ ] **Step 5: Commit (only on the user's yes)**

`git commit -m "From normal column: open, controls, readouts"`.

---

### Task 7: The overwrite dialog

**Files:**
- Modify: `src/tab.rs`, `tests/tab.rs`

**Interfaces:**
- Consumes: Task 5's `save::{plan, review, Review}`.
- Produces: `Tab.review: Option<Review>`, shown as an `egui::Modal` with id `"save-review"`.

- [ ] **Step 1: Write the failing tests**

Use a temp folder as `Tab.folder`. The outputs are hand-built, at 4×4.

```rust
#[test]
fn save_without_conflicts_writes_at_once() // empty folder: click "Save 3 maps" → files exist, no modal, status Ok("Saved 3 maps to …")
#[test]
fn a_foreign_file_opens_the_dialog_with_keep_chosen() // plain PNG at x_ao.png: modal shows "x_ao.png", its reason, "Keep" selected, "Keep a backup in _sdc\\texalith-backup" ticked
#[test]
fn enter_in_the_dialog_does_nothing() // press Enter twice: modal still open, foreign bytes unchanged, no new files
#[test]
fn overwrite_with_backup_writes_and_keeps_the_old_file() // select "Overwrite", click "Save": x_ao.png replaced, backup holds old bytes, modal closed
#[test]
fn cancel_writes_nothing() // click "Cancel" (or Esc): no new files
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --offline -j 2 --test tab -- --test-threads=2`. Expected: the 5 new tests fail.

- [ ] **Step 3: Implement**

- Clicking Save runs `plan` and then `review`. If `!needs_dialog()`, it writes at once. Otherwise it stores the `Review`.
- The modal has:
  - a heading "Some files will be kept";
  - one row per conflicting row: file name, reason, and for Foreign rows two selectable labels "Keep" and "Overwrite";
  - the backup checkbox;
  - buttons "Save" and "Cancel". Save writes the New and Ours rows and the rows set to Overwrite; `export::write` skips Same.
- No button takes Enter. Esc is Cancel.
- `export::write`'s `Err` goes to `status` as `Err`, and the modal closes. This is the rollback message from Review Focus 5.
- The modal deviates from egui-ui-rules 5.1 and 7, which prefer a non-modal panel. The spec (research report line 203) names `egui::Modal`, and the spec wins. Note this in the code with one comment.

- [ ] **Step 4: Run them to see them pass**

Expected: all 15 tab tests pass.

- [ ] **Step 5: Commit (only on the user's yes)**

`git commit -m "From normal: overwrite dialog"`.

---

### Task 8: Window wiring: 2D view, 3D preview, startup

**Files:**
- Create: `src/view2d.rs`
- Modify:
  - `src/app.rs`: `App::new` gains `tab: Tab`. Add the left column, a 3D/2D toggle at the top of the centre, and calls to `preview_maps` on new outputs.
  - `src/main.rs`: spawn the worker. Start with an empty `Content` (no maps, so a flat ball), not `testmaps::content(2048)`.
  - `tests/ui.rs`: the rig builds a `Tab` with dead-end channels. `panel_left` stays `width - 280`.
  - `examples/window_timing.rs`: the new `App::new` argument.
- Test: `tests/ui.rs`

**Interfaces:**
- Consumes: everything above.
- Produces:
  - `view2d::View2d { map: MapKind, repeat: bool }`
  - `view2d::draw(ui, v: &mut View2d, out: &Outputs, textures: &mut HashMap<MapKind, egui::TextureHandle>)`

- [ ] **Step 1: Write the failing tests** (in `tests/ui.rs`)

```rust
#[test]
fn new_outputs_reach_the_preview() // a Done reply with parallax and ao → one Cmd::SetContent whose maps keys == {"n","p","r"}; a second reply with the same keys → Cmd::SwapMap only for changed maps
#[test]
fn the_2d_view_shows_the_chosen_map_and_repeats_it() // toggle "2D", pick "AO" in the "Map" combo, tick "Repeat 2x2": four image widgets named "AO"
#[test]
fn the_three_columns_fit_at_the_checked_widths() // 1100/1280/1600: the From normal column, the preview panel and the centre do not overlap; centre width ≥ 400
```

Keep `nothing_leaves_the_window_at_the_checked_widths`, extended to the left column.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --offline -j 2 --test ui -- --test-threads=2`. Expected: FAIL.

- [ ] **Step 3: Implement**

- **Panel order:** `Panel::left("from-normal")` resizable, default 340, range `300..=(avail − 280 − 400).max(300)`; then `Panel::right("controls")` as now; then the centre.
- **Centre:** two selectable labels at the top, "3D" and "2D". 3D is the default. Then the viewport, or `view2d::draw`.
- **2D view:**
  - A "Map" combo over the outputs present.
  - The image is fitted and centred. With Repeat 2x2, the same texture is drawn in four cells.
  - Each image is named after its map for AccessKit.
  - Textures update only when `Tab::drain` reports new outputs.
- **Preview:** `App` keeps the last key set it sent. When the set changes it sends `SetContent` with the current form and displacement. When the set is the same it sends `SwapMap` for each map whose bytes changed.
- **Restart line:** the restart notice becomes "The graphics device was lost, so Texalith restarted. Open your file again; maps that were not saved are gone." Add a `shortcut:` comment in `main.rs`: the open file is not carried across a restart; pass `--open <path>` in the restart args when someone loses work to it.
- **testmaps:** stays for `frame_diff` and `window_timing`. Update its header shortcut to say it is no longer loaded at startup.

- [ ] **Step 4: Run the whole suite**

Run: `cargo test --offline -j 2 -- --test-threads=2`. Expected: every earlier test passes (95 + the new ones), with 0 warnings.

- [ ] **Step 5: Look at it**

Run `cargo run --offline -j 2`, only when `toolbag.exe` is not running. Open a normal from `research_notes`' truth sets. Capture the window at about 1100, 1280 and 1600 px wide, and check egui-ui-rules groups 2 and 3 against the images.

- [ ] **Step 6: Commit (only on the user's yes)**

`git commit -m "Window: From normal column, 2D view, preview wiring"`.
