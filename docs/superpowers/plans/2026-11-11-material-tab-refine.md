# Material Tab Refinement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The From normal tab becomes the Material tab: collapsible map sections with thumbnails and their own Save, help behind (?) buttons on all three tabs, a wider Parallax Depth, and a preview panel that hides its 3D options outside the 3D view.

**Architecture:** Task 1 changes words and the Depth range. Task 2 replaces the shared `help` line with a `hint` button, which every tab's column uses. Tasks 3 to 5 rebuild the Material column around one `section` function: header, always-visible notes, collapsible body. Task 6 gates the preview panel's 3D rows. Task 7 is a look at the real window and, if needed, a throwaway build against Tessalith `release/3.0`.

**Tech Stack:** Rust, edition 2021, eframe/egui 0.36.2, egui_kittest 0.36.2. No new crate.

**Spec:** This plan is its own spec. The user's request of 2026-11-11 and their answers are recorded below. Plan 8 (RNM layers) covers the rest of that request and is not written yet.

## The request and the answers

The user's request for this tab, in their words:

- From normal, rename to material tab
- AO, curvature, cavity, parallax and height strong visible subheadings, collapsible, with thumbnails
- Depth min 0.01, max 100
- per map export and save buttons
- repeat height specific settings for parallax
- tooltips in (?) buttons next to options instead of always displayed
- Darkness, rename to ao strength, tooltips for what it does
- 3d preview options shouldn't appear in 2d
- "7.4× deeper than the normal implies", "Levels used: 33 of 256", "Clipped: 0.5 %" all should be in advanced info, same scale text
- I believe mlp preview was fixed in render
- "Surface detail only" makes it sound like something is discarded, so rename it to fine surface detail

The user's answers on 2026-11-11:

1. "Depth" is Parallax Depth, and the range is 1 % to 100 %.
2. Flatten, Material and Compare show under Height and again under Parallax as one setting. Parallax gets no Flatten of its own.
3. Each section gets one Save button that writes only that map. The ticks and "Save N maps" stay.
4. The "White map looks shifted in 3D" warning stays until a White map is looked at in the real window. The user added that this may need the render from Tessalith `release/3.0`.

## Decisions taken in this plan

These are the plan's own calls. The user has not ruled on them.

- Only words on screen change. Module names, `Mode::FromNormal`, `tab::Tab` and marker keys such as `darkness` stay, so saved markers still read.
- The (?) button shows its sentence on hover. The sentence is also the button's AccessKit name, so a screen reader reaches it and a test finds it without hovering.
- Still drawn in the column: warnings, the AO progress bar, live readouts ("About 12 texels", "Fades shapes wider than 42 % of the width", "Black to white spans…", the Green and rebuild lines), the two AO caveat lines, the empty-state line and RNM's "Alpha is kept as it is.". They are not option help.
- A section's thumbnail sits left of its tick. A right-aligned widget inside `ScrollArea::both` can land past the clip (egui-ui-rules 3.8).
- A section starts open when its map is ticked the first time it is drawn, so Curvature and Cavity start closed. Ticking a section opens it. egui keeps the open state for the session, across files.
- Height data and Normal draw their rows whether ticked or not, because Flatten and Normal depth shape the master every map is made from. An unticked Parallax, AO, Curvature or Cavity draws one line: "Tick {name} to make this map."
- Warnings and the AO progress bar are drawn between a section's header and its body, so closing a section never hides them.
- The three "Advanced" headers become "Height advanced", "Parallax advanced" and "Cavity advanced". Three headers with one name cannot be told apart by a screen reader or a test.
- The copy of the Flatten controls under Parallax carries " (shared with Height)" in its names, as "Surface depth (shared with AO)" already does. Source green is not repeated: it describes the source file, not the height.
- "Levels used" turns into a warning under 60 levels today. That warning stays outside Parallax advanced with its own words: "Only {n} of 256 grey levels are used; raise Depth."
- Above about 94 % (Mid-grey) and 97 % (White) the soft knee has no room before the 8/255 margin, so the tallest and deepest relief clips hard, not only the outliers. The Clipped readout shows it.
- A section's Save is enabled when its map is ticked and made, the outputs are full size, no job runs and a folder is set.
- In 2D and on the Cubemap tab the preview panel keeps its heading, the Community Shaders and GPU lines, and both warnings (software rendering, restart after a lost device).
- Task 7 changes no dependency. If the White preview needs `release/3.0`, the move is its own plan and comes after plan 8: `tests/rnm_engine.rs` and `src/from_normal/spike4.rs` compare against `engine::rnm` and `engine::from_normal`, which `release/3.0` no longer has.

## Global Constraints

- Never edit anything under `F:\ai\dds`. `git -C F:\ai\dds\converter status --short --untracked-files=no` and `git -C F:\ai\dds\converter-nif-p1\.worktrees\release-3.0 status --short --untracked-files=no` stay as they were when the plan started.
- Commit only when the user asks. No attribution lines.
- Cargo: `--offline -j 2`, tests with `-- --test-threads=2`, BelowNormal priority, only when no other cargo or rustc runs. Recreate the `cargo.ps1` helper in this plan's workspace, as plans 5 and 6 did. Run long builds in the background.
- Never run a GPU test or launch Texalith while `toolbag.exe` runs (`Get-Process toolbag`). `tests/bundle.rs` uses the GPU and runs in the whole suite.
- Scan every new or changed file for NUL bytes before a task is done.
- egui 0.36.2 only. Read `references/gotchas-egui-0.36.md` of the egui-ui-rules skill before writing collapsing or id code. `get_by_*` in kittest panics on two matches, so every widget a person can press, tick or drag has a unique accessible name in its column. A plain text line may appear twice (Task 4 does this); a test counts those with `query_all_by_label`.
- Exact words:
  - Tab button and column heading: "Material".
  - Slider: "AO strength". Its reset button reads "Reset AO strength to 1".
  - AO strength help: "Makes the shadows this map already has darker or lighter without moving them; leave it at 1 if your renderer has its own AO strength setting, and raise it only if the map will be multiplied in directly."
  - Flatten's right-hand end label: "Fine surface detail".
  - Help button: visible text "?", AccessKit name "Help for {what}: {sentence}".
  - Section toggle names: "Show {name} settings" when closed, "Hide {name} settings" when open.
  - Thumbnail names: "{name} thumbnail" when the map is made, "{name} not made" when it is not.
  - Section Save buttons: "Save Height data", "Save Parallax", "Save Normal", "Save AO", "Save Curvature", "Save Cavity".
  - `{name}` is `view2d::name(kind)`: "Height data", "Parallax", "Normal", "AO", "Curvature", "Cavity".
- Parallax Depth: stored 0.01 to 1.0, shown 1 % to 100 %, default 0.9, logarithmic, no decimals.
- Not built here: anything on the RNM tab beyond its help buttons, the Cubemap collision warning, moving Save off the UI thread, a multilayer preview material, a move to `release/3.0`.

## Review Focus

1. A map that is not square (256x128): its thumbnail keeps the shape and stays at most 64 on the long side. Test: Task 3, `a_thumb_is_at_most_64_on_its_long_side`.
2. A section closed while its warning applies or its job runs: the warning and the progress bar stay on screen. Test: Task 3, `closing_a_section_keeps_its_warning_and_progress`.
3. Depth at both ends on both flat levels: every texel finite, at least 8/255, and at most 1 − 8/255 on Mid-grey or 1.0 on White. Test: Task 1, `depth_runs_from_1_to_100_percent`.
4. A section's Save pressed while a job runs or only a draft is in: nothing is written, because the button is disabled. Test: Task 5, `a_section_save_waits_for_full_outputs`.
5. Flatten dragged on the copy under Parallax: drafts go out while dragging and one full job on release, as on the Height copy. Test: Task 4, `the_shared_flatten_sends_drafts_too`.

---

### Task 1: Words and the Depth range

**Files:**
- Modify: `src/app.rs:257` (tab button), `src/tab.rs` (heading at 343, `HELP_DARKNESS`, the Darkness row at 645, the end label at 455, the Depth row at 541)
- Modify: `src/from_normal/maps.rs:45,132,171-177` (Depth clamp and doc, `match_depth` clamp) and its tests at 472-477
- Test: `tests/ui.rs`, `tests/tab.rs`, unit tests in `src/from_normal/maps.rs`

**Interfaces:**
- Produces: `ParallaxOpts::depth` accepts 0.01 to 1.0. `match_depth` returns 0.01 to 1.0.

- [ ] **Step 1: Write the failing tests**

In `src/from_normal/maps.rs` tests, with the bumpy master the neighbouring parallax tests build:

```rust
#[test]
fn depth_runs_from_1_to_100_percent() {
    let m = /* the bumpy master used by parallax_pivots_on_the_median_and_stays_in_the_margin */;
    for flat in [FlatLevel::MidGrey, FlatLevel::White] {
        let at = |depth| parallax(&m, &ParallaxOpts { depth, flat, ..Default::default() });
        // A White map's untouched surface is exactly 1.0; only its deep end has the margin.
        let top = if flat == FlatLevel::White { 1.0 } else { 1.0 - MARGIN + 1e-6 };
        for d in [0.01, 1.0] {
            let (g, _) = at(d);
            assert!(g.px.iter().all(|v| v.is_finite() && *v >= MARGIN - 1e-6 && *v <= top), "{flat:?} {d}");
        }
        assert!(at(0.01).1.levels_used < at(1.0).1.levels_used);
        assert_eq!(at(0.0).0.px, at(0.01).0.px); // clamped below
        assert_eq!(at(2.0).0.px, at(1.0).0.px); // clamped above
    }
}
```

In `match_depth_gives_one_times_the_normal`, the tall master (`sine(256, 400.0)`, raw value about 31) now gives `1.0`, not `0.9`. Its first assertion, `(0.1..0.9).contains(&d)`, stays.

In `tests/tab.rs`:

```rust
#[test]
fn the_new_words_are_on_the_column() {
    let (h, _rig, _dir) = ready("words");
    h.get_by_role_and_label(Role::Slider, "AO strength");
    h.get_by_role_and_label(Role::Button, "Reset AO strength to 1");
    assert!(h.query_by_role_and_label(Role::Slider, "Darkness").is_none());
    assert!(label_exists(&h, "Fine surface detail"));
    assert!(!label_exists(&h, "Surface detail only"));
    assert!(label_exists(&h, "Material"));
}
```

In `tests/ui.rs`, change the tab name "From normal" to "Material" at lines 368, 398, 494, 509 and 540.

- [ ] **Step 2: Run them and see them fail**

Run: `.\cargo.ps1 test --lib depth_runs -- --test-threads=2`, then `--test tab the_new_words` and `--test ui the_tab_bar`.
Expected: FAIL (clamp at 0.1 and 0.9; no "AO strength"; no "Material" button).

- [ ] **Step 3: Make the changes**

The words from Global Constraints. The Depth row becomes `range: 0.01..=1.0`, `log: true`, the rest unchanged. Both clamps in `maps.rs` become `(0.01, 1.0)`, and the doc comments say 0.01 to 1.0. The flat-relief return of `match_depth` stays 0.9.

- [ ] **Step 4: Run the three test targets and see them pass**

Run: `.\cargo.ps1 test --lib maps -- --test-threads=2`, `--test tab`, `--test ui`.
Expected: PASS. `an_output_slider_sends_a_job_with_the_same_master_options` still passes: a click near the slider's left end gives a depth below 0.9.

---

### Task 2: Help behind (?) buttons

**Files:**
- Modify: `src/tab.rs` (`help` at 105 goes; `slider_row`, `tick` and every `help(` call)
- Modify: `src/rnm/tab.rs` (import at 10; `HELP_MASK`, `HELP_ORDER`, `HELP_FLIP`, `HELP_SHADOWS`)
- Modify: `src/cube/tab.rs` (the `help(` calls between 278 and 412)
- Test: `tests/tab.rs`, `tests/cube_tab.rs`, `tests/rnm_tab.rs`

**Interfaces:**
- Produces, in `src/tab.rs`:
  - `pub fn hint_name(what: &str, text: &str) -> String`, which is `format!("Help for {what}: {text}")`.
  - `pub(crate) fn hint(ui: &mut egui::Ui, what: &str, text: &str)`: a "?" button at least 20 by 20 points with `on_hover_text(text)`, then `widget_info` naming it `hint_name(what, text)`. `widget_info` comes after `ui.add`, because it replaces the label egui set.
- `help` is deleted. `slider_row` keeps its signature and draws `hint(ui, r.name, r.help)` on the row that holds the name, not the row that holds the slider.

`what` is the option's visible name. The ones that are not a row or combo label:

| Call site | `what` |
|---|---|
| a section tick's purpose sentence | the tick's name |
| Flat level (the chosen level's sentence) | "Flat level" |
| Invert | "Invert" |
| Source green | "Source green" |
| RNM mask, per layer | `format!("{} mask", id.name())` |
| RNM order, flip, shadows | "Order", "Flip this layer", "Bend mostly in shadows" |
| Cubemap `help` after a checkbox or combo | that widget's label |

A combo or checkbox and its hint share one `ui.horizontal`. The empty-state line at `src/tab.rs:392` becomes a wrapped weak label and stays.

- [ ] **Step 1: Write the failing tests**

In `tests/tab.rs`, `NORMAL_SOURCE_HELP` becomes pairs and `every_help_sentence_is_visible` is replaced:

```rust
const NORMAL_SOURCE_HELP: [(&str, &str); 11] = [
    ("Output size", "Sets how sharp the maps are and how long they take; lower it for quick tries, and raise it for the final maps."),
    ("Edges", /* the Edges sentence, unchanged */),
    ("Flatten broad shapes", /* unchanged */),
    ("Depth", /* unchanged */),
    ("Flat level", /* the Mid-grey sentence */),
    ("Reach", /* unchanged */),
    ("AO strength", "Makes the shadows this map already has darker or lighter without moving them; leave it at 1 if your renderer has its own AO strength setting, and raise it only if the map will be multiplied in directly."),
    ("Surface depth", /* unchanged */),
    ("Line thickness", /* unchanged */),
    ("Contrast", /* unchanged */),
    ("Crevice width", /* unchanged */),
];

#[test]
fn every_option_has_a_help_button_and_no_sentence_is_drawn() {
    // set-up as every_help_sentence_is_visible had it
    for (what, text) in NORMAL_SOURCE_HELP {
        h.get_by_role_and_label(Role::Button, &texalith::tab::hint_name(what, text));
        assert!(!label_exists(&h, text), "still drawn: {text}");
    }
    assert!(label_exists(&h, "Estimated from the normal, not baked from a mesh."));
    assert!(label_exists(&h, "If you have a real height, load it."));
}
```

In `tests/cube_tab.rs`, `the_help_text_is_shown` becomes `help_sits_behind_buttons`: after `advanced(&mut h)`, `has(&h, Role::Button, &hint_name("Turn", TURN_SENTENCE))` and `has(&h, Role::Button, &hint_name("Ground", GROUND_SENTENCE))`, and `!shown(&h, TURN_SENTENCE)`.

In `tests/rnm_tab.rs`, add `the_mask_help_is_a_button`: with a base and a Detail layer filled as the neighbouring tests do, `get_by_role_and_label(Role::Button, &hint_name("Detail mask", MASK_SENTENCE))`.

- [ ] **Step 2: Run them and see them fail**

Run: `.\cargo.ps1 test --test tab every_option -- --test-threads=2` (and `cube_tab help_sits`, `rnm_tab the_mask_help`).
Expected: FAIL to compile: `hint_name` is not defined.

- [ ] **Step 3: Add `hint_name` and `hint`, delete `help`, convert every call site**

- [ ] **Step 4: Run all four UI test targets**

Run: `.\cargo.ps1 test --test tab --test rnm_tab --test cube_tab --test ui -- --test-threads=2`
Expected: PASS, including `the_column_fits_at_the_checked_widths`, `the_three_columns_fit_at_the_checked_widths` and `nothing_leaves_the_window_at_the_checked_widths`: no (?) button may sit past its column at 1100.

---

### Task 3: Sections with thumbnails

**Files:**
- Modify: `src/tab.rs` (`tick` goes; `draw_column`, `height_rows`, `parallax_rows`, `normal_rows`, `ao_rows`, `curvature_rows`, `cavity_rows`; a new `thumbs` field beside `compare_tex`)
- Modify: `src/view2d.rs` (add `thumb`), `src/from_normal/settings.rs` (add `Ticks::get_mut`)
- Test: `tests/tab.rs`

**Interfaces:**
- Produces:
  - `impl Ticks { pub fn get_mut(&mut self, k: MapKind) -> &mut bool }` in `settings.rs`; `Rnm` and `Cube` are `unreachable!`.
  - `pub fn thumb(out: &Outputs, k: MapKind) -> Option<egui::ColorImage>` in `view2d.rs`: `None` when `out` holds no map of kind `k` (Normal reads `out.normal`). Otherwise the map with every `n`-th texel taken, `n = ceil(long side / 64)`, so the long side is at most 64.
  - `fn section(&mut self, ui: &mut egui::Ui, kind: MapKind, always: fn(&mut Tab, &mut egui::Ui), body: fn(&mut Tab, &mut egui::Ui))` on `Tab`. Tasks 4 and 5 put rows into the `body` functions.
- `Tab` gains `thumbs: Option<(u64, HashMap<MapKind, egui::TextureHandle>)>`, rebuilt when `outputs.id` changes and cleared in `accept`.

`section` draws, in order:

1. `ui.separator()`.
2. The header, through `egui::collapsing_header::CollapsingState::load_with_default_open(ctx, ui.make_persistent_id(("section", kind)), ticked)` and `show_header`: the thumbnail fitted into 36 by 36 points (a stroked empty box when not made), then the tick as `ui.checkbox(tick, RichText::new(name).strong().size(15.0))`, then `hint(ui, name, purpose)`.
3. When the tick turned on this frame, `set_open(true)` on the header response before the body.
4. `always(self, ui)`.
5. The body, indented. The toggle button that `body` returns gets `widget_info` with the Show or Hide name: `show_header`'s arrow has no AccessKit name of its own.

`draw_column` calls `section` for Height data, Parallax, Normal (height source only), AO, Curvature and Cavity, in that order. Parallax is no longer indented under Height.

What goes in `always`: `NOT_SAVED` (Height), `WHITE_PREVIEW` (Parallax), "Scale unknown" (Normal), the "Making AO…" bar (AO). Everything else a `*_rows` function draws today is body. Parallax, AO, Curvature and Cavity bodies draw "Tick {name} to make this map." when unticked.

- [ ] **Step 1: Write the failing tests**

In `tests/tab.rs`:

```rust
#[test]
fn a_thumb_is_at_most_64_on_its_long_side() {
    let mut o = outputs(1);
    o.height.0 = Gray { width: 256, height: 128, px: vec![0.5; 256 * 128] };
    o.curvature = None;
    assert_eq!(texalith::view2d::thumb(&o, MapKind::HeightData).unwrap().size, [64, 32]);
    assert_eq!(texalith::view2d::thumb(&o, MapKind::Ao).unwrap().size, [4, 4]);
    assert!(texalith::view2d::thumb(&o, MapKind::Curvature).is_none());
}

#[test]
fn sections_have_a_tick_a_thumbnail_and_a_toggle() {
    let (h, _rig, _dir) = ready("sections");
    let panel = /* PanelState "column" outer_rect, as the_column_fits_at_the_checked_widths reads it */;
    for name in ["Height data", "Parallax", "AO"] {
        h.get_by_role_and_label(Role::CheckBox, name);
        h.get_by_role_and_label(Role::Button, &format!("Hide {name} settings"));
        let t = h.get_by_role_and_label(Role::Image, &format!("{name} thumbnail")).rect();
        assert!(t.max.x <= panel.max.x && t.width() <= 36.5 && t.height() <= 36.5, "{name}: {t:?}");
    }
    for name in ["Curvature", "Cavity"] {
        h.get_by_role_and_label(Role::Button, &format!("Show {name} settings"));
    }
    assert!(h.query_by_role_and_label(Role::CheckBox, "Normal").is_none()); // a normal source
}

#[test]
fn ticking_a_closed_section_opens_it() {
    let (mut h, rig, _dir) = ready("tick-opens");
    assert!(h.query_by_role_and_label(Role::Slider, "Line thickness").is_none());
    h.get_by_role_and_label(Role::CheckBox, "Curvature").click_accesskit();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Slider, "Line thickness");
    h.get_by_role_and_label(Role::Button, "Hide Curvature settings");
    assert!(jobs(&rig).last().unwrap().settings.ticks.curvature);
}

#[test]
fn an_unmade_map_says_so() {
    let (mut h, rig) = rig(1280.0);
    h.state_mut().accept(normal_source("unmade"));
    let mut o = outputs(1);
    (o.curvature, o.cavity) = (None, None);
    rig.replies.send(Reply::Done(Box::new(o))).unwrap();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Image, "Curvature not made");
    h.get_by_role_and_label(Role::Button, "Show Curvature settings").click_accesskit();
    h.run_steps(3);
    assert!(label_exists(&h, "Tick Curvature to make this map."));
}

#[test]
fn closing_a_section_keeps_its_warning_and_progress() {
    let (mut h, _rig) = rig(1280.0);
    h.state_mut().accept(normal_source("closed")); // a job runs, no reply yet
    h.state_mut().settings.as_mut().unwrap().parallax.flat = FlatLevel::White;
    h.run_steps(3);
    for name in ["Parallax", "AO"] {
        h.get_by_role_and_label(Role::Button, &format!("Hide {name} settings")).click_accesskit();
        h.run_steps(3);
    }
    assert!(h.query_by_role_and_label(Role::Slider, "Depth").is_none());
    assert!(h.query_by_role_and_label(Role::Slider, "Reach").is_none());
    assert!(label_exists(&h, "This preview pivots on mid-grey, so a White map looks shifted in 3D."));
    assert_eq!(h.query_all_by_role(Role::ProgressIndicator).count(), 1);
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `.\cargo.ps1 test --test tab -- --test-threads=2`
Expected: FAIL to compile: `view2d::thumb` is not defined.

- [ ] **Step 3: Add `Ticks::get_mut`, `view2d::thumb`, the `thumbs` cache and `section`; move each map's rows into `always` and `body` functions**

- [ ] **Step 4: Run the tab and ui targets**

Run: `.\cargo.ps1 test --test tab --test ui -- --test-threads=2`
Expected: PASS. `every_option_has_a_help_button_and_no_sentence_is_drawn` still passes, because it ticks Curvature and Cavity before the first frame, so they start open. `save_is_visible_without_scrolling` still passes.

---

### Task 4: Flatten shown twice, and Parallax advanced

**Files:**
- Modify: `src/tab.rs` (the Flatten block of `height_rows`, the Parallax body, the three Advanced headers, `compare_strip`)
- Test: `tests/tab.rs`

**Interfaces:**
- Consumes: `section` and the body functions from Task 3.
- Produces: `fn flatten_rows(&mut self, ui: &mut egui::Ui, suffix: &str)` on `Tab`: the Flatten slider, its two end labels, the fade readout, the Material combo and the Compare button. Every widget name in it ends in `suffix` (the slider row, its reset, its hint, the combo label, the Compare button), and the block sits in `ui.push_id(suffix, ..)`. It sets `self.dragging` from the slider. The end labels and the fade readout are plain text and read the same in both copies.

The Height body calls `flatten_rows(ui, "")`. The Parallax body calls `flatten_rows(ui, " (shared with Height)")` after Flat level, for a normal source only. With Compare on, `flatten_rows` draws the strip under each copy, the second with " (shared with Height)" on its three names. (Changed after the final review: one strip under Height left the second Compare button with nothing to show when Height was closed.)

Task 1's `the_new_words_are_on_the_column` changes here: "Fine surface detail" is now counted, `h.query_all_by_label("Fine surface detail").count() == 2`, and "Surface detail only" is counted as 0.

The three collapsing headers are renamed as in Decisions. Parallax advanced holds Smooth sharp steps, Invert, then the three readouts, each drawn by one small-text helper so they share a size:

- `"{:.1}× deeper than the normal implies"`
- `"Levels used: {} of 256"`
- `"Clipped: {:.1} %"`

The under-60 warning goes into the Parallax `always` function with the words from Decisions.

- [ ] **Step 1: Write the failing tests**

```rust
const SHARED: &str = "Flatten broad shapes (shared with Height)";

#[test]
fn flatten_shows_under_height_and_parallax_as_one_setting() {
    let (mut h, _rig, _dir) = ready("twice");
    h.get_by_role_and_label(Role::Slider, "Flatten broad shapes");
    h.get_by_role_and_label(Role::Slider, SHARED);
    h.state_mut().settings.as_mut().unwrap().master.flatten = 0.8;
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, &format!("Reset {SHARED} to 0.3")).click_accesskit();
    h.run_steps(2);
    assert_eq!(h.state().settings.as_ref().unwrap().master.flatten, 0.3);
}

#[test]
fn the_shared_flatten_sends_drafts_too() {
    // First close Height ("Hide Height data settings", click_accesskit), so the copy under Parallax sits near the top
    // of the 900 px rig: a pointer drag misses a slider below the fold.
    // Then dragging_flatten_sends_drafts_and_release_sends_a_full_job, on the slider named SHARED.
}

#[test]
fn a_height_source_has_no_flatten() {
    // accept(height_source(..)), outputs in: no slider named "Flatten broad shapes" or SHARED
}

#[test]
fn the_parallax_readouts_live_in_advanced() {
    let (mut h, _rig, _dir) = ready("readouts"); // outputs(1): times_normal 9.6, levels_used 61, clipped 0.0
    let lines = ["9.6× deeper than the normal implies", "Levels used: 61 of 256", "Clipped: 0.0 %"];
    assert!(lines.iter().all(|l| !label_exists(&h, l)));
    h.get_by_label("Parallax advanced").click_accesskit();
    h.run_steps(3);
    assert!(lines.iter().all(|l| label_exists(&h, l)));
}

#[test]
fn few_levels_warn_outside_advanced() {
    // outputs(1) with levels_used: 33
    let line = "Only 33 of 256 grey levels are used; raise Depth.";
    assert!(label_exists(&h, line));
    h.get_by_role_and_label(Role::Button, "Hide Parallax settings").click_accesskit();
    h.run_steps(3);
    assert!(label_exists(&h, line));
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `.\cargo.ps1 test --test tab -- --test-threads=2`
Expected: FAIL: no slider named "Flatten broad shapes (shared with Height)", no "Parallax advanced".

- [ ] **Step 3: Extract `flatten_rows`, call it twice, rename the headers, move the readouts and the warning**

- [ ] **Step 4: Run the tab and ui targets**

Run: `.\cargo.ps1 test --test tab --test ui -- --test-threads=2`
Expected: PASS.

---

### Task 5: Save per section

**Files:**
- Modify: `src/tab.rs` (`save` at 764, each section body's last row)
- Test: `tests/tab.rs`

**Interfaces:**
- Consumes: the body functions from Task 3.
- Produces: `fn save(&mut self, only: Option<MapKind>)` on `Tab`. `None` is today's Save. `Some(k)` keeps only kind `k` of `save::plan`'s result, then reviews, asks and writes as today. `save::plan` is unchanged, so a Parallax saved alone still carries its master's hash.

Each section body ends with its Save button from Global Constraints, enabled by the rule in Decisions. An unticked section has no Save button except Height data and Normal, whose bodies always draw; theirs is disabled while unticked.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_section_save_writes_only_its_map() {
    let (mut h, _rig, dir) = ready("one");
    press(&mut h, "Save AO");
    assert!(dir.join("rock_ao.png").is_file());
    assert!(!dir.join("rock_height.png").exists() && !dir.join("rock_parallax.png").exists());
    assert!(matches!(&h.state().status, Some(Ok(m)) if m.starts_with("Saved 1 map to ")));
}

#[test]
fn a_section_save_waits_for_full_outputs() {
    let (mut h, rig) = rig(1280.0);
    h.state_mut().accept(normal_source("wait"));
    h.run_steps(2);
    let off = |h: &Harness<'_, Tab>| h.get_by_role_and_label(Role::Button, "Save AO").accesskit_node().is_disabled();
    assert!(off(&h)); // running, nothing in
    rig.replies.send(Reply::Done(Box::new(Outputs { draft: true, ..outputs(1) }))).unwrap();
    h.run_steps(2);
    assert!(off(&h)); // a draft
    h.state_mut().settings.as_mut().unwrap().ticks.height = false;
    h.run_steps(2);
    let id = jobs(&rig).last().unwrap().id; // the untick sent a job
    rig.replies.send(Reply::Done(Box::new(outputs(id)))).unwrap();
    h.run_steps(2);
    assert!(!off(&h)); // full outputs, nothing running
    assert!(h.get_by_role_and_label(Role::Button, "Save Height data").accesskit_node().is_disabled()); // unticked
}

#[test]
fn a_section_save_asks_about_its_own_file_only() {
    let (mut h, _rig, dir) = ready("ask");
    let old = foreign(&dir); // rock_ao.png, not Texalith's
    press(&mut h, "Save AO");
    assert!(label_exists(&h, REVIEW));
    assert!(label_exists(&h, "rock_ao.png") && !label_exists(&h, "rock_height.png"));
    press(&mut h, "Cancel");
    assert_eq!(std::fs::read(dir.join("rock_ao.png")).unwrap(), old);
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `.\cargo.ps1 test --test tab a_section_save -- --test-threads=2`
Expected: FAIL: no button named "Save AO".

- [ ] **Step 3: Give `save` its `only` argument and add the buttons**

- [ ] **Step 4: Run the tab and save targets**

Run: `.\cargo.ps1 test --test tab --test save -- --test-threads=2`
Expected: PASS, with `save_without_conflicts_writes_at_once` ("Save 3 maps") unchanged.

---

### Task 6: 3D options only in the 3D view

**Files:**
- Modify: `src/app.rs:328-384` (`controls`)
- Test: `tests/ui.rs`

**Interfaces:**
- Consumes: `App::mode`, `App::flat`.

Form, Parallax scale and its note, Reset view, the orbit hint and "Last frame" draw only when `self.mode != Mode::Cube && !self.flat`. The heading, the Community Shaders and GPU lines and both warnings always draw.

- [ ] **Step 1: Write the failing tests**

```rust
fn has_3d_options(h: &Harness<'_, App>) -> bool {
    h.query_by_role_and_label(Role::Slider, "Parallax scale").is_some()
        && h.query_by_role_and_label(Role::Button, "Reset view").is_some()
        && h.query_by_role_and_label(Role::ComboBox, "Form").is_some()
}

#[test]
fn the_3d_options_show_only_with_the_3d_view() {
    let (mut h, rig) = rig((1280.0, 800.0));
    rig.replies.send(Reply::Ready { adapter: "A".into(), warp: true, fingerprint: "0123456789abcdef".into() }).unwrap();
    h.run_steps(2);
    assert!(has_3d_options(&h));
    h.get_by_role_and_label(Role::Button, "2D").click();
    h.run_steps(2);
    assert!(h.query_by_role_and_label(Role::Slider, "Parallax scale").is_none());
    assert!(h.query_by_role_and_label(Role::Button, "Reset view").is_none());
    assert!(h.query_by_label("Software rendering: the preview will be slow.").is_some());
    assert!(h.query_by_label("Community Shaders").is_some());
    h.get_by_role_and_label(Role::Button, "3D").click();
    h.run_steps(2);
    assert!(has_3d_options(&h));
    tab_button(&mut h, "Cubemap");
    assert!(h.query_by_role_and_label(Role::Slider, "Parallax scale").is_none());
}
```

In `nothing_leaves_the_window_at_the_checked_widths`, the "Reset view" and "Parallax scale" check runs only when `tab != "Cubemap"`.

- [ ] **Step 2: Run it and see it fail**

Run: `.\cargo.ps1 test --test ui the_3d_options -- --test-threads=2`
Expected: FAIL: "Parallax scale" is still there in 2D.

- [ ] **Step 3: Gate the rows in `controls`**

- [ ] **Step 4: Run the ui target**

Run: `.\cargo.ps1 test --test ui -- --test-threads=2`
Expected: PASS.

---

### Task 7: The whole suite, the real window, and the White preview

No commit comes out of this task. Its results go into the ledger and to the user.

**Files:**
- Modify, only on outcome A: `src/tab.rs` (`WHITE_PREVIEW` and its `warn` call), `tests/tab.rs` (`closing_a_section_keeps_its_warning_and_progress` then uses `NOT_SAVED` with Height closed)

- [ ] **Step 1: Run the whole suite**

Run, with no `toolbag.exe`: `.\cargo.ps1 test -- --test-threads=2`
Expected: every test passes, 0 warnings.

- [ ] **Step 2: Look at the Material tab in the real window**

Build and launch the debug exe, with computer-use access requested by the name `texalith.exe`. Open a tiling normal map with clear relief. At window widths near 1100, 1280 and 1600 check: the six headings read as headings; each thumbnail shows its map; (?) shows its sentence on hover; a section's Save and "Save N maps" are both reachable; nothing is cut at the column's right edge. Write what was seen into the ledger.

- [ ] **Step 3: Look at a White map in 3D**

"Shifted" is defined before looking: Form Plane, Parallax scale 1, the camera tilted to a low angle and left alone. Switch Flat level between Mid-grey and White. The preview is shifted if the whole pattern slides the same way across the plane's edges on White, by more than the relief itself changes. Take a screenshot of each.

- [ ] **Step 4, outcome A (not shifted): remove `WHITE_PREVIEW`, its `warn` call and its use in the test, and rerun `--test tab`**

- [ ] **Step 4, outcome B (shifted): keep the warning and try the build against `release/3.0`**

1. `git worktree add --detach <scratchpad>\texalith-v3 HEAD`.
2. In that worktree's `Cargo.toml` only, point `render3d`, `nif` and `engine` at `F:/ai/dds/converter-nif-p1/.worktrees/release-3.0/crates/`.
3. With `CARGO_TARGET_DIR` set to `F:\ai\matierial\target\v3-spike`: `cargo check --offline -j 2 --lib --bins`.
4. If it builds, run it and repeat Step 3. If it does not, record the error count and the first 20 errors, with missing-crate errors under `--offline` listed apart from API errors, and stop. `render3d::locate` and `engine::{from_normal, rnm}` are known to be gone there.
5. `git worktree remove --force <scratchpad>\texalith-v3`, delete `target\v3-spike`, and run both never-edit checks from Global Constraints and `git status --short` here.

- [ ] **Step 5: Report**

Tell the user what the White map looked like on each render that ran, and whether moving to `release/3.0` builds. The move itself is a later plan, after plan 8.
