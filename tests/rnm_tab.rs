//! The RNM column with a real blend thread. Fixtures are small 16-bit PNGs in a temp folder.
use eframe::egui;
use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use std::path::{Path, PathBuf};
use std::time::Duration;
use texalith::marker::{self, MapKind};
use texalith::rnm::tab::{Part, RnmTab, Slot};
use texalith::rnm::{worker, Kind};
use texalith::tab::COLUMN_WIDTH;

const EMPTY: &str = "Open a base normal (_n or _cnr) to blend layers into it.";
const ALPHA: &str = "Alpha is kept as it is.";

fn rig<'a>() -> Harness<'a, RnmTab> {
    rig_dt(0.25)
}

/// `rig` with `dt` seconds between frames; kittest's own step is a quarter second, longer than a slider's rest.
fn rig_dt<'a>(dt: f32) -> Harness<'a, RnmTab> {
    let (jobs, outputs) = worker::spawn(|| {});
    Harness::builder().with_size(egui::vec2(1280.0, 900.0)).with_step_dt(dt).build_ui_state(
        |ui, tab: &mut RnmTab| {
            tab.drain();
            egui::Panel::left("column").resizable(false).exact_size(COLUMN_WIDTH).show(ui, |ui| tab.draw_column(ui));
        },
        RnmTab::new(jobs, outputs),
    )
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("texalith-rnm-tab-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A 16x16 16-bit RGBA PNG from `f` (0..1 per channel).
fn png(dir: &Path, name: &str, f: impl Fn(u32, u32) -> [f32; 4]) -> PathBuf {
    let p = dir.join(name);
    image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::from_fn(16, 16, |x, y| {
        image::Rgba(f(x, y).map(|v| (v * 65535.0).round() as u16))
    })
    .save(&p)
    .unwrap();
    p
}

fn base(dir: &Path, name: &str) -> PathBuf {
    png(dir, name, |x, y| [0.5 + 0.01 * x as f32, 0.5 - 0.01 * y as f32, 1.0, 0.3])
}

fn layer(dir: &Path, name: &str) -> PathBuf {
    png(dir, name, |x, y| [0.3 + 0.03 * x as f32, 0.7 - 0.02 * y as f32, 0.9, 1.0])
}

/// Runs frames until no file is loading and the newest job's output has arrived.
fn settle(h: &mut Harness<'_, RnmTab>) {
    for _ in 0..500 {
        h.run_steps(1);
        let t = h.state();
        let done = t.jobs_sent == 0 || t.outputs.as_ref().map(|o| o.id) == Some(t.jobs_sent);
        if !t.busy() && done {
            h.run_steps(2);
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the tab never settled");
}

fn open(h: &mut Harness<'_, RnmTab>, p: &Path) {
    let ctx = h.ctx.clone();
    h.state_mut().open_base(p.to_path_buf(), &ctx);
    settle(h);
}

fn press(h: &mut Harness<'_, RnmTab>, label: &str) {
    h.get_by_role_and_label(Role::Button, label).click_accesskit();
    h.run_steps(2);
}

fn shown(h: &Harness<'_, RnmTab>, text: &str) -> bool {
    h.query_by_label(text).is_some()
}

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

#[test]
fn an_empty_tab_shows_only_the_open_row() {
    let mut h = rig();
    h.run_steps(2);
    assert!(shown(&h, EMPTY));
    h.get_by_role_and_label(Role::Button, "Open base…");
    assert!(h.query_by_role_and_label(Role::Button, "Save").is_none());
    assert!(h.query_by_role_and_label(Role::Button, "Add layer").is_none());
    assert_eq!(h.query_all_by_role(Role::Slider).count(), 0);
}

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
    // A Bump layer's slider is a Strength too; its number is a height in texels, said once in the label.
    h.get_by_role_and_label(Role::Button, "Reset Layer 1 strength to 0.8");
    h.get_by_role_and_label(Role::Slider, "Layer 1 strength");
    assert!(shown(&h, "Strength (texels)") && !shown(&h, "Depth"));
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
fn the_mask_help_is_a_button() {
    let d = temp("mask-help");
    let b = base(&d, "rock_n.png");
    layer(&d, "rock_detail.png");
    let mut h = rig();
    open(&mut h, &b);
    let mask = "Limits the layer to the white parts of a mask image; add one when this detail should stay off some areas.";
    h.get_by_role_and_label(Role::Button, &texalith::tab::hint_name("Layer 1 mask", mask));
    assert!(!shown(&h, mask));
}

#[test]
fn a_refused_base_opens_nothing() {
    let d = temp("refused");
    let b = base(&d, "rock_n.png");
    layer(&d, "rock_bump.png");
    let msn = base(&d, "rock_msn.png");
    let mut h = rig();
    open(&mut h, &b);
    open(&mut h, &msn);
    assert!(shown(&h, "rock_msn.png is a model-space normal (_msn); RNM blends tangent-space normals only."));
    assert_eq!(h.state().base.as_ref().map(|(s, _)| s.path.clone()), Some(b));
    assert_eq!(kinds(&h), [Kind::Bump]);
}

#[test]
fn the_last_opened_base_wins() {
    let d = temp("last-wins");
    // Big enough that it finishes loading well after the small one.
    let slow = d.join("slow_n.png");
    image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::from_pixel(1024, 1024, image::Rgba([32768, 32768, 65535, 65535]))
        .save(&slow)
        .unwrap();
    let fast = base(&d, "fast_n.png");
    let mut h = rig();
    let ctx = h.ctx.clone();
    h.state_mut().open_base(slow, &ctx);
    h.state_mut().open_base(fast.clone(), &ctx);
    settle(&mut h);
    assert_eq!(h.state().base.as_ref().map(|(s, _)| s.path.clone()), Some(fast));
}

#[test]
fn an_msn_base_is_refused_before_it_is_read() {
    let d = temp("msn-early");
    // The file does not exist: a read would report "Could not read".
    let msn = d.join("rock_msn.png");
    let mut h = rig();
    let ctx = h.ctx.clone();
    h.state_mut().open_base(msn, &ctx);
    assert!(!h.state().busy(), "nothing is read");
    match &h.state().status {
        Some(Err(e)) => assert_eq!(e, "rock_msn.png is a model-space normal (_msn); RNM blends tangent-space normals only."),
        other => panic!("{other:?}"),
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
fn a_moving_strength_slider_sends_no_job_until_it_rests() {
    let d = temp("rest");
    let b = base(&d, "rock_n.png");
    layer(&d, "rock_bump.png");
    let mut h = rig_dt(1.0 / 60.0);
    open(&mut h, &b);
    let sent = h.state().jobs_sent;
    let c = h.get_by_role_and_label(Role::Slider, "Layer 1 strength").rect().center();
    h.hover_at(c);
    h.step();
    h.drag_at(c);
    h.step();
    for k in 1..=10 {
        h.hover_at(c + egui::vec2(3.0 * k as f32, 0.0));
        h.step();
    }
    assert_eq!(h.state().jobs_sent, sent, "no job while the slider moves");
    h.run_steps(20); // a third of a second at rest, the button still down
    assert_eq!(h.state().jobs_sent, sent + 1, "one job once the slider rests");
    h.drop_at(c + egui::vec2(30.0, 0.0));
    h.run_steps(2);
    assert_eq!(h.state().jobs_sent, sent + 1);
}

#[test]
fn a_result_for_older_settings_does_not_land_while_a_slider_is_held() {
    let d = temp("stale");
    let b = base(&d, "rock_n.png");
    layer(&d, "rock_bump.png");
    // A stand-in for the blend thread, so a job can still be running when the slider moves.
    let (job_tx, job_rx) = std::sync::mpsc::channel::<worker::Job>();
    let (out_tx, out_rx) = std::sync::mpsc::channel();
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 900.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, tab: &mut RnmTab| {
            tab.drain();
            egui::Panel::left("column").resizable(false).exact_size(COLUMN_WIDTH).show(ui, |ui| tab.draw_column(ui));
        },
        RnmTab::new(job_tx, out_rx),
    );
    let ctx = h.ctx.clone();
    h.state_mut().open_base(b, &ctx);
    for _ in 0..500 {
        h.run_steps(1);
        if !h.state().busy() && h.state().layers.iter().any(|l| l.file.is_some()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    h.run_steps(2);
    let running = h.state().jobs_sent;
    assert!(running > 0 && job_rx.try_iter().count() > 0, "a job is out and has no result yet");
    let out = |id| std::sync::Arc::new(worker::Output { id, size: (16, 16), px: vec![[0.5; 4]; 256], preview: None });
    let c = h.get_by_role_and_label(Role::Slider, "Layer 1 strength").rect().center();
    h.hover_at(c);
    h.step();
    h.drag_at(c);
    h.step();
    h.hover_at(c + egui::vec2(20.0, 0.0));
    h.step();
    out_tx.send(out(running)).unwrap();
    h.run_steps(2);
    assert!(h.state().outputs.is_none(), "showing it would stall the drag for settings already replaced");
    h.drop_at(c + egui::vec2(20.0, 0.0));
    h.run_steps(2);
    let next = h.state().jobs_sent;
    assert_eq!(next, running + 1, "a job for the value let go at");
    out_tx.send(out(next)).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().outputs.as_ref().map(|o| o.id), Some(next));
}

#[test]
#[ignore = "one real 8192 blend: several GB of memory and minutes; run alone with --ignored --nocapture"]
fn an_8192_base_blends_and_saves() {
    let d = temp("8192");
    let p = d.join("big_n.png");
    image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::from_fn(8192, 8192, |x, y| {
        image::Rgba([32768 + (x % 64) as u16 * 100, 32768 - (y % 64) as u16 * 100, 65535, 65535])
    })
    .save(&p)
    .unwrap();
    layer(&d, "big_detail.png");
    let mut h = rig();
    let start = std::time::Instant::now();
    let ctx = h.ctx.clone();
    h.state_mut().open_base(p, &ctx);
    // `settle` gives up after five seconds, which an 8192 load and blend outlast.
    loop {
        h.run_steps(1);
        let t = h.state();
        if let Some(Err(e)) = &t.status {
            panic!("{e}");
        }
        if !t.busy() && t.jobs_sent > 0 && t.outputs.as_ref().map(|o| o.id) == Some(t.jobs_sent) {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(900), "no blend after 15 minutes");
        std::thread::sleep(Duration::from_millis(50));
    }
    let blended = start.elapsed();
    let o = h.state().outputs.clone().unwrap();
    assert_eq!(o.size, (8192, 8192));
    assert_eq!(h.state().layers.len(), 1, "the detail layer beside the base");
    let n = &o.preview.as_ref().expect("packed maps")["n"];
    assert_eq!(u32::from_le_bytes(n[16..20].try_into().unwrap()), 4096, "the preview's map is halved");
    let out = temp("8192-out");
    h.state_mut().folder = Some(out.clone());
    h.run_steps(2);
    press(&mut h, "Save");
    let saved = std::fs::read(out.join("big_n.png")).expect("the saved blend");
    assert!(marker::is_ours(&saved));
    eprintln!("8192: open and blend {blended:.1?}, with save {:.1?}, file {} MB", start.elapsed(), saved.len() >> 20);
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
    // The row is the numbered file, on Keep; a plan that aimed at the base would have no such choice.
    let keep = h.get_by_role_and_label(Role::Button, "Keep rock_n1.png");
    assert_eq!(keep.accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
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
    add(&mut h, Kind::Detail, &used);
    press(&mut h, "Save");
    assert!(d.join("rock_n2.png").is_file());
    assert_eq!(std::fs::read(&used).unwrap(), before);
}

#[test]
fn save_writes_one_marked_png_with_the_base_alpha() {
    let d = temp("save");
    let b = base(&d, "rock_n.png");
    layer(&d, "rock_detail.png");
    let before = std::fs::read(&b).unwrap();
    let out = temp("save-out");
    let mut h = rig();
    open(&mut h, &b);
    h.state_mut().folder = Some(out.clone());
    settle(&mut h);
    press(&mut h, "Save");
    let bytes = std::fs::read(out.join("rock_n.png")).expect("the output was written");
    assert_eq!(marker::read(&bytes).unwrap().kind, MapKind::Rnm);
    let img = image::load_from_memory(&bytes).unwrap().to_rgba16();
    let a = (0.3f32 * 65535.0).round() as u16;
    assert!(img.pixels().all(|p| p[3] == a));
    assert_eq!(std::fs::read(&b).unwrap(), before);
    match &h.state().status {
        Some(Ok(m)) => assert!(m.starts_with("Saved rock_n.png to "), "{m}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_foreign_file_opens_the_shared_dialog() {
    let d = temp("foreign");
    let b = base(&d, "rock_n.png");
    layer(&d, "rock_detail.png");
    let out = temp("foreign-out");
    let foreign = layer(&out, "rock_n.png");
    let before = std::fs::read(&foreign).unwrap();
    let mut h = rig();
    open(&mut h, &b);
    h.state_mut().folder = Some(out.clone());
    settle(&mut h);
    press(&mut h, "Save");
    assert!(shown(&h, "Some files will be kept"));
    press(&mut h, "Cancel");
    assert!(h.state().review.is_none());
    assert_eq!(std::fs::read(&foreign).unwrap(), before);
}

#[test]
fn the_alpha_line_is_always_shown_with_a_base() {
    let d = temp("alpha");
    let b = base(&d, "rock_n.png");
    let mut h = rig();
    open(&mut h, &b);
    assert!(shown(&h, ALPHA));
}

#[test]
fn keeping_the_foreign_file_says_nothing_was_saved() {
    let d = temp("keep");
    let b = base(&d, "rock_n.png");
    layer(&d, "rock_detail.png");
    let out = temp("keep-out");
    let foreign = layer(&out, "rock_n.png");
    let before = std::fs::read(&foreign).unwrap();
    let mut h = rig();
    open(&mut h, &b);
    h.state_mut().folder = Some(out.clone());
    settle(&mut h);
    press(&mut h, "Save");
    // The dialog's Save, not the column's: the one further right.
    h.query_all_by_role_and_label(Role::Button, "Save")
        .max_by(|a, b| a.rect().min.x.total_cmp(&b.rect().min.x))
        .unwrap()
        .click_accesskit();
    h.run_steps(2);
    assert_eq!(std::fs::read(&foreign).unwrap(), before);
    match &h.state().status {
        Some(Ok(m)) => assert!(m.starts_with("Nothing saved: rock_n.png was kept"), "{m}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_dead_blend_thread_is_reported() {
    let (jobs, _job_rx) = std::sync::mpsc::channel();
    let (out_tx, out_rx) = std::sync::mpsc::channel();
    let mut t = RnmTab::new(jobs, out_rx);
    t.jobs_sent = 1;
    drop(out_tx);
    assert!(!t.drain());
    assert_eq!(t.status, Some(Err("Blending stopped. Restart Texalith.".into())));
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
    assert!(h.get_by_role_and_label(Role::Button, "Remove layer 12").rect().max.x <= COLUMN_WIDTH + 1.0);
}

#[test]
fn an_older_output_of_the_same_base_is_shown_while_newer_jobs_run() {
    use std::sync::Arc;
    let (jobs, _job_rx) = std::sync::mpsc::channel();
    let (out_tx, out_rx) = std::sync::mpsc::channel();
    let mut t = RnmTab::new(jobs, out_rx);
    let d = temp("older");
    let b = base(&d, "rock_n.png");
    let img = Arc::new(engine::load::load_image(&b).unwrap().0);
    let src = texalith::from_normal::files::source(&b).unwrap();
    t.base = Some((src, img));
    t.jobs_sent = 3;
    let out = |id| Arc::new(worker::Output { id, size: (16, 16), px: vec![[0.5; 4]; 256], preview: None });
    out_tx.send(out(2)).unwrap();
    assert!(t.drain(), "a newer output than the one shown is shown, even while job 3 runs");
    assert_eq!(t.outputs.as_ref().map(|o| o.id), Some(2));
    out_tx.send(out(1)).unwrap();
    assert!(!t.drain(), "an older output than the one shown is dropped");
    assert_eq!(t.outputs.as_ref().map(|o| o.id), Some(2));
}

const GOLDEN: [(usize, [f32; 3]); 6] = [
    (0, [0.43860745, 0.5787963, 0.9899205]),
    (37, [0.60535866, 0.5480104, 0.9864099]),
    (100, [0.5914631, 0.3352903, 0.96314704]),
    (141, [0.65570927, 0.34354556, 0.9486387]),
    (200, [0.69190013, 0.36633545, 0.9419368]),
    (255, [0.836299, 0.3860637, 0.8520249]),
];

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

#[test]
fn escape_cancels_a_drag() {
    let (mut h, _d) = three("drag-escape");
    let before = uids(&h);
    let from = h.get_by_label("Drag layer 1").rect().center();
    let below = h.get_by_role_and_label(Role::Button, "Add layer").rect().center();
    h.drag_at(from);
    h.run_steps(1);
    h.hover_at(below);
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    h.drop_at(below);
    h.run_steps(2);
    assert_eq!(uids(&h), before);
}

#[test]
fn the_font_has_every_symbol_the_column_draws() {
    let mut h = rig();
    h.run_steps(2);
    let font = egui::FontId::proportional(14.0);
    for g in texalith::rnm::tab::GLYPHS {
        assert!(h.ctx.fonts_mut(|f| f.has_glyphs(&font, g)), "{g} would draw as an empty box");
    }
}
