# Cubemap Advanced Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Cubemap tab's collapsed Advanced group with its 10 items: Side size, Turn, Mirror, Highlight rolloff, Colour strength, Tint, Ground, Horizon line, Fix one side, Turn bottom cell.

**Architecture:** Task 1 grows `Settings` and adds the per-face fixes to the importer. Task 2 adds `src/cube/adjust.rs`: Turn, Mirror and Ground are copied from Tessalith's `cube/filter.rs`, and a new `tone` step (Brightness, Highlight rolloff, Colour strength, Tint) replaces `make::brighten`. Task 3 runs them in the worker in a fixed order, with a second cache so a tone change does not blur again, and records every setting in the marker. Task 4 draws the Advanced group.

**Tech Stack:** Rust 1.99, edition 2021, eframe/egui 0.36.2, egui_kittest 0.36.2, rayon, `engine::cube::{Cube, cube_dir, world::{to_world, to_cube}, sample::resample}`, `engine::ops` game gamma.

**Spec:** `reports/Texalith generation modes and settings.md`, line 173: the 10 Advanced names, Side size 4 to 2048, Tessalith's Brightness, Contrast and Midtones dropped, nothing on the tab measured. `reports/Rust material completer research.md`, lines 118-121: run order (yaw, mirror, colour, ground fill, cone blur, then the new tone step), "Exposure and soft shoulder after blur", and a small Texalith-owned settings struct, not `CubemapSpec`.

The spec gives the Advanced items names only. The ranges, defaults and help text below are this plan's, as plans 2 and 3 did for their Advanced rows. Meanings come from Tessalith's `CubemapSpec` (`crates/engine/src/config.rs:887-933`), `cube/filter.rs` (yaw, mirror, ground_fill, colour), `cube/layout.rs:59-91,141-161` (rotate, flip, the vertical-cross −Z turn, orient) and `gui/src/ui/cube_section.rs:305-444` (ranges, first values).

Decisions taken in this plan:

- Colour strength and Tint run in linear light after the blur, in `tone`. Tessalith grades in game gamma before the blur and clamps to 0..1 there, which would clip an HDR sun before Brightness and the blur see it. A luma mix and a multiply are linear, so they commute with the blur. Tint multiplies by the tint decoded with game gamma, which equals Tessalith's display-space multiply exactly.
- Highlight rolloff is Tessalith's highlight clamp (white point `w`, `l' = min(l·(1 + l/w²)/(1 + l), 1)` on Rec. 709 luma). It runs after Brightness, which is the research note's "soft shoulder after blur".
- Ground "Darken toward the bottom" is Tessalith's `gradient`. "Solid colour" first writes Tessalith's `[40, 30, 20]`.
- An explicit Side size wins over the source's cell, so a small source can be scaled up. Auto keeps plan 5's rule.
- `make::blur` works on at most a 256 face and scales the result back up with `engine::cube::sample::resample`. At 2048 a full-size cone blur is 25 million texels × up to 289 samples.
- The 2D cross draws at most 512 per face, taking every `(n / 512)`-th texel.
- Opening a source resets every Advanced setting to its default, as plan 5 resets Brightness.

## Global Constraints

- Never edit anything under `F:\ai\dds`. `git -C F:\ai\dds\converter status --short --untracked-files=no` stays empty.
- Commit only when the user asks. No attribution lines.
- Cargo: `--offline -j 2`, tests with `-- --test-threads=2`, BelowNormal priority, only when no other cargo or rustc runs. Recreate the `cargo.ps1` helper in this plan's workspace, as plan 5 did. Run long builds in the background.
- Never run a GPU test while `toolbag.exe` runs. `tests/bundle.rs` uses the GPU and runs in the whole suite. Scan every new or changed file for NUL bytes before a task is done.
- Each copied function block names its Tessalith source in a comment line above it. Copied code keeps its tests.
- Settings, ranges and defaults (`Settings` field in brackets):
  - Side size [`side: Option<u32>`]: Auto (`None`, default), or 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048. Combo items "Auto ({n} px)" when a fitting type gives Auto a size, else "Auto"; then "{n} px".
  - Turn [`turn: f32`]: −180 to 180 degrees, default 0, positive clockwise seen from above (90 moves east to south), whole degrees (no decimals).
  - Mirror [`mirror: bool`]: off.
  - Highlight rolloff [`rolloff: Option<f32>`]: off (`None`). A checkbox; ticking it writes 4. When ticked, a slider "White point", 1 to 64, logarithmic, suffix "×", one decimal, default 4.
  - Colour strength [`colour: f32`]: 0 to 2, default 1, shown as 0 to 200 % (scale 100, suffix " %", no decimals).
  - Tint [`tint: [u8; 3]`]: default `[255, 255, 255]` (no tint). A colour button and a "No tint" button, enabled when the tint is not white.
  - Ground [`ground: Ground`]: `Keep` (default, "Keep the source"), `Darken` ("Darken toward the bottom"), `Colour([u8; 3])` ("Solid colour", first `[40, 30, 20]`, with a colour button beside the combo).
  - Horizon line [`horizon: f32`]: −45 to 45 degrees, default 0, one decimal. Shown only when Ground is not Keep.
  - Fix one side [`face_turn: [u16; 6]`, `face_flip: [Flip; 6]`]: per face, turn 0, 90, 180 or 270 clockwise, and flip `None`, `LeftRight` or `TopBottom`. Defaults all 0 and `None`. The UI is a face combo "Fix one side" (items are `view::FACE_LABELS`; the picked face is tab state, not a setting), then combos "Turn this face" ("None", "90° clockwise", "180°", "90° anticlockwise") and "Flip this face" ("None", "Left to right", "Top to bottom"). Below them, when any face is changed: "Changed faces: {labels, comma-separated}". Shown only for crosses, strips and six face files.
  - Turn bottom cell [`turn_bottom: bool`]: on. Shown only for Vertical cross.
- Help text, verbatim:
  - Side size: "Sets the width of each face in pixels; leave it on Auto, and raise it only for a sharp mirror seen up close, because a bigger cube costs memory and a soft reflection gains nothing."
  - Turn: "Turns the surroundings around the up axis, and positive turns clockwise seen from above; use it to move the sun or a bright window to where the reflection should show it."
  - Mirror: "Swaps east and west; tick it if writing or a known landmark reads backwards in the preview."
  - Highlight rolloff: "Squeezes very bright light such as the sun so it fades into the sky instead of clipping to a hard white patch; tick it for HDR sources, and lower the white point to squeeze harder."
  - Colour strength: "Sets how colourful the reflection is, and 0 makes it grey; lower it if a coloured sky tints every shiny surface, and raise it for a deliberately vivid look."
  - Tint: "Multiplies the reflection by a colour, and white changes nothing; use it to warm or cool reflections to match the scene's light."
  - Ground: "Replaces everything below the horizon line; use it when the bottom of the source shows a tripod, a blank area or the wrong floor."
  - Horizon line: "Sets where the ground starts, in degrees above or below level; raise it if a strip of the old floor still shows above the fill."
  - Fix one side: "Turns or flips one face of a cross, strip or six face files; use it when one face in the labelled preview is rotated or mirrored against its neighbours."
  - Turn bottom cell: "A vertical cross usually stores the bottom face upside down, so it is turned back; untick it if the down face in the preview is upside down."
- Run order in the worker: import (crop or project; the vertical-cross bottom turn; per-face fix) → Turn → Mirror → Ground → blur → tone (Brightness, Highlight rolloff, Colour strength, Tint) → output. The per-face fix applies only when the type has cells (crosses, strips, six face files). Turn bottom cell is read only for Vertical cross. Horizon is read only when Ground is not Keep.
- The Advanced group is one `egui::CollapsingHeader::new("Advanced").id_salt("cube-advanced")`, closed by default, below Brightness. Row order is the spec's order.
- Not built here: per-mip GGX, cube alpha, the 1x1 marker, a slider band, moving Save off the UI thread, the 3D ball.

## Review Focus

1. A face fix set on a cross, then Source type switched to a panorama type: the fix must not turn panorama faces. Test: Task 3, `a_face_fix_is_ignored_for_a_panorama`.
2. Side size 2048 with Reflection softness above 0: the blur must stay bounded and the output must still be 2048. Test: Task 2, `a_large_soft_cube_is_blurred_small_and_comes_back_full_size`.
3. A Side size above the source's cell (256 on a 64x32 panorama): the output is 256, not capped. Test: Task 4, `a_side_size_of_256_makes_a_256_cube`.
4. An HDR sun (linear 1000) with Highlight rolloff: every value stays finite, and luma ends at or below 1. Test: Task 2, `rolloff_keeps_a_bright_sun_finite_and_at_most_white`.
5. A second source opened after Advanced edits: no Turn, Mirror, fix or Ground carries over. Test: Task 4, `a_new_source_resets_the_advanced_settings`.

---

### Task 1: Settings and the per-face fixes

**Files:**
- Modify: `src/cube/mod.rs`, `src/cube/import.rs`, `src/cube/tab.rs` (only the `Settings { .. }` literal in `poll_load`)

**Interfaces:**
- Produces: `Settings { source, softness, brightness, side, turn, mirror, rolloff, colour, tint, ground, horizon, face_turn, face_flip, turn_bottom }` with a hand-written `Default` (values above); `#[derive(Clone, PartialEq, Debug, Serialize)]`, `source` serialised as `"source_type"`, snake_case enums. `enum Ground { Keep, Darken, Colour([u8; 3]) }` and `enum Flip { None, LeftRight, TopBottom }` (`Copy`, `PartialEq`, `Debug`, `Serialize`, `Default` = first variant). `SourceType::has_cells(self) -> bool` (true for HCross, VCross, HStrip, VStrip, Faces). `import::import(img, t, n, turn_bottom: bool) -> Cube`. `import::fix(c: &mut Cube, turn: &[u16; 6], flip: &[Flip; 6])`.

- [ ] **Step 1: Write the failing tests** in `src/cube/import.rs`'s test module:

```rust
#[test]
fn a_face_fix_turns_then_flips_one_face() {
    // Face 0 holds a ramp; turn 90 clockwise then flip left to right is a transpose.
    let n = 3u32;
    let mut c = Cube { size: n, faces: std::array::from_fn(|_| Rgba32F::new(n, n, [0.0; 4])) };
    for (i, p) in c.faces[0].px.iter_mut().enumerate() { *p = [i as f32, 0.0, 0.0, 1.0]; }
    let before = c.clone();
    fix(&mut c, &[90, 0, 0, 0, 0, 0], &[Flip::LeftRight, Flip::None, Flip::None, Flip::None, Flip::None, Flip::None]);
    for y in 0..n { for x in 0..n {
        assert_eq!(c.faces[0].at(x, y), before.faces[0].at(y, x), "({x},{y})");
    }}
    assert_eq!(c.faces[1..], before.faces[1..]);
}

#[test]
fn turn_bottom_off_keeps_the_stored_bottom_cell() {
    // A vertical cross whose -Z cell (col 1, row 3) is a ramp.
    let n = 4u32;
    let mut img = Rgba32F::new(3 * n, 4 * n, [0.0; 4]);
    for y in 0..n { for x in 0..n { *img.at_mut(n + x, 3 * n + y) = [(y * n + x) as f32, 0.0, 0.0, 1.0]; } }
    let kept = import(&img, SourceType::VCross, n, false);
    let turned = import(&img, SourceType::VCross, n, true);
    assert_eq!(kept.faces[5].at(0, 0)[0], 0.0);
    assert_eq!(turned.faces[5].at(0, 0)[0], (n * n - 1) as f32);
}
```

Also in `src/cube/mod.rs`: `the_defaults_change_nothing` asserting `Settings::default()` has `side None, turn 0, mirror false, rolloff None, colour 1, tint [255; 3], ground Ground::Keep, horizon 0, face_turn [0; 6], face_flip [Flip::None; 6], turn_bottom true`, and `has_cells` true for exactly the five types.

- [ ] **Step 2: Run** `cargo.ps1 <ws>/t1-red.log "test --offline -j 2 --lib -- --test-threads=2 cube::"`. Expected: compile errors for `fix`, `Flip` and the 4-argument `import`.

- [ ] **Step 3: Implement.** Copy `rotate` and `flip` from Tessalith `cube/layout.rs:59-91` into `import.rs`; `turn180(f)` becomes `rotate(f, 180)` (delete `turn180`, keep the test-local helper if a test still needs it). `import` turns the vertical cross's −Z cell only when `turn_bottom`. `fix` applies `rotate` then `flip` to each face whose entry is not the default (Tessalith's `orient`). Update every `import(..)` caller to pass `true` for now; `poll_load` builds `Settings { source, softness, ..Settings::default() }`.

- [ ] **Step 4: Run** the Step 2 command. Expected: all `cube::` lib tests pass.

### Task 2: Turn, Mirror, Ground, tone and the bounded blur

**Files:**
- Create: `src/cube/adjust.rs` (add `pub mod adjust;` to `mod.rs`)
- Modify: `src/cube/make.rs` (delete `brighten` and `brightness_one_doubles_linear`; bound `blur`)

**Interfaces:**
- Consumes: `Settings`, `Ground` from Task 1.
- Produces: `adjust::turn(c: &Cube, degrees: f32) -> Cube`, `adjust::mirror(c: &Cube) -> Cube`, `adjust::ground(c: &mut Cube, g: Ground, horizon: f32)` (no-op for `Keep`), `adjust::tone(c: &mut Cube, s: &Settings)`. `make::BLUR_MAX: u32 = 256`, `make::blur_capped(c: &Cube, amount: f64, cap: u32) -> Cube`.

- [ ] **Step 1: Write the failing tests.** Copy from Tessalith `cube/filter.rs` tests, adapted to `Ground`: `ground_colour_replaces_the_lower_hemisphere`, `colour_code_decodes_with_game_gamma`, `gradient_follows_the_pinned_ramp_at_any_horizon`, `blend_weights_at_the_edges`, `horizon_colour_is_the_in_band_mean`, `empty_band_uses_the_nearest_row_above`, `horizon_moves_the_line`. New in `adjust.rs`:
  - `turn_90_moves_east_to_south`: a 16 cube that is red only near world east (`to_world(d)[0] > 0.9`); after `turn(&c, 90.0)` the texel nearest world south (`[0, -1, 0]` through `to_cube`) is red and the one nearest east is not.
  - `mirror_swaps_east_and_west`: same cube; after `mirror`, the texel nearest world west is red.
  - `turn_0_and_keep_change_nothing`: `turn(&c, 0.0) == c`; `ground(&mut c, Ground::Keep, 30.0)` leaves `c` equal.
  - `tone_defaults_change_nothing`: `tone` with `Settings::default()` leaves a cube with values 0.05..3.5 bit-identical.
  - `brightness_one_doubles_linear` (moved from `make.rs`, now through `tone`).
  - `rolloff_keeps_a_bright_sun_finite_and_at_most_white`: a cube of `[1000, 900, 800]`, `rolloff: Some(4.0)`: every RGB finite, Rec. 709 luma ≤ 1 + 1e-5; and a `[4, 4, 4]` cube maps to 1 ± 1e-5 (Tessalith's `highlight_clamp_maps_white_point_to_one`).
  - `colour_strength_zero_is_grey_and_stays_positive`: `[0.8, 0.2, 0.1]` at `colour: 0.0` gives three equal channels; `[1.0, 0.0, 0.0]` at `colour: 2.0` has no channel below 0.
  - `tint_matches_a_game_gamma_multiply`: `[0.5; 3]` with `tint: [255, 128, 255]`: `linear_to_game_gamma(g) == linear_to_game_gamma(0.5) * 128/255` within 1e-5, red unchanged.
  - In `make.rs`: `a_large_soft_cube_is_blurred_small_and_comes_back_full_size`: a 16 cube with one bright texel on face 0 (so a full-size blur differs). `let id = |d| d;` `blur_capped(&c, 0.5, 8)` has size 16 and equals `resample(&blur_capped(&resample(&c, 8, id), 0.5, 8), 16, id)` exactly; `blur_capped(&c, 0.0, 8) == c`.

- [ ] **Step 2: Run** `cargo.ps1 <ws>/t2-red.log "test --offline -j 2 --lib -- --test-threads=2 cube::"`. Expected: compile errors (`adjust` missing); after a stub, the new tests fail.

- [ ] **Step 3: Implement.**
  - `turn`, `mirror` and `ground` are Tessalith's `yaw`, `mirror` and `ground_fill`, with `GroundFill` replaced by `Ground` (`Darken` is `Mode(Gradient)`).
  - `tone` in linear light, per texel, alpha kept, each step skipped at its default: RGB × `2^brightness`; rolloff with white point `w` on Rec. 709 luma; colour strength `y + (v − y)·colour` with `y` the luma, then `max(0)`; tint multiply by `game_gamma_to_linear(t / 255)` per channel.
  - `blur_capped(c, amount, cap)`: when `amount > 0` and `c.size > cap`, run the copied blur on `engine::cube::sample::resample(c, cap, |d| d)`, then return `resample(&blurred, c.size, |d| d)`; otherwise the copied blur as it is. `blur(c, amount)` is `blur_capped(c, amount, BLUR_MAX)`. Add `// shortcut: blurs at most a 256 face; raise BLUR_MAX if a very slight blur on a 1024+ cube looks soft.`

- [ ] **Step 4: Run** the Step 2 command. Expected: all pass, including `bright_sources_clip_and_stay_finite` changed to call `tone` with `brightness: 10.0`.

### Task 3: The worker order, the shape cache, the marker and the preview cap

**Files:**
- Modify: `src/cube/worker.rs`, `src/cube/files.rs`, `src/cube/view.rs`

**Interfaces:**
- Consumes: Tasks 1-2.
- Produces: `worker::compute(job: &Job, cache: &mut Cache) -> Output` where `pub(crate) struct Cache` holds the import cache (key: `Arc<Opened>` by `ptr_eq`, source, `n`, the fix when `has_cells`, `turn_bottom` when VCross) and the shaped cache (the import key plus turn, mirror, ground, horizon when Ground is not Keep, softness). `Cache::shaped_runs: u32` counts shape stage runs; tests read it. `view::PREVIEW_MAX: u32 = 512`.

- [ ] **Step 1: Write the failing tests.**
  - `worker.rs` `an_explicit_side_size_wins_over_the_source_cell`: the 512x256 panorama job with `side: Some(256)` gives `n == 256` and `cube.size == 256`.
  - `worker.rs` `a_face_fix_is_ignored_for_a_panorama`: two jobs on the panorama, one with `face_turn: [90; 6]`; their output cubes are equal.
  - `worker.rs` `a_tone_change_reuses_the_shaped_cube`: call `compute` three times with one `Cache`: softness 0.5; then brightness 1; then colour 0.5. `shaped_runs` is 1. Then turn 30: it is 2.
  - `files.rs` `a_cube_png_records_every_setting`: settings with `turn: 30.0`, `ground: Ground::Colour([1, 2, 3])`, `face_turn[2] = 90`; the marker's settings have `turn` 30, `ground` `{"colour": [1, 2, 3]}`, `face_turn[2]` 90, `side` null, `turn_bottom` true, and still `source_type` "panorama" and `face_size`.
  - `view.rs` `a_big_cube_previews_at_512`: a 1024 cube whose face 0 is 1 at even x and 0 at odd x: `cross_image` is 2048 x 1536, and face 0's cell shows only the even (bright) texels.

- [ ] **Step 2: Run** `cargo.ps1 <ws>/t3-red.log "test --offline -j 2 --lib -- --test-threads=2 cube::"`. Expected: the five tests fail or do not compile.

- [ ] **Step 3: Implement.** `n = side.unwrap_or(face_size(softness, cell))`. Run order as Global Constraints. Each cache is dropped before it is rebuilt, as plan 5's fix did. `cube_png` writes `json!(s)` merged with `face_size` and `inputs`. `cross_image` steps by `k = (n / PREVIEW_MAX).max(1)`. Add `// shortcut: at 2048 the worker holds the imported, shaped and output cubes (about 1.2 GB); drop the import cache first if memory runs short.` Replace the old `shortcut: blur reruns on every Brightness change…` line, which this task resolves.

- [ ] **Step 4: Run** the Step 2 command, then the whole suite (check `Get-Process toolbag` first) to `<ws>/t3-suite.log`. Expected: all pass, 2 ignored, no warnings; `tests/cube_contract.rs` and `tests/cube_tab.rs` unchanged and green.

### Task 4: The Advanced group

**Files:**
- Modify: `src/cube/tab.rs`
- Test: `tests/cube_tab.rs`

**Interfaces:**
- Consumes: `Settings`, `Ground`, `Flip`, `SourceType::has_cells`, `view::FACE_LABELS`, `import::cell`, `make::face_size`.
- Produces: `CubeTab::advanced_rows(&mut self, ui)`, private; the picked face for Fix one side is a private `fix_face: usize` field, reset to 0 on open.

- [ ] **Step 1: Write the failing tests** in `tests/cube_tab.rs` (helper `advanced(h)` clicks the "Advanced" header and runs 3 steps):
  - `advanced_is_closed_until_clicked`: after opening the 64x32 panorama, no `Role::Slider` "Turn". After `advanced`: sliders "Turn" and "Colour strength", checkboxes "Mirror" and "Highlight rolloff", combos "Side size" ("Auto (16 px)") and "Ground" ("Keep the source"), button "No tint" disabled. No "Fix one side" combo, no "Turn bottom cell" checkbox, no "Horizon line" slider.
  - `a_vertical_cross_offers_its_bottom_cell_and_face_fixes`: open a 24x32 gradient PNG (not detected), pick "Vertical cross (3:4)", `advanced`: checkbox "Turn bottom cell" is checked, combos "Fix one side" ("+X east"), "Turn this face" ("None") and "Flip this face" ("None") exist.
  - `the_horizon_line_shows_once_a_ground_is_picked`: set `settings.ground = Ground::Darken` through `state_mut`, `advanced`: slider "Horizon line" exists.
  - `a_side_size_of_256_makes_a_256_cube`: set `settings.side = Some(256)`, settle: `outputs.n == 256`.
  - `a_new_source_resets_the_advanced_settings`: after setting `turn 90`, `mirror true`, `face_turn[0] 90`, open a second panorama: `settings == Settings { source, softness, ..Settings::default() }`.
  - `the_help_text_is_shown`: after `advanced` on the panorama, the Turn and Ground help sentences are on screen verbatim.

- [ ] **Step 2: Run** `cargo.ps1 <ws>/t4-red.log "test --offline -j 2 --test cube_tab -- --test-threads=2"`. Expected: the six new tests fail; the seven old ones pass.

- [ ] **Step 3: Implement** the group with `slider_row` for Turn, White point, Colour strength and Horizon line, `ui.checkbox` for Mirror, Highlight rolloff and Turn bottom cell, `egui::ComboBox::from_label` for Side size, Ground, Fix one side, Turn this face and Flip this face, `ui.color_edit_button_srgb` for Tint and the ground colour, and `help` under each item. Give every colour button `widget_info` with the label "Tint colour" or "Ground colour" (egui-ui-rules 8.2).

- [ ] **Step 4: Run** the Step 2 command, then the whole suite (toolbag check first) to `<ws>/t4-suite.log`. Expected: all pass, 2 ignored, no warnings. NUL-scan every file this plan touched.
