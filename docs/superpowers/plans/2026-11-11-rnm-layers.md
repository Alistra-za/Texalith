# RNM Layer List and Save Names Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The RNM tab blends any number of layers that the user adds, removes and reorders (arrows or drag and drop), with every setting inside its layer, and a save beside its own source gets a numbered name on the RNM and Cubemap tabs instead of a blocking warning.

**Architecture:** Task 1 adds one shared name function and removes both warnings. Task 2 replaces the five fixed slots (`LayerId`, `Settings`, `Inputs`) with a list of layers through the blend, the worker, the marker, the sibling search and the tab, and redraws the column as one block per layer. Task 3 adds drag and drop on top of the same `move_layer` the arrows use. Task 4 is a look at the real window.

**Tech Stack:** Rust, edition 2021, eframe/egui 0.36.2, egui_kittest 0.36.2. No new crate.

**Spec:** This plan is its own spec. The user's request of 2026-11-11 and their answer are recorded below. Plan 7 (`2026-11-11-material-tab-refine.md`) covered the Material tab part of the same request.

## The request and the answers

The user's request for this tab, in their words:

- remove hardcoded layers, replace with add layers (gets rid of oddities like bump2 being named directly, allows scalable number of layers)
- advanced section can go, allow drag an drop plus arrows to reorganize layers directly, all settings should live in there layer.
- "The output, teeth_n.png, would replace the base, so choose another folder." is annoying, just append a 1 to the suffix on export

The user's answer on 2026-11-11:

1. The Cubemap tab has the same blocking line ("The output, sky_e.png, would replace the source, so choose another folder."). Asked whether this plan should fix it too, the user said: "Yes, include it".

## Decisions taken in this plan

These are the plan's own calls. The user has not ruled on them.

- A layer is one of three kinds: Bent, Bump, Detail. The kind is chosen when the layer is added and does not change; to change it, remove the layer and add another.
- A layer is named by its place: "Layer 2: Bump". The place is the blend order, top first.
- Opening a base still adds a layer for each file found beside it, in the old order (bent, bump, bump2, detail, detail2) and with the old strengths (0.35, 0.4, 0.4, 0.6, 0.3). A folder that opened to a result before this plan opens to the same pixels after it. Task 2 pins that with values recorded from the old code.
- The file-name vocabulary (`_bump2`, `_detailnorm2`, `_bentmask` and the rest) stays, because the files on disk are named that way. It is private to `src/rnm/files.rs` and is no longer a name on screen.
- Each Bent layer has its own "Bend mostly in shadows", AO map and "Focus on crevices". The AO map row and the Focus slider show only while "Bend mostly in shadows" is ticked.
- Behaviour that changes without being asked for:
  - An `_ao` file beside the base loads only into a Bent layer. With no Bent layer it is not loaded; a Bent layer added later takes it.
  - A mask file beside the base whose layer file is missing is not loaded.
  - A layer's file row has "Choose…" and no clear button. "Remove layer" takes the layer away.
  - The saved marker's `settings` change shape: one `layers` array in blend order replaces `order`, `use_ao`, `ao_power`, the `layers` map and `inputs`. Nothing reads the old shape.
  - A layer found as a second detail starts at 0.3 and its reset button says 0.3. A Detail layer added by hand starts at 0.6.
- Layers added by hand go when a new base opens, as the slots do today.
- "Add layer" lists the three kinds; picking one opens the file picker, and the layer is added only if a file is picked. That handler is glue over tested functions and has no automated test, like "Choose…" today. Task 4 presses it once by hand.
- Drag and drop uses a drag handle and the layer's title as the drag source. Sliders inside a layer do not drag the layer. The drop position is worked out from the pointer's height against the layer blocks, so no `dnd_drop_zone` is used (it repaints its frame and takes any payload; gotchas, section 4).
- The numbered save name is the first of `teeth_n.png`, `teeth_n1.png`, `teeth_n2.png`, … that is neither the base nor a layer, mask or AO file in use. For the Cubemap tab it is the first that is none of the source files. Only a file the tab has open pushes the number up. A file that merely exists under that name goes through the usual overwrite check: a Texalith file is replaced, anything else opens the dialog.
- The Save button's hover text names the file ("Saves as teeth_n1.png"). After a save the status line names it too.
- `export::check` still returns `Verdict::Source` when a planned path is the opened file under another spelling. That net stays.

## Global Constraints

- Before Task 1: plan 7 must be committed. `git -C F:\ai\matierial status --short` must show nothing but this plan file. If it shows more, stop and ask the user; this plan's review would otherwise include plan 7's changes.
- Never edit anything under `F:\ai\dds`. `git -C F:\ai\dds\converter status --short --untracked-files=no` and `git -C F:\ai\dds\converter-nif-p1\.worktrees\release-3.0 status --short --untracked-files=no` stay as they were when the plan started.
- This plan runs against Tessalith main. `tests/rnm_engine.rs` compares with `engine::rnm`, which `release/3.0` no longer has.
- Commit only when the user asks. No attribution lines.
- Cargo: `--offline -j 2`, tests with `-- --test-threads=2`, BelowNormal priority, only when no other cargo or rustc runs. The helper is at `F:\ai\matierial\.superpowers\sdd\2026-11-11-rnm-layers.md\cargo.ps1`; run it from that folder (`.\cargo.ps1 test --test rnm_tab`). Run long builds in the background.
- Never run a GPU test or launch Texalith while `toolbag.exe` runs (`Get-Process toolbag`). `tests/bundle.rs` uses the GPU and runs in the whole suite.
- Scan every new or changed file for NUL bytes before a task is done.
- egui 0.36.2 only. Read sections 3 and 4 of `references/gotchas-egui-0.36.md` in the egui-ui-rules skill before Task 2's column and Task 3. `get_by_*` in kittest panics on two matches, so every widget a person can press, tick or drag has a unique accessible name in its column.
- Exact words. `{n}` is the layer's place, counted from 1.
  - Subheading: "Layers", with the help "Layers blend from the top down, so each layer sits over the ones above it." (`hint` named for "Layers").
  - Layer title: "Layer {n}: Bent", "Layer {n}: Bump", "Layer {n}: Detail".
  - Header buttons: "▲" named "Move layer {n} up", "▼" named "Move layer {n} down", "✕" named "Remove layer {n}". Drag source: "≡" and the title, named "Drag layer {n}", hover text "Drag to reorder".
  - File row: "None" when empty; "Choose…" named "Choose file for layer {n}".
  - Strength slider: visible "Strength", named "Layer {n} strength". For a Bump layer: visible "Depth", named "Layer {n} depth", shown as 2k texels as today.
  - Flip: visible "Flip this layer", named "Flip layer {n}", help named for "Layer {n} flip".
  - Mask: visible "Where it applies", help named for "Layer {n} mask"; row named "Choose mask for layer {n}" and "Clear mask for layer {n}"; "No mask" when empty.
  - Bent only: visible "Bend mostly in shadows", named "Bend layer {n} mostly in shadows", help named for "Layer {n} bend mostly in shadows"; visible "AO map", row named "Choose AO map for layer {n}" and "Clear AO map for layer {n}", "No AO map" when empty; slider visible "Focus on crevices", named "Layer {n} focus on crevices".
  - Help sentences keep their present words (`HELP_STRENGTH`, `HELP_DEPTH`, `HELP_MASK`, `HELP_ORDER`, `HELP_FLIP`, `HELP_SHADOWS`, `HELP_FOCUS`).
  - "Add layer" menu items and their hover text: "Bent" ("A bent normal map; leans the base toward it, mostly where the AO map is dark."), "Bump" ("A grey height image; its slopes become bumps over the base."), "Detail" ("A tangent-space normal map; its detail is laid over the base.").
  - With a base and no layer: "No layers yet. Add one to blend it into the base."
  - Save button hover: "Saves as {file name}".
  - Gone from the screen: "Advanced", "Order", the two "would replace … so choose another folder." lines.

## Review Focus

- A file finishes loading after its layer was removed: nothing is added, no error shows. Test: `a_file_for_a_removed_layer_is_dropped` (Task 2).
- Two layers with the same settings and different files swap places: a new blend is sent and the result changes. Test: `swapping_two_equal_layers_sends_a_job` (Task 2).
- A layer is dropped where it already is: the order stays and no blend is sent. Test: `dropping_a_layer_on_itself_changes_nothing` (Task 3).
- `rock_n1.png` is already in the folder and is not Texalith's: the overwrite dialog opens on Keep and the base is untouched. Test: `a_foreign_n1_file_opens_the_dialog` (Task 1).
- The numbered name is a layer file in use: the save moves to the next number and the layer file is untouched. Test: `an_output_name_used_by_a_layer_moves_on` (Task 1).
- Twelve layers: the blend arrives, Save stays in view, the header buttons stay inside the column. Test: `twelve_layers_keep_save_in_view` (Task 2).

---

### Task 1: A save beside its source gets a number

**Files:**
- Modify: `src/export.rs` (add `free_name` and its test)
- Modify: `src/rnm/files.rs` (remove `collides` and the test `only_a_base_whose_output_has_its_own_name_collides`)
- Modify: `src/rnm/tab.rs:472-535` (`collides`, `save_rows`, `save`, `write`)
- Modify: `src/cube/files.rs:22-26` (remove `collides` and the test `the_output_collides_only_with_its_own_source`)
- Modify: `src/cube/tab.rs:426-478` (`collides`, `save_rows`, `save`, `write`)
- Test: `tests/rnm_tab.rs`, `tests/cube_tab.rs`
- Modify: `tests/ui.rs:129` (the comment on `rnm_filled` names the collision line; reword it)

**Interfaces:**
- Produces: `pub fn export::free_name(folder: &Path, name: &str, taken: &[&Path]) -> String`. It returns `name`, or `name` with 1, 2, … before its extension: the first whose path in `folder` is none of `taken`. Paths are compared without regard to case or slash direction.
- Produces in each tab: `fn save_name(&self) -> Option<String>` (private), used by `save_rows`, `save` and `write`. RNM passes the base and every file in `self.slots` as `taken`; Cubemap passes every path in `opened.sources`.

- [ ] **Step 1: Write the failing unit test in `src/export.rs`**

```rust
#[test]
fn a_taken_name_gets_the_first_free_number() {
    let d = Path::new("C:/tex/rock");
    let (base, layer) = (d.join("rock_n.png"), d.join("ROCK_N1.png"));
    assert_eq!(free_name(d, "rock_n.png", &[]), "rock_n.png");
    assert_eq!(free_name(Path::new("C:/tex/out"), "rock_n.png", &[&base]), "rock_n.png");
    assert_eq!(free_name(d, "rock_n.png", &[&base]), "rock_n1.png");
    assert_eq!(free_name(Path::new("C:\\tex\\rock"), "rock_n.png", &[&base, &layer]), "rock_n2.png");
    assert_eq!(free_name(d, "Rock_cnr.png", &[&d.join("rock_cnr.png")]), "Rock_cnr1.png");
}
```

- [ ] **Step 2: Write the failing tab tests**

In `tests/rnm_tab.rs`, replace `the_base_folder_collides_and_save_waits_for_another_folder` with:

```rust
const OLD_LINE: &str = "The output, rock_n.png, would replace the base, so choose another folder.";

#[test]
fn a_save_beside_the_base_gets_a_1() {
    let d = temp("beside");
    let b = base(&d, "rock_n.png");
    layer(&d, "rock_detail.png");
    let before = std::fs::read(&b).unwrap();
    let mut h = rig();
    open(&mut h, &b);
    assert!(!shown(&h, OLD_LINE));
    press(&mut h, "Save");
    let bytes = std::fs::read(d.join("rock_n1.png")).expect("the output was written");
    assert_eq!(marker::read(&bytes).unwrap().kind, MapKind::Rnm);
    assert_eq!(std::fs::read(&b).unwrap(), before);
    match &h.state().status {
        Some(Ok(m)) => assert!(m.starts_with("Saved rock_n1.png to "), "{m}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_cnr_base_saves_as_cnr1_beside_itself() {
    let d = temp("beside-cnr");
    let b = base(&d, "rock_cnr.png");
    let mut h = rig();
    open(&mut h, &b);
    press(&mut h, "Save");
    assert!(d.join("rock_cnr1.png").is_file());
}

#[test]
fn a_foreign_n1_file_opens_the_dialog() {
    let d = temp("beside-foreign");
    let b = base(&d, "rock_n.png");
    let foreign = layer(&d, "rock_n1.png");
    let (base_before, before) = (std::fs::read(&b).unwrap(), std::fs::read(&foreign).unwrap());
    let mut h = rig();
    open(&mut h, &b);
    press(&mut h, "Save");
    assert!(shown(&h, "Some files will be kept"));
    press(&mut h, "Cancel");
    assert_eq!(std::fs::read(&foreign).unwrap(), before);
    assert_eq!(std::fs::read(&b).unwrap(), base_before);
}

#[test]
fn an_output_name_used_by_a_layer_moves_on() {
    let d = temp("beside-layer");
    let b = base(&d, "rock_n.png");
    let used = layer(&d, "rock_n1.png");
    let before = std::fs::read(&used).unwrap();
    let mut h = rig();
    open(&mut h, &b);
    let ctx = h.ctx.clone();
    h.state_mut().fill(Slot::Layer(LayerId::Detail), used.clone(), &ctx);
    settle(&mut h);
    press(&mut h, "Save");
    assert!(d.join("rock_n2.png").is_file());
    assert_eq!(std::fs::read(&used).unwrap(), before);
}
```

In `tests/cube_tab.rs`, replace `a_strip_source_collides_in_its_own_folder` with:

```rust
#[test]
fn a_strip_saved_beside_itself_gets_a_1() {
    let d = temp("beside");
    let p = png(&d, "sky_e.png", 48, 8);
    let before = std::fs::read(&p).unwrap();
    let mut h = rig();
    open(&mut h, &p);
    assert_eq!(combo(&h), "Horizontal strip (6:1)");
    assert!(!shown(&h, "The output, sky_e.png, would replace the source, so choose another folder."));
    assert!(!save_disabled(&h));
    h.get_by_role_and_label(Role::Button, "Save").click_accesskit();
    h.run_steps(2);
    let bytes = std::fs::read(d.join("sky_e1.png")).expect("the output was written");
    assert_eq!(marker::read(&bytes).unwrap().kind, MapKind::Cube);
    assert_eq!(std::fs::read(&p).unwrap(), before);
}
```

- [ ] **Step 3: Run them and see them fail**

Run: `.\cargo.ps1 test --lib a_taken_name` then `.\cargo.ps1 test --test rnm_tab beside` and `.\cargo.ps1 test --test cube_tab beside`
Expected: the first does not compile (`free_name` is not defined). After Step 4's function exists, the tab tests fail on the disabled Save or the missing `rock_n1.png`.

- [ ] **Step 4: Implement**

- `export::free_name` as in Interfaces.
- Both tabs: `save_name()` joins the tab's own `out_name` with `free_name`. `save_rows` drops the warning and the `!self.collides()` term and gives the Save button `.on_hover_text(format!("Saves as {name}"))`. `save` plans `folder.join(save_name)`. `write` puts the same name in the three status sentences.
- Remove both `collides` functions, their tests, and the tabs' `collides` methods.

- [ ] **Step 5: Run the tests**

Run: `.\cargo.ps1 test --lib export` then `.\cargo.ps1 test --test rnm_tab` then `.\cargo.ps1 test --test cube_tab` then `.\cargo.ps1 test --test ui`
Expected: all pass, 0 warnings. `rg "would replace" src tests` finds only the two negative assertions.

---

### Task 2: Layers as a list

Two things about this task's tests, decided here so they are not read as findings:

- Step 1's test passes before any code changes. It records what the old code does, so the rewrite can be held to it.
- Every other test in this task is rewritten for types that do not exist yet. Their first failure is a compile error. That is the expected RED here: the lib, the tab and the tests share the types, so no test can run until all three agree.

**Files:**
- Modify: `src/rnm/mod.rs` (whole file)
- Modify: `src/rnm/blend.rs` (`apply` and the tests)
- Modify: `src/rnm/worker.rs` (`Job` and the tests)
- Modify: `src/rnm/files.rs` (`rnm_png`, `TERMS`, `layer_of`, `Found`, `siblings` and their tests)
- Modify: `src/rnm/tab.rs` (state, logic, column)
- Modify: `src/tab.rs:162-199` (add `slider_row_named`)
- Test: `tests/rnm_tab.rs`, `tests/rnm_engine.rs`, `tests/ui.rs:129-141`

**Interfaces:**

`src/rnm/mod.rs` (removes `LayerId`, `Settings`, `Inputs`):

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind { Bent, Bump, Detail }

impl Kind {
    pub const ALL: [Kind; 3];
    pub fn name(self) -> &'static str;          // "Bent", "Bump", "Detail"
    pub fn default_strength(self) -> f32;       // 0.35, 0.4, 0.6
}

/// One layer's settings. `use_ao` and `ao_power` are read for a Bent layer only.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Layer { pub kind: Kind, pub strength: f32, pub flip: bool, pub use_ao: bool, pub ao_power: f32 }

impl Layer {
    pub fn new(kind: Kind) -> Layer;            // default strength, flip false, use_ao true, ao_power 1.5
}

/// One layer of a blend: its settings and images. `ao` carries the channel it is read from.
#[derive(Clone)]
pub struct Input {
    pub set: Layer,
    pub img: Arc<Rgba32F>,
    pub mask: Option<Arc<Rgba32F>>,
    pub ao: Option<(Arc<Rgba32F>, usize)>,
}
```

`src/rnm/blend.rs`: `pub fn apply(work: &mut Rgba32F, layers: &[Input])` blends in list order. The three branches keep their arithmetic; a Bent layer uses its AO only when `set.use_ao` and `ao` is `Some`.

`src/rnm/worker.rs`: `pub struct Job { pub id: u64, pub base: Arc<Rgba32F>, pub layers: Vec<Input> }`.

`src/rnm/files.rs`:

```rust
/// A layer file found beside a base, with the mask found for it.
#[derive(Debug, PartialEq)]
pub struct FoundLayer { pub kind: Kind, pub strength: f32, pub file: PathBuf, pub mask: Option<PathBuf> }

/// What `siblings` found: the layers in the order bent, bump, bump2, detail, detail2, and the `_ao` file.
#[derive(Default, Debug, PartialEq)]
pub struct Found { pub layers: Vec<FoundLayer>, pub ao: Option<PathBuf> }

/// As before, but the layer is its place in that order, 0 to 4.
pub fn layer_of(stem: &str) -> Option<(String, usize, bool, Option<Suffix>)>;
pub fn siblings(base: &Path) -> Found;

/// One blended layer as the marker records it.
pub struct Recorded<'a> { pub set: Layer, pub file: &'a Loaded, pub mask: Option<&'a Loaded>, pub ao: Option<&'a Loaded> }
pub fn rnm_png(width: u32, height: u32, px: &[[f32; 4]], base: &Source, layers: &[Recorded]) -> Vec<u8>;
```

The five places and their kinds and strengths live in one private table: `[(Bent, 0.35), (Bump, 0.4), (Bump, 0.4), (Detail, 0.6), (Detail, 0.3)]`. The marker's `settings` is `{ "layers": [ … ] }`; each entry has `kind`, `strength`, `flip`, `file: {path, sha256}`, `mask` when there is one, and for a Bent layer `use_ao`, `ao_power`, and `ao` only when `use_ao` is true and an AO file is given.

`src/tab.rs`: `pub(crate) fn slider_row_named(ui: &mut egui::Ui, r: &Row, v: &mut f32, named: &str) -> Moved`. The visible label is `r.name`; the help button, the slider's accessible name and the reset button use `named`. `slider_row(ui, r, v)` calls it with `r.name`.

`src/rnm/tab.rs`:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Part { File, Mask, Ao }

/// One file place: a layer, by its `uid`, and which of its files.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Slot { pub layer: u64, pub part: Part }

pub struct LayerState {
    /// Never reused, so a file that finishes loading finds its layer after a reorder.
    pub uid: u64,
    pub set: Layer,
    /// What the strength reset goes back to.
    pub default: f32,
    pub file: Option<Loaded>,
    pub mask: Option<Loaded>,
    pub ao: Option<Loaded>,
}

// RnmTab's field `pub layers: Vec<LayerState>` replaces `slots` and `settings`.
impl RnmTab {
    pub fn add_layer(&mut self, kind: Kind, ctx: &egui::Context) -> u64;     // at the end; a Bent layer loads the base's `_ao` file
    pub fn remove_layer(&mut self, uid: u64);                                // also drops its files still loading
    pub fn move_layer(&mut self, from: usize, to: usize);                    // the layer at `from` ends at `to`; out of range does nothing
    pub fn fill(&mut self, slot: Slot, path: PathBuf, ctx: &egui::Context);
    pub fn clear(&mut self, slot: Slot);                                     // Mask and Ao
}
```

`sent` becomes `Option<Vec<(u64, Layer)>>`, so a reorder is a change even when two layers have equal settings. A file arriving or cleared calls `submit()` as today; list and setting changes are caught by the `sent` comparison at the end of `draw_column`. A file that arrives for a `uid` no longer in the list is dropped. `submit` sends an `Input` for each layer that has a file.

- [ ] **Step 1: Write the test that pins today's result (it passes now)**

In `tests/rnm_tab.rs`. It uses only `open` and `outputs`, which this task does not change.

```rust
/// What a folder with every sibling blends to. The numbers come from the five fixed slots this test outlived, so
/// they are never recorded again.
#[test]
fn all_five_siblings_blend_as_before() {
    let d = temp("golden");
    let b = base(&d, "rock_n.png");
    let wave = |a: f32, c: f32| move |x: u32, y: u32| [0.5 + 0.3 * (x as f32 * a).sin(), 0.5 + 0.25 * (y as f32 * c).cos(), 0.9, 1.0];
    let grey = |a: f32| {
        move |x: u32, y: u32| {
            let v = 0.5 + 0.4 * (x as f32 * a).sin() * (y as f32 * 0.3).cos();
            [v, v, v, 1.0]
        }
    };
    png(&d, "rock_bent.png", wave(0.15, 0.25));
    png(&d, "rock_bent_mask.png", |x, _| [x as f32 / 15.0, 0.0, 0.0, 1.0]);
    png(&d, "rock_bump.png", grey(0.4));
    png(&d, "rock_bump_mask.png", |_, y| [y as f32 / 15.0, 1.0, 0.2, 1.0]);
    png(&d, "rock_bump2.png", grey(0.9));
    png(&d, "rock_detail.png", wave(0.5, 0.7));
    png(&d, "rock_detail_mask.png", |x, y| [((x + y) % 4) as f32 / 3.0, 0.0, 0.0, 1.0]);
    png(&d, "rock_detail2.png", wave(0.9, 0.4));
    png(&d, "rock_ao.png", |x, y| [((x as f32 - 8.0).hypot(y as f32 - 8.0) / 11.0).min(1.0), 0.0, 1.0, 1.0]);
    let mut h = rig();
    open(&mut h, &b);
    let out = h.state().outputs.clone().expect("an output");
    for (i, want) in GOLDEN {
        for c in 0..3 {
            assert!((out.px[i][c] - want[c]).abs() < 1e-6, "texel {i}: {:?} vs {want:?}", out.px[i]);
        }
    }
    // The layers did something: the base alone gives other pixels.
    let alone = temp("golden-alone");
    let mut plain = rig();
    open(&mut plain, &base(&alone, "rock_n.png"));
    let p = plain.state().outputs.clone().expect("an output");
    assert!(out.px.iter().zip(&p.px).any(|(a, b)| (a[0] - b[0]).abs() > 1e-3));
}
```

- [ ] **Step 2: Record `GOLDEN` from the present code**

Add `const GOLDEN: [(usize, [f32; 3]); 6]` for the texels 0, 37, 100, 141, 200 and 255. To get the numbers, start with an empty `GOLDEN` and end the test once with `panic!("{:?}", [0, 37, 100, 141, 200, 255].map(|i| out.px[i]))`, run it, read the six texels from `last.log` beside `cargo.ps1` (the helper cannot pass `--nocapture`), paste the first three channels of each, and remove the panic.

Run: `.\cargo.ps1 test --test rnm_tab all_five`
Expected: PASS, on the code as it is before Step 4. Write the six rows into the ledger too.

- [ ] **Step 3: Rewrite the tests for the list**

Unit tests in `src/rnm/blend.rs`. The helpers become `one(kind, img) -> Vec<Input>` (settings from `Layer::new`), `masked(v, mask)` (sets the last layer's mask) and `run(layers: &[Input])`. Each existing test keeps its assertions; a strength, flip or AO change goes on `Input.set`, and an AO image on `Input.ao`. `apply_follows_the_order` builds its two Detail layers in list order ab and ba. `defaults_match_the_spec` becomes:

```rust
#[test]
fn a_new_layer_has_the_spec_defaults() {
    assert_eq!(Kind::ALL.map(|k| Layer::new(k).strength), [0.35, 0.4, 0.6]);
    assert!(Kind::ALL.iter().all(|&k| {
        let l = Layer::new(k);
        l.kind == k && !l.flip && l.use_ao && l.ao_power == 1.5
    }));
}
```

Two new blend tests:

```rust
#[test]
fn each_bent_layer_reads_its_own_ao() {
    let src = img(16, 16, textured);
    let bent = |ao: f32| Input { set: Layer::new(Kind::Bent), img: src.clone(), mask: None, ao: Some((solid([ao, 0.0, 0.0, 1.0]), 0)) };
    let open = run(&[bent(0.0)]);
    // The first layer's AO is white, so its weight is 0 and only the second bends the base.
    assert!(max_diff(&run(&[bent(1.0), bent(0.0)]), &open) < 1e-6);
    assert!(max_diff(&open, &run(&[])) > 1e-3);
}

#[test]
fn seven_layers_blend_in_list_order() {
    let layers: Vec<Input> = (0..7u32)
        .map(|k| Input {
            set: Layer { strength: 0.2 + 0.1 * k as f32, ..Layer::new(Kind::Detail) },
            img: img(16, 16, move |x, y| textured((x + k) % 16, (y + 2 * k) % 16)),
            mask: None,
            ao: None,
        })
        .collect();
    let mut want = base(16);
    for (j, px) in want.px.iter_mut().enumerate() {
        let mut n = [px[0], px[1], px[2]];
        for l in &layers {
            n = rnm(n, decode(l.img.px[j], l.set.strength, false));
        }
        put(px, n);
    }
    assert!(max_diff(&run(&layers), &want) < 1e-6);
}
```

Unit tests in `src/rnm/files.rs`. `layer_terms_match_longest_first_and_need_a_stem` expects the places 2, 2, 3, 0 and 4 where it expected `Bump2`, `Bump2`, `Detail`, `Bent` and `Detail2`. The sibling and marker tests become:

```rust
#[test]
fn siblings_list_the_layers_in_the_old_order() {
    // The same ten files as before.
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
    // `src` and `loaded` as in the test this replaces.
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
```

Unit tests in `src/rnm/worker.rs`: `detail()` returns `Vec<Input>` with one Detail layer and `job(id, layers)` takes it. Assertions stay.

`tests/rnm_engine.rs`: `ours` is blended from a `Vec<Input>` in Tessalith's order for the test, Detail (0.8, flipped, with `detail_mask`), Bent (0.55, `use_ao`, `ao_power` 2.5, with `bent_mask` and the AO on channel 0), Bump from `bump2.png` (0.25), Detail from `detail2.png` (0.45), Bump from `bump.png` (0.7, with `bump_mask`). The Tessalith side and both assertions stay as they are.

`tests/ui.rs`, `rnm_filled`: after setting the base, add a Bent layer with a mask and an AO map and a Bump layer, all with the long names, so the tallest block is in the width test:

```rust
let ctx = h.ctx.clone();
let t = &mut h.state_mut().rnm;
// (folder and base as before)
t.add_layer(Kind::Bent, &ctx);
let l = t.layers.last_mut().unwrap();
l.file = Some(loaded("a_very_long_texture_set_name_for_a_rock_wall_bent.png"));
l.mask = Some(loaded("a_very_long_texture_set_name_for_a_rock_wall_bent_mask.png"));
l.ao = Some(loaded("a_very_long_texture_set_name_for_a_rock_wall_ao.png"));
t.add_layer(Kind::Bump, &ctx);
t.layers.last_mut().unwrap().file = Some(loaded("a_very_long_texture_set_name_for_a_rock_wall_bump.png"));
```

`tests/rnm_tab.rs`. The imports become `texalith::rnm::tab::{Part, RnmTab, Slot}` and `texalith::rnm::{worker, Kind}`; `tests/ui.rs` imports `Kind` in place of `Slot` and `LayerId`. New helpers:

```rust
fn kinds(h: &Harness<'_, RnmTab>) -> Vec<Kind> {
    h.state().layers.iter().map(|l| l.set.kind).collect()
}

/// Adds a layer of `kind` with `file` and waits for the blend.
fn add(h: &mut Harness<'_, RnmTab>, kind: Kind, file: &Path) -> u64 {
    let ctx = h.ctx.clone();
    let uid = h.state_mut().add_layer(kind, &ctx);
    h.state_mut().fill(Slot { layer: uid, part: Part::File }, file.to_path_buf(), &ctx);
    settle(h);
    uid
}

/// Saves into `out` and returns the marker's layer list.
fn recorded(h: &mut Harness<'_, RnmTab>, out: &Path) -> serde_json::Value {
    h.state_mut().folder = Some(out.to_path_buf());
    settle(h);
    press(h, "Save");
    marker::read(&std::fs::read(out.join("rock_n.png")).unwrap()).unwrap().settings["layers"].clone()
}
```

Tests that keep their body: `the_last_opened_base_wins`, `an_msn_base_is_refused_before_it_is_read`, `save_writes_one_marked_png_with_the_base_alpha`, `a_foreign_file_opens_the_shared_dialog`, `the_alpha_line_is_always_shown_with_a_base`, `keeping_the_foreign_file_says_nothing_was_saved`, `a_dead_blend_thread_is_reported`, `an_older_output_of_the_same_base_is_shown_while_newer_jobs_run`, `all_five_siblings_blend_as_before`, and Task 1's four (in `an_output_name_used_by_a_layer_moves_on` the layer is added with `add(&mut h, Kind::Detail, &used)`). `an_empty_tab_shows_only_the_open_row` also asserts there is no "Add layer" button. `a_refused_base_opens_nothing` ends with `assert_eq!(kinds(&h), [Kind::Bump])`. `the_mask_help_is_a_button` looks for `hint_name("Layer 1 mask", mask)`.

Tests that change or are new:

```rust
#[test]
fn opening_a_base_adds_a_layer_per_sibling() {
    let d = temp("siblings");
    let b = base(&d, "rock_n.png");
    for f in ["rock_bump.png", "rock_detail.png", "rock_detail_mask.png"] {
        layer(&d, f);
    }
    let mut h = rig();
    open(&mut h, &b);
    assert_eq!(kinds(&h), [Kind::Bump, Kind::Detail]);
    for f in ["rock_n.png", "rock_bump.png", "rock_detail.png", "rock_detail_mask.png", "Layer 1: Bump", "Layer 2: Detail"] {
        assert!(shown(&h, f), "{f}");
    }
    h.get_by_role_and_label(Role::Button, "Reset Layer 1 depth to 0.8 texels");
    h.get_by_role_and_label(Role::Button, "Reset Layer 2 strength to 0.6");
    assert!(!shown(&h, "None") && !shown(&h, "Advanced") && !shown(&h, "Order"));
}

#[test]
fn all_five_siblings_open_as_five_layers() {
    let d = temp("five");
    let b = base(&d, "rock_n.png");
    for f in ["rock_bent.png", "rock_bent_mask.png", "rock_bump.png", "rock_bump2.png", "rock_detail.png", "rock_detail2.png", "rock_ao.png"] {
        layer(&d, f);
    }
    let mut h = rig();
    open(&mut h, &b);
    assert_eq!(kinds(&h), [Kind::Bent, Kind::Bump, Kind::Bump, Kind::Detail, Kind::Detail]);
    let t = h.state();
    assert_eq!(t.layers.iter().map(|l| l.set.strength).collect::<Vec<_>>(), [0.35, 0.4, 0.4, 0.6, 0.3]);
    assert!(t.layers[0].mask.is_some() && t.layers[0].ao.is_some());
    assert!(t.layers[1..].iter().all(|l| l.mask.is_none() && l.ao.is_none()));
    h.get_by_role_and_label(Role::Button, "Reset Layer 5 strength to 0.3");
}

#[test]
fn every_setting_sits_in_its_layer() {
    let d = temp("in-layer");
    let b = base(&d, "rock_n.png");
    layer(&d, "rock_bent.png");
    layer(&d, "rock_ao.png");
    let mut h = rig();
    open(&mut h, &b);
    assert!(shown(&h, "Strength") && shown(&h, "Focus on crevices"));
    h.get_by_role_and_label(Role::Slider, "Layer 1 strength");
    h.get_by_role_and_label(Role::Slider, "Layer 1 focus on crevices");
    h.get_by_role_and_label(Role::CheckBox, "Flip layer 1");
    h.get_by_role_and_label(Role::Button, "Choose AO map for layer 1");
    assert!(h.query_by_role_and_label(Role::Button, "Clear file for layer 1").is_none());
    h.get_by_role_and_label(Role::CheckBox, "Bend layer 1 mostly in shadows").click_accesskit();
    h.run_steps(2);
    assert!(!h.state().layers[0].set.use_ao);
    assert!(h.query_by_role_and_label(Role::Slider, "Layer 1 focus on crevices").is_none());
    assert!(h.query_by_role_and_label(Role::Button, "Choose AO map for layer 1").is_none());
}

#[test]
fn no_layers_shows_the_empty_line_and_a_bent_layer_takes_the_sibling_ao() {
    let d = temp("sibling-ao");
    let b = base(&d, "rock_n.png");
    layer(&d, "rock_ao.png");
    let mut h = rig();
    open(&mut h, &b);
    assert!(h.state().layers.is_empty());
    assert!(shown(&h, "No layers yet. Add one to blend it into the base."));
    let ctx = h.ctx.clone();
    h.state_mut().add_layer(Kind::Bent, &ctx);
    settle(&mut h);
    assert_eq!(h.state().layers[0].ao.as_ref().map(|l| l.source.path.clone()), Some(d.join("rock_ao.png")));
    assert!(!shown(&h, "No layers yet. Add one to blend it into the base."));
}

#[test]
fn the_add_menu_lists_three_kinds() {
    let d = temp("add-menu");
    let mut h = rig();
    open(&mut h, &base(&d, "rock_n.png"));
    press(&mut h, "Add layer");
    h.run_steps(3);
    // Not pressed: an item opens the file picker.
    for k in ["Bent", "Bump", "Detail"] {
        h.get_by_role_and_label(Role::Button, k);
    }
}

#[test]
fn a_layer_that_fails_to_load_is_reported_and_left_empty() {
    let d = temp("bad-layer");
    let b = base(&d, "rock_n.png");
    layer(&d, "rock_bump.png");
    let bad = d.join("broken.png");
    std::fs::write(&bad, "not an image").unwrap();
    let mut h = rig();
    open(&mut h, &b);
    let ctx = h.ctx.clone();
    let uid = h.state_mut().add_layer(Kind::Detail, &ctx);
    settle(&mut h);
    let sent = h.state().jobs_sent;
    h.state_mut().fill(Slot { layer: uid, part: Part::File }, bad, &ctx);
    settle(&mut h);
    match &h.state().status {
        Some(Err(e)) => assert!(e.starts_with("Could not read broken.png"), "{e}"),
        other => panic!("{other:?}"),
    }
    let t = h.state();
    assert!(t.layers[0].file.is_some() && t.layers[1].file.is_none());
    assert_eq!(t.jobs_sent, sent);
    assert!(shown(&h, "None"));
}

#[test]
fn the_arrows_move_a_layer_and_send_a_job() {
    let d = temp("move");
    let b = base(&d, "rock_n.png");
    layer(&d, "rock_bump.png");
    layer(&d, "rock_detail.png");
    let mut h = rig();
    open(&mut h, &b);
    let sent = h.state().jobs_sent;
    assert!(h.get_by_role_and_label(Role::Button, "Move layer 1 up").accesskit_node().is_disabled());
    assert!(h.get_by_role_and_label(Role::Button, "Move layer 2 down").accesskit_node().is_disabled());
    press(&mut h, "Move layer 2 up");
    assert_eq!(kinds(&h), [Kind::Detail, Kind::Bump]);
    assert_eq!(h.state().jobs_sent, sent + 1);
    assert!(shown(&h, "Layer 1: Detail"));
}

#[test]
fn swapping_two_equal_layers_sends_a_job() {
    let d = temp("swap-equal");
    let b = base(&d, "rock_n.png");
    let one = layer(&d, "one.png");
    let two = png(&d, "two.png", |x, y| [0.7 - 0.02 * y as f32, 0.3 + 0.03 * x as f32, 0.9, 1.0]);
    let mut h = rig();
    open(&mut h, &b);
    add(&mut h, Kind::Detail, &one);
    add(&mut h, Kind::Detail, &two);
    let (sent, before) = (h.state().jobs_sent, h.state().outputs.clone().unwrap());
    press(&mut h, "Move layer 2 up");
    settle(&mut h);
    assert_eq!(h.state().jobs_sent, sent + 1);
    assert_ne!(h.state().outputs.as_ref().unwrap().px, before.px);
}

#[test]
fn removing_a_layer_drops_its_files_and_sends_a_job() {
    let d = temp("remove");
    let b = base(&d, "rock_n.png");
    for f in ["rock_bump.png", "rock_detail.png", "rock_detail_mask.png"] {
        layer(&d, f);
    }
    let mut h = rig();
    open(&mut h, &b);
    let sent = h.state().jobs_sent;
    press(&mut h, "Remove layer 2");
    assert_eq!(kinds(&h), [Kind::Bump]);
    assert_eq!(h.state().jobs_sent, sent + 1);
    assert!(!shown(&h, "rock_detail_mask.png"));
}

#[test]
fn a_file_for_a_removed_layer_is_dropped() {
    let d = temp("removed-load");
    let b = base(&d, "rock_n.png");
    let f = layer(&d, "late.png");
    let mut h = rig();
    open(&mut h, &b);
    let ctx = h.ctx.clone();
    let uid = h.state_mut().add_layer(Kind::Detail, &ctx);
    h.state_mut().fill(Slot { layer: uid, part: Part::File }, f, &ctx);
    h.state_mut().remove_layer(uid);
    settle(&mut h);
    assert!(h.state().layers.is_empty());
    assert_eq!(h.state().status, None);
    assert!(!shown(&h, "late.png"));
}

#[test]
fn a_mask_without_its_layer_is_not_loaded() {
    let d = temp("lone-mask");
    let b = base(&d, "rock_n.png");
    layer(&d, "rock_detail.png");
    layer(&d, "rock_bump_mask.png");
    let mut h = rig();
    open(&mut h, &b);
    assert_eq!(kinds(&h), [Kind::Detail]);
    assert!(h.state().layers[0].mask.is_none());
    assert!(!shown(&h, "rock_bump_mask.png"));
}

#[test]
fn the_marker_lists_the_filled_layers_in_blend_order() {
    let d = temp("marker");
    let b = base(&d, "rock_n.png");
    for f in ["rock_bump.png", "rock_detail.png", "rock_detail_mask.png"] {
        layer(&d, f);
    }
    let mut h = rig();
    open(&mut h, &b);
    let ctx = h.ctx.clone();
    h.state_mut().add_layer(Kind::Detail, &ctx); // no file: not recorded
    press(&mut h, "Move layer 2 up");
    let l = recorded(&mut h, &temp("marker-out"));
    assert_eq!(l.as_array().unwrap().len(), 2);
    assert_eq!((&l[0]["kind"], &l[1]["kind"]), (&serde_json::json!("detail"), &serde_json::json!("bump")));
    assert!(l[0]["mask"]["path"].as_str().unwrap().ends_with("rock_detail_mask.png"));
}

#[test]
fn ao_is_recorded_only_while_bend_in_shadows_is_on() {
    let d = temp("ao-marker");
    let b = base(&d, "rock_n.png");
    layer(&d, "rock_bent.png");
    layer(&d, "rock_ao.png");
    let mut h = rig();
    open(&mut h, &b);
    let with = recorded(&mut h, &temp("ao-marker-1"));
    assert!(with[0]["ao"]["path"].as_str().unwrap().ends_with("rock_ao.png"));
    h.state_mut().layers[0].set.use_ao = false;
    let without = recorded(&mut h, &temp("ao-marker-2"));
    assert!(without[0].get("ao").is_none());
}

#[test]
fn twelve_layers_keep_save_in_view() {
    let d = temp("twelve");
    let b = base(&d, "rock_n.png");
    let f = layer(&d, "one.png");
    let mut h = rig();
    open(&mut h, &b);
    h.state_mut().folder = Some(temp("twelve-out"));
    for i in 0..12 {
        add(&mut h, if i % 2 == 0 { Kind::Detail } else { Kind::Bump }, &f);
    }
    assert_eq!(h.state().layers.len(), 12);
    assert_eq!(h.state().outputs.as_ref().map(|o| o.id), Some(h.state().jobs_sent));
    let save = h.get_by_role_and_label(Role::Button, "Save");
    assert!(!save.accesskit_node().is_disabled());
    assert!(save.rect().max.y <= 900.0 && save.rect().min.y >= 0.0, "{:?}", save.rect());
    assert!(h.get_by_role_and_label(Role::Button, "Move layer 12 up").rect().max.x <= COLUMN_WIDTH + 1.0);
}
```

The old `moving_a_layer_changes_the_order_and_sends_a_job`, `clearing_a_layer_clears_its_mask`, `a_mask_without_its_layer_is_not_recorded` and `ao_is_recorded_only_with_a_bent_layer` are replaced by the tests above.

- [ ] **Step 4: See the compile failure**

Run: `.\cargo.ps1 test --test rnm_tab`
Expected: does not compile; the errors name `Kind`, `Part`, `Input`, `Recorded`, `add_layer` and `layers`.

- [ ] **Step 5: Implement the list in the lib**

`src/rnm/mod.rs`, `blend.rs`, `worker.rs` and `files.rs` as in Interfaces. In `siblings` the slot bookkeeping (best candidate per place, prefix and extension ranking) stays as it is; only the result is repacked.

- [ ] **Step 6: Implement the tab and the column**

State and logic as in Interfaces. `accept_base` clears the layers, keeps `found.ao` for later Bent layers, and adds a layer per `FoundLayer` with its strength as both the value and `default`; each Bent layer among them gets `found.ao`. `save` builds a `Recorded` per layer that has a file and passes every file of every layer, with the base, to `free_name`.

The column, top to bottom inside the scroll area: heading, base rows, "Layers" with its help, one block per layer, "Add layer", the alpha line. A block is drawn inside `ui.push_id(uid, …)` so a slider keeps its state when its layer moves: a separator, a header row (`egui::Sides`, the title on the left, the three buttons on the right, as the Order rows are built today), the file row, and when a file is set the rows listed under Exact words. `slot_row` takes the names from its caller and a flag for the clear button. The "Advanced" header and `advanced_rows` are removed. Clicks are collected as `Act` values and applied after drawing, as today; `Act` gains `Add(Kind)`, `Remove(u64)` and `Move(usize, usize)`.

`src/tab.rs`: `slider_row_named` as in Interfaces.

- [ ] **Step 7: Run the tests**

Run: `.\cargo.ps1 test --lib rnm` then `.\cargo.ps1 test --test rnm_engine` then `.\cargo.ps1 test --test rnm_tab` then `.\cargo.ps1 test --test ui`
Expected: all pass, 0 warnings. `all_five_siblings_blend_as_before` passes with the numbers from Step 2 untouched. `rg "LayerId|Bump2|Detail2" src tests` finds only the Tessalith side of `tests/rnm_engine.rs` (`LayerId as E`, `E::Bump2`, `E::Detail2`).

---

### Task 3: Drag and drop

**Files:**
- Modify: `src/rnm/tab.rs` (the header row, the block loop, two pure functions and their tests)
- Test: `tests/rnm_tab.rs`

**Interfaces:**
- Consumes: `RnmTab::move_layer(from, to)` and the layer blocks from Task 2.
- Produces:

```rust
/// The insert place, 0 to `centres.len()`, for a pointer at height `y`: the number of blocks whose centre is above it.
pub fn drop_slot(centres: &[f32], y: f32) -> usize;
/// Where the layer at `from` ends up when it is dropped at insert place `slot`.
pub fn drop_index(from: usize, slot: usize) -> usize;
```

How it works:
- The header's "≡" and title sit inside `ui.dnd_drag_source(id, DragLayer(uid), …)`, with an id made from the layer's `uid`. `DragLayer` is a private struct holding the `uid`.
- While a `DragLayer` is in flight (`egui::DragAndDrop::payload`), each frame works out the insert place from `pointer.interact_pos()` and the centres of this frame's blocks, keeps it in the tab, and paints a 2-point line in the selection colour at that place. The place is cleared when the pointer is outside the rectangle that holds the blocks and the "Add layer" button. That rectangle is cut to the scroll area's visible part first, so with a long list a release over the Save area below is not a drop.
- On the frame a button is released (`pointer.any_released()`), the tab takes the payload and, if a place is kept, calls `move_layer(from, drop_index(from, slot))`. The place is kept from the frame before because kittest's `drop_at` sends the release and removes the pointer in one frame, and egui clears the payload at the end of that frame.
- Escape cancels: egui clears the payload, and the tab clears the place when no payload is in flight.

- [ ] **Step 1: Write the failing unit tests in `src/rnm/tab.rs`**

```rust
#[test]
fn the_drop_slot_counts_the_blocks_above_the_pointer() {
    let c = [100.0, 200.0, 300.0];
    assert_eq!([50.0, 150.0, 250.0, 999.0].map(|y| drop_slot(&c, y)), [0, 1, 2, 3]);
    assert_eq!(drop_slot(&[], 5.0), 0);
}

#[test]
fn a_drop_beside_itself_keeps_the_place() {
    assert_eq!((drop_index(1, 1), drop_index(1, 2)), (1, 1));
    assert_eq!((drop_index(0, 3), drop_index(2, 0)), (2, 0));
}
```

- [ ] **Step 2: Write the failing window tests in `tests/rnm_tab.rs`**

```rust
fn uids(h: &Harness<'_, RnmTab>) -> Vec<u64> {
    h.state().layers.iter().map(|l| l.uid).collect()
}

/// Presses on the drag source of layer `n`, moves to `to` in four steps and lets go there.
fn drag(h: &mut Harness<'_, RnmTab>, n: usize, to: egui::Pos2) {
    let from = h.get_by_label(&format!("Drag layer {n}")).rect().center();
    h.drag_at(from);
    h.run_steps(1);
    for t in 1..=4 {
        h.hover_at(from + (to - from) * (t as f32 / 4.0));
        h.run_steps(1);
    }
    h.drop_at(to);
    h.run_steps(2);
}

fn three(name: &str) -> (Harness<'static, RnmTab>, PathBuf) {
    let d = temp(name);
    let b = base(&d, "rock_n.png");
    for f in ["rock_bump.png", "rock_detail.png", "rock_detail2.png"] {
        layer(&d, f);
    }
    let mut h = rig();
    open(&mut h, &b);
    (h, d)
}

#[test]
fn dragging_a_layer_below_the_last_moves_it_there() {
    let (mut h, _d) = three("drag-last");
    let (before, sent) = (uids(&h), h.state().jobs_sent);
    let below = h.get_by_role_and_label(Role::Button, "Add layer").rect().center();
    drag(&mut h, 1, below);
    settle(&mut h);
    assert_eq!(uids(&h), [before[1], before[2], before[0]]);
    assert_eq!(h.state().jobs_sent, sent + 1);
}

#[test]
fn dragging_the_last_layer_to_the_top_moves_it_there() {
    let (mut h, _d) = three("drag-first");
    let before = uids(&h);
    // Layer 1's header is above the middle of its block, so this is insert place 0.
    let top = h.get_by_label("Drag layer 1").rect().center();
    drag(&mut h, 3, top);
    settle(&mut h);
    assert_eq!(uids(&h), [before[2], before[0], before[1]]);
}

#[test]
fn dropping_a_layer_on_itself_changes_nothing() {
    let (mut h, _d) = three("drag-self");
    let (before, sent) = (uids(&h), h.state().jobs_sent);
    let beside = h.get_by_label("Drag layer 2").rect().center() + egui::vec2(24.0, 0.0);
    drag(&mut h, 2, beside);
    settle(&mut h);
    assert_eq!(uids(&h), before);
    assert_eq!(h.state().jobs_sent, sent);
}

#[test]
fn a_drop_outside_the_layers_changes_nothing() {
    let (mut h, _d) = three("drag-out");
    let before = uids(&h);
    drag(&mut h, 1, egui::pos2(900.0, 450.0));
    assert_eq!(uids(&h), before);
}
```

- [ ] **Step 3: Run them and see them fail**

Run: `.\cargo.ps1 test --lib drop_` then `.\cargo.ps1 test --test rnm_tab drag`
Expected: the unit tests do not compile (`drop_slot` is not defined). Once the functions exist, the window tests fail because no node is named "Drag layer 1".

- [ ] **Step 4: Implement**

The two functions, the drag source in the header (named "Drag layer {n}" with `widget_info` after it is drawn, hover text "Drag to reorder"), the kept place, the line and the release handling, as under "How it works".

- [ ] **Step 5: Run the tests**

Run: `.\cargo.ps1 test --lib rnm` then `.\cargo.ps1 test --test rnm_tab`
Expected: all pass, 0 warnings.

If the three moving window tests cannot be made to drive a drag through kittest after a fair try (the handle is found, the events are sent, and `egui::DragAndDrop::payload` stays empty), record a ruling in the ledger, keep the two unit tests and `a_drop_outside_the_layers_changes_nothing`, and make the drag in Task 4 the proof. Do not change the design to suit the test.

---

### Task 4: The whole suite and the real window

No code is planned here. A fault found is fixed test-first in the task that owns the code.

**Files:**
- Create (git-ignored, for the check only): `F:\ai\matierial\target\mat_test\rock_detail.png`, `rock_detail2.png`, `rock_bump.png`

- [ ] **Step 1: Run the whole suite**

Check `Get-Process toolbag` first. Run: `.\cargo.ps1 test`
Expected: 0 failed, 0 warnings. Write the count into the ledger.

- [ ] **Step 2: Make the sibling files**

In `F:\ai\matierial\target\mat_test`: copy `rock_n.png` to `rock_detail.png` and `rock_detail2.png`, and copy `rock_ao.png` to `rock_bump.png`. Delete `rock_n1.png` if an earlier run left it.

- [ ] **Step 3: Look at the real window**

Start `F:\ai\matierial\target\x86_64-pc-windows-msvc\debug\texalith.exe` with `Start-Process`. On the RNM tab, open `rock_n.png`. At 1100, 1280 and 1396 points wide, check each of these and write what was seen into the ledger:

1. Three layers show, each with its title, drag handle, arrows and remove button, and nothing is cut at the column's edge.
2. Dragging layer 1 by its handle below layer 3 moves it there, an insert line shows while dragging, and the preview changes.
3. Dragging a slider inside a layer moves the slider, not the layer.
4. "Add layer" then "Bent" opens the file picker; choosing `rock_n.png` adds "Layer 4: Bent" with `rock_ao.png` as its AO map. Unticking "Bend mostly in shadows" hides the AO map row and the Focus slider.
5. "Remove layer 4" takes it away.
6. Save, with the folder left on the base's own, writes `rock_n1.png`, leaves `rock_n.png` as it was (compare the file's hash before and after), and the status line names `rock_n1.png`.
7. On the Cubemap tab, a strip named `…_e.png` saved into its own folder writes `…_e1.png`.

Expected: all seven hold at the three widths. Anything not looked at is written into the ledger as not seen.

- [ ] **Step 4: Close up**

Close Texalith. NUL-scan every changed file. Run both `git status` checks from Global Constraints for `F:\ai\dds`; both must be as they were.
