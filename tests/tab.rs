//! The From normal column, with a stand-in for the generation thread. No real solve runs here.
use eframe::egui;
use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use texalith::from_normal::input::Kind;
use texalith::from_normal::maps::{FlatLevel, Gray, ParallaxReport};
use texalith::marker::MapKind;
use texalith::from_normal::master::MasterReport;
use texalith::from_normal::open::{open, Opened};
use texalith::from_normal::settings::Settings;
use texalith::from_normal::worker::{Job, Outputs, Reply};
use texalith::tab::{Tab, COLUMN_WIDTH};

struct Rig {
    jobs: Receiver<Job>,
    replies: Sender<Reply>,
}

fn rig<'a>(width: f32) -> (Harness<'a, Tab>, Rig) {
    rig_dt(width, 0.25)
}

/// `rig` with `dt` seconds between frames; kittest's own step is a quarter second, longer than a slider's rest.
fn rig_dt<'a>(width: f32, dt: f32) -> (Harness<'a, Tab>, Rig) {
    let (job_tx, job_rx) = channel();
    let (reply_tx, reply_rx) = channel();
    let h = Harness::builder().with_size(egui::vec2(width, 900.0)).with_step_dt(dt).build_ui_state(
        |ui, tab: &mut Tab| {
            tab.drain();
            egui::Panel::left("column")
                .resizable(false)
                .exact_size(COLUMN_WIDTH)
                .show(ui, |ui| tab.draw_column(ui));
        },
        Tab::new(job_tx, reply_rx),
    );
    (h, Rig { jobs: job_rx, replies: reply_tx })
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("texalith-tab-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn normal_source(name: &str) -> Opened {
    let p = temp(name).join("rock_n.png");
    image::RgbImage::from_fn(64, 64, |x, _| image::Rgb([(120 + x) as u8, 128, 230])).save(&p).unwrap();
    open(&p).unwrap()
}

fn height_source(name: &str) -> Opened {
    let p = temp(name).join("rock_disp.png");
    image::GrayImage::from_fn(64, 64, |x, _| image::Luma([(x * 4) as u8])).save(&p).unwrap();
    open(&p).unwrap()
}

fn gray() -> Gray {
    Gray { width: 4, height: 4, px: vec![0.5; 16] }
}

fn outputs(id: u64) -> Outputs {
    Outputs {
        id,
        draft: false,
        size: (4, 4),
        report: MasterReport { fit_mean_deg: Some(1.2), fit_p95_deg: Some(3.4), span_known: true, ..Default::default() },
        match_depth: 0.4,
        height: (gray(), 12.4),
        parallax: Some((gray(), ParallaxReport { times_normal: 9.6, shader_depth_pct: 1.0, levels_used: 61, clipped_share: 0.0 })),
        normal: Some(vec![[0.5, 0.5, 1.0, 1.0]; 16]),
        preview_normal: vec![[0.5, 0.5, 1.0, 1.0]; 16],
        ao: Some(gray()),
        cavity: Some(gray()),
        curvature: Some(gray()),
        compare: None,
        preview: None,
    }
}

fn jobs(r: &Rig) -> Vec<Job> {
    r.jobs.try_iter().collect()
}

fn label_exists(h: &Harness<'_, Tab>, text: &str) -> bool {
    h.query_by_label(text).is_some()
}

#[test]
fn the_empty_tab_has_one_action() {
    let (mut h, _rig) = rig(1280.0);
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Open a normal or height map…");
    assert!(h.get_by_role_and_label(Role::Button, "Save").accesskit_node().is_disabled());
    assert_eq!(h.query_all_by_role(Role::Slider).count(), 0);
}

#[test]
fn opening_sends_one_full_job() {
    let (mut h, rig) = rig(1280.0);
    let o = normal_source("open");
    h.state_mut().accept(o.clone());
    h.run_steps(2);
    let sent = jobs(&rig);
    assert_eq!(sent.len(), 1);
    assert!(!sent[0].draft);
    assert_eq!(sent[0].settings, Settings::for_source(&o.kind, (64, 64)));
}

#[test]
fn dragging_flatten_sends_drafts_and_release_sends_a_full_job() {
    let (mut h, rig) = rig(1280.0);
    h.state_mut().accept(normal_source("drag"));
    {
        // Height data alone: it opens and, with the sections above it closed, its slider is in reach of the pointer.
        let t = &mut h.state_mut().settings.as_mut().unwrap().ticks;
        (t.height, t.parallax, t.ao) = (true, false, false);
    }
    h.run_steps(2);
    let id = h.state().jobs_sent;
    rig.replies.send(Reply::Done(Box::new(outputs(id)))).unwrap();
    h.run_steps(2);
    jobs(&rig);
    let c = h.get_by_role_and_label(Role::Slider, "Flatten broad shapes").rect().center();
    h.hover_at(c);
    h.step();
    h.drag_at(c);
    h.step();
    h.hover_at(c + egui::vec2(30.0, 0.0));
    h.run_steps(2);
    let during = jobs(&rig);
    assert!(during.iter().any(|j| j.draft), "a draft while dragging");
    h.drop_at(c + egui::vec2(30.0, 0.0));
    h.run_steps(2);
    let last = jobs(&rig).pop().expect("a job on release");
    assert!(!last.draft);
    assert_eq!(last.settings.master.flatten, h.state().settings.as_ref().unwrap().master.flatten);
}

/// Presses the slider named `label` at its centre and moves it right for ten frames at 60 a second. Returns where
/// the pointer ends, still held.
fn hold_and_move(h: &mut Harness<'_, Tab>, label: &str) -> egui::Pos2 {
    let c = h.get_by_role_and_label(Role::Slider, label).rect().center();
    h.hover_at(c);
    h.step();
    h.drag_at(c);
    h.step();
    for k in 1..=10 {
        h.hover_at(c + egui::vec2(3.0 * k as f32, 0.0));
        h.step();
    }
    c + egui::vec2(30.0, 0.0)
}

#[test]
fn a_moving_slider_sends_no_job_until_it_rests() {
    let (mut h, rig) = rig_dt(1280.0, 1.0 / 60.0);
    h.state_mut().accept(normal_source("rest"));
    h.run_steps(2);
    rig.replies.send(Reply::Done(Box::new(outputs(1)))).unwrap();
    h.run_steps(2);
    jobs(&rig);
    let end = hold_and_move(&mut h, "Depth");
    assert!(jobs(&rig).is_empty(), "no job while the slider moves");
    h.run_steps(20); // a third of a second at rest, the button still down
    let sent = jobs(&rig);
    assert_eq!(sent.len(), 1, "one job once the slider rests");
    assert_eq!(sent[0].settings, *h.state().settings.as_ref().unwrap());
    h.drop_at(end);
    h.run_steps(2);
    assert!(jobs(&rig).is_empty(), "the resting value was sent already");
}

#[test]
fn a_longer_update_delay_holds_the_job_back() {
    let (mut h, rig) = rig_dt(1280.0, 1.0 / 60.0);
    texalith::settle::set_rest(&h.ctx, 1.0);
    h.state_mut().accept(normal_source("long-rest"));
    h.run_steps(2);
    rig.replies.send(Reply::Done(Box::new(outputs(1)))).unwrap();
    h.run_steps(2);
    jobs(&rig);
    hold_and_move(&mut h, "Depth");
    h.run_steps(20); // a third of a second at rest: enough for the default delay, not for one second
    assert!(jobs(&rig).is_empty(), "the delay in use is the one set");
    h.run_steps(50);
    assert_eq!(jobs(&rig).len(), 1);
}

#[test]
fn letting_go_of_a_slider_sends_its_job_at_once() {
    let (mut h, rig) = rig_dt(1280.0, 1.0 / 60.0);
    h.state_mut().accept(normal_source("let-go"));
    h.run_steps(2);
    rig.replies.send(Reply::Done(Box::new(outputs(1)))).unwrap();
    h.run_steps(2);
    jobs(&rig);
    let end = hold_and_move(&mut h, "Depth");
    h.drop_at(end);
    h.run_steps(2);
    let sent = jobs(&rig);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].settings, *h.state().settings.as_ref().unwrap());
}

#[test]
fn moving_a_slider_stops_the_running_job_and_a_new_one_follows() {
    let (mut h, rig) = rig_dt(1280.0, 1.0 / 60.0);
    h.state_mut().accept(normal_source("stop"));
    h.run_steps(2);
    let first = jobs(&rig).pop().expect("the job for the opened file"); // still running: no reply
    let end = hold_and_move(&mut h, "Depth");
    assert!(first.cancel.load(std::sync::atomic::Ordering::Relaxed), "its result would land in the middle of the drag");
    assert!(jobs(&rig).is_empty());
    // The thread can be past its last cancel check (packing the preview's maps) and still answer.
    rig.replies.send(Reply::Done(Box::new(outputs(first.id)))).unwrap();
    h.run_steps(2);
    assert!(h.state().outputs.is_none(), "a stopped job's result is not shown");
    h.drop_at(end);
    h.run_steps(2);
    let last = jobs(&rig).pop().expect("a job on release");
    assert!(!last.cancel.load(std::sync::atomic::Ordering::Relaxed));
    assert_eq!(last.settings, *h.state().settings.as_ref().unwrap());
}

#[test]
fn an_output_slider_sends_a_job_with_the_same_master_options() {
    let (mut h, rig) = rig(1280.0);
    h.state_mut().accept(normal_source("output"));
    h.run_steps(2);
    rig.replies.send(Reply::Done(Box::new(outputs(1)))).unwrap();
    h.run_steps(2);
    let before = jobs(&rig).pop().unwrap().settings.master;
    let r = h.get_by_role_and_label(Role::Slider, "Depth").rect();
    let at = egui::pos2(r.left() + 4.0, r.center().y);
    h.hover_at(at);
    h.step();
    h.drag_at(at);
    h.step();
    h.drop_at(at);
    h.run_steps(3);
    let last = jobs(&rig).pop().expect("a job after the change");
    assert_eq!(last.settings.master, before);
    assert!(last.settings.parallax.depth < 0.9);
}

#[test]
fn a_stale_reply_is_ignored() {
    let (mut h, rig) = rig(1280.0);
    h.state_mut().accept(normal_source("stale"));
    h.run_steps(2);
    h.state_mut().submit(false);
    h.run_steps(2);
    assert_eq!(jobs(&rig).last().unwrap().id, 2);
    rig.replies.send(Reply::Done(Box::new(outputs(1)))).unwrap();
    h.run_steps(2);
    assert!(h.state().outputs.is_none());
}

const NORMAL_SOURCE_HELP: [(&str, &str); 11] = [
    ("Output size", "Sets how sharp the maps are and how long they take; lower it for quick tries, and raise it for the final maps."),
    ("Edges", "Says whether opposite edges join up; leave it on Repeats for tiling textures, and choose Does not repeat only for a crop or single photo, because the wrong choice on a tile leaves a hard seam."),
    ("Flatten broad shapes", "Fades hills and slopes wider than the size shown; stay near 0 for rock, ground and maps whose height truly matches the normal, move toward 1 for brick, bark, weave and carved parts, and use Compare if unsure."),
    ("Depth", "Sets how far the surface appears to rise and sink; raise it if the effect is hard to see, and lower it if the surface warps or swims at low angles."),
    ("Flat level", "Details rise and sink around mid-grey; use this for ordinary parallax shaders, including True PBR."),
    ("Reach", "Sets how far shadow spreads from each crevice; it matters most on deep surfaces like bark, so raise it for soft broad shading and lower it if whole areas turn grey."),
    ("AO strength", "Makes the shadows this map already has darker or lighter without moving them; leave it at 1 if your renderer has its own AO strength setting, and raise it only if the map will be multiplied in directly."),
    ("Surface depth", "Treats the surface as deeper, so shadows reach shallower dips and change shape; raise it if AO stays nearly white, which is common because a normal map implies only shallow relief."),
    ("Line thickness", "Sets how wide the bright edges and dark creases are; raise it to catch large forms, and lower it for thin crisp lines."),
    ("Contrast", "Makes edges and creases stand out from the grey; raise it if the map looks nearly flat, and lower it if large areas hit pure black or white."),
    ("Crevice width", "Sets the widest crack that goes dark; raise it if grooves stay too light, and lower it if it starts to look like broad AO."),
];

#[test]
fn every_option_has_a_help_button_and_no_sentence_is_drawn() {
    let (mut h, rig) = rig(1280.0);
    h.state_mut().accept(normal_source("help"));
    {
        let t = &mut h.state_mut().settings.as_mut().unwrap().ticks;
        (t.height, t.curvature, t.cavity) = (true, true, true);
    }
    rig.replies.send(Reply::Done(Box::new(outputs(1)))).unwrap();
    h.run_steps(3);
    for (what, text) in NORMAL_SOURCE_HELP {
        h.get_by_role_and_label(Role::Button, &texalith::tab::hint_name(what, text));
        assert!(!label_exists(&h, text), "still drawn: {text}");
    }
    assert!(label_exists(&h, "Estimated from the normal, not baked from a mesh."));
    assert!(label_exists(&h, "If you have a real height, load it."));
}

#[test]
fn an_unticked_height_with_parallax_says_it_will_not_be_saved() {
    let (mut h, _rig) = rig(1280.0);
    h.state_mut().accept(normal_source("unticked"));
    h.run_steps(2);
    let line = "Height data is still made for the Parallax map, but it will not be saved.";
    assert!(label_exists(&h, line)); // Height data starts unticked
    h.state_mut().settings.as_mut().unwrap().ticks.height = true;
    h.run_steps(2);
    assert!(!label_exists(&h, line));
}

#[test]
fn height_data_starts_unticked_below_the_other_maps() {
    let (mut h, rig) = rig(1280.0);
    h.state_mut().accept(normal_source("height-last"));
    rig.replies.send(Reply::Done(Box::new(outputs(1)))).unwrap();
    h.run_steps(3);
    assert_eq!(toggled(&h, Role::CheckBox, "Height data"), Some(egui::accesskit::Toggled::False));
    h.get_by_role_and_label(Role::Button, "Show Height data settings");
    let top = |name: &str| h.get_by_role_and_label(Role::CheckBox, name).rect().top();
    for name in ["Parallax", "AO", "Curvature", "Cavity"] {
        assert!(top("Height data") > top(name), "Height data is above {name}");
    }
    h.get_by_role_and_label(Role::Button, "Save 2 maps");
}

#[test]
fn a_height_source_shows_scale_unknown_and_the_normal_row() {
    let (mut h, _rig) = rig(1280.0);
    let o = height_source("height");
    assert_eq!(o.kind, Kind::Height { span_texels: None });
    h.state_mut().accept(o);
    h.run_steps(2);
    assert!(label_exists(&h, "Scale unknown"));
    h.get_by_role_and_label(Role::ComboBox, "Green direction");
}

#[test]
fn reset_buttons_name_their_default() {
    let (mut h, _rig) = rig(1280.0);
    h.state_mut().accept(normal_source("reset"));
    {
        let s = h.state_mut().settings.as_mut().unwrap();
        (s.master.flatten, s.ticks.height) = (0.8, true); // the tick opens Height data, where this reset is
    }
    h.run_steps(2);
    // Height data is the last section, below the fold for a pointer click.
    h.get_by_role_and_label(Role::Button, "Reset Flatten broad shapes to 0.3").click_accesskit();
    h.run_steps(2);
    assert_eq!(h.state().settings.as_ref().unwrap().master.flatten, 0.3);
}

/// A tab whose outputs are in, with `rock_n.png` open; returns its folder.
fn ready(name: &str) -> (Harness<'static, Tab>, Rig, PathBuf) {
    let (mut h, rig) = rig(1280.0);
    let o = normal_source(name);
    let dir = o.path.parent().unwrap().to_path_buf();
    h.state_mut().accept(o);
    // Height data ticked too: these tests save three maps and use the rows under Height data.
    h.state_mut().settings.as_mut().unwrap().ticks.height = true;
    h.run_steps(2);
    // The tick sent a second job; the reply answers the newest.
    let id = h.state().jobs_sent;
    rig.replies.send(Reply::Done(Box::new(outputs(id)))).unwrap();
    h.run_steps(2);
    (h, rig, dir)
}

/// A plain PNG with no marker at `rock_ao.png`; returns its bytes.
fn foreign(dir: &std::path::Path) -> Vec<u8> {
    let p = dir.join("rock_ao.png");
    image::GrayImage::from_fn(4, 4, |x, _| image::Luma([x as u8 * 60])).save(&p).unwrap();
    std::fs::read(p).unwrap()
}

#[test]
fn the_new_words_are_on_the_column() {
    let (h, _rig, _dir) = ready("words");
    h.get_by_role_and_label(Role::Slider, "AO strength");
    h.get_by_role_and_label(Role::Button, "Reset AO strength to 1");
    assert!(h.query_by_role_and_label(Role::Slider, "Darkness").is_none());
    assert_eq!(h.query_all_by_label("Fine surface detail").count(), 2); // under Height and under Parallax
    assert_eq!(h.query_all_by_label("Surface detail only").count(), 0);
    // The preset combo is also labelled "Material", so the heading is counted, not found alone.
    assert!(h.query_all_by_label("Material").count() >= 2);
    assert!(!label_exists(&h, "From normal"));
}

const REVIEW: &str = "Some files will be kept";
const BACKUP: &str = "Keep a backup in _sdc\\texalith-backup";

/// An AccessKit click: `click()` is a pointer click at the widget's centre, and Save sits below the 900 px fold.
fn press(h: &mut Harness<'_, Tab>, label: &str) {
    h.get_by_role_and_label(Role::Button, label).click_accesskit();
    h.run_steps(2);
    saved(h);
}

/// Steps until the running save has finished and its result is shown.
fn saved(h: &mut Harness<'_, Tab>) {
    let start = std::time::Instant::now();
    while h.state().saving.is_some() {
        assert!(start.elapsed().as_secs() < 300, "the save never finished");
        std::thread::sleep(std::time::Duration::from_millis(2));
        h.run_steps(1);
    }
    h.run_steps(2);
}

/// A save that waits for the returned sender, then has nothing to write.
fn held_save(ctx: &egui::Context) -> (std::sync::mpsc::Sender<()>, texalith::save::Saving) {
    let (gate, wait) = std::sync::mpsc::channel();
    let say = Box::new(|w, k| format!("{w} written, {k} kept"));
    let s = texalith::save::start(ctx, vec![], say, move || {
        let _ = wait.recv();
        vec![]
    });
    (gate, s)
}

fn toggled(h: &Harness<'_, Tab>, role: Role, label: &str) -> Option<egui::accesskit::Toggled> {
    h.get_by_role_and_label(role, label).accesskit_node().toggled()
}

#[test]
fn save_without_conflicts_writes_at_once() {
    let (mut h, _rig, dir) = ready("save");
    press(&mut h, "Save 3 maps");
    for f in ["rock_height.png", "rock_parallax.png", "rock_ao.png"] {
        assert!(dir.join(f).is_file(), "{f} written; status {:?}", h.state().status);
    }
    assert!(!label_exists(&h, REVIEW));
    match &h.state().status {
        Some(Ok(m)) => assert!(m.starts_with("Saved 3 maps to "), "{m}"),
        s => panic!("status {s:?}"),
    }
}

#[test]
fn a_foreign_file_opens_the_dialog_with_keep_chosen() {
    let (mut h, _rig, dir) = ready("foreign");
    foreign(&dir);
    press(&mut h, "Save 3 maps");
    assert!(label_exists(&h, REVIEW));
    assert!(label_exists(&h, "rock_ao.png"));
    assert!(label_exists(&h, "Not made by Texalith, or its marker was removed"));
    assert_eq!(toggled(&h, Role::Button, "Keep rock_ao.png"), Some(egui::accesskit::Toggled::True));
    assert_eq!(toggled(&h, Role::Button, "Overwrite rock_ao.png"), Some(egui::accesskit::Toggled::False));
    assert_eq!(toggled(&h, Role::CheckBox, BACKUP), Some(egui::accesskit::Toggled::True));
    assert!(!dir.join("rock_height.png").exists(), "nothing written before the answer");
}

#[test]
fn enter_in_the_dialog_does_nothing() {
    let (mut h, _rig, dir) = ready("enter");
    let old = foreign(&dir);
    press(&mut h, "Save 3 maps");
    for _ in 0..2 {
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
    }
    assert!(label_exists(&h, REVIEW));
    assert_eq!(std::fs::read(dir.join("rock_ao.png")).unwrap(), old);
    assert!(!dir.join("rock_height.png").exists());
}

#[test]
fn overwrite_with_backup_writes_and_keeps_the_old_file() {
    let (mut h, _rig, dir) = ready("overwrite");
    let old = foreign(&dir);
    press(&mut h, "Save 3 maps");
    press(&mut h, "Overwrite rock_ao.png");
    press(&mut h, "Save");
    assert!(!label_exists(&h, REVIEW));
    assert_ne!(std::fs::read(dir.join("rock_ao.png")).unwrap(), old);
    assert_eq!(std::fs::read(dir.join("_sdc").join("texalith-backup").join("rock_ao.png")).unwrap(), old);
    assert!(dir.join("rock_height.png").is_file());
    assert!(matches!(&h.state().status, Some(Ok(_))));
}

#[test]
fn cancel_writes_nothing() {
    let (mut h, _rig, dir) = ready("cancel");
    let old = foreign(&dir);
    press(&mut h, "Save 3 maps");
    press(&mut h, "Cancel");
    assert!(!label_exists(&h, REVIEW));
    press(&mut h, "Save 3 maps");
    assert!(label_exists(&h, REVIEW));
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(!label_exists(&h, REVIEW));
    assert_eq!(std::fs::read(dir.join("rock_ao.png")).unwrap(), old);
    assert!(!dir.join("rock_height.png").exists());
    assert!(!dir.join("_sdc").exists());
}

#[test]
fn opening_another_file_closes_the_dialog() {
    let (mut h, _rig, dir) = ready("reopen");
    let old = foreign(&dir);
    press(&mut h, "Save 3 maps");
    assert!(label_exists(&h, REVIEW));
    let (other, ctx) = (normal_source("reopen-other").path, h.ctx.clone());
    h.state_mut().start_open(other, &ctx);
    h.run_steps(2);
    assert!(h.state().review.is_none());
    assert!(!label_exists(&h, REVIEW));
    assert_eq!(std::fs::read(dir.join("rock_ao.png")).unwrap(), old);
}

#[test]
fn save_is_visible_without_scrolling() {
    let (h, _rig, _dir) = ready("visible");
    let r = h.get_by_role_and_label(Role::Button, "Save 3 maps").rect();
    assert!(r.max.y <= 900.0, "Save below the fold: {r:?}");
}

#[test]
fn the_column_fits_at_the_checked_widths() {
    for (width, source) in [(1100.0, 0), (1280.0, 0), (1600.0, 0), (1100.0, 1)] {
        let (mut h, rig) = rig(width);
        let name = format!("fit{width}-{source}");
        h.state_mut().accept(if source == 0 { normal_source(&name) } else { height_source(&name) });
        {
            let t = &mut h.state_mut().settings.as_mut().unwrap().ticks;
            (t.height, t.curvature, t.cavity) = (true, true, true);
        }
        rig.replies.send(Reply::Done(Box::new(outputs(1)))).unwrap();
        h.run_steps(3);
        // The harness adds a root margin, so the panel does not start at x = 0: bound by its own rect.
        let panel = egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("column"))
            .expect("the column was drawn")
            .outer_rect;
        assert!((panel.width() - COLUMN_WIDTH).abs() < 0.5, "column width at {width}: {panel:?}");
        let widgets = h.ctx.viewport(|v| v.this_pass.widgets.clone());
        for (_, layer) in widgets.layers() {
            // Containers have no sense and span the root; only widgets a person can use count. The
            // unclipped rect, so a row that needs horizontal scrolling to reach also fails.
            for w in layer.iter().filter(|w| w.sense != egui::Sense::hover()) {
                let r = w.rect;
                assert!(r.max.x <= panel.max.x + 0.5, "past the column at {width}: {r:?} {:?}", w.id);
            }
        }
    }
}

const RNM_WARNING: &str =
    "This normal was blended by RNM, so its height comes out less exact; open the base normal instead if you have it.";

#[test]
fn an_rnm_input_shows_the_warning() {
    let (mut h, _rig) = rig(1280.0);
    let mut o = normal_source("rnm-warning");
    h.state_mut().accept(o.clone());
    h.run_steps(2);
    assert!(!label_exists(&h, RNM_WARNING));
    o.marker = Some(texalith::marker::Marker {
        tool: "Texalith".into(),
        version: "test".into(),
        kind: texalith::marker::MapKind::Rnm,
        pixels_sha256: String::new(),
        source_path: "C:/t/rock_n.png".into(),
        source_sha256: "00".into(),
        settings: serde_json::json!({}),
        span_texels: None,
        master_sha256: None,
    });
    h.state_mut().accept(o);
    h.run_steps(2);
    assert!(label_exists(&h, RNM_WARNING));
}

fn column(h: &Harness<'_, Tab>) -> egui::Rect {
    egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("column")).expect("the column was drawn").outer_rect
}

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
    let panel = column(&h);
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
fn closing_a_section_keeps_its_progress_and_white_has_no_warning() {
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
    // The preview draws a White map resting on white, so nothing warns about it.
    assert!(!label_exists(&h, "This preview pivots on mid-grey, so a White map looks shifted in 3D."));
    assert_eq!(h.query_all_by_role(Role::ProgressIndicator).count(), 1);
}

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
    let (mut h, rig, _dir) = ready("shared-drag");
    // Height closed, so the copy under Parallax sits near the top: a pointer drag misses a slider below the fold.
    h.get_by_role_and_label(Role::Button, "Hide Height data settings").click_accesskit();
    h.run_steps(3);
    jobs(&rig);
    let c = h.get_by_role_and_label(Role::Slider, SHARED).rect().center();
    assert!(c.y < 880.0, "the shared slider is below the fold: {c:?}");
    h.hover_at(c);
    h.step();
    h.drag_at(c);
    h.step();
    h.hover_at(c + egui::vec2(30.0, 0.0));
    h.run_steps(2);
    assert!(jobs(&rig).iter().any(|j| j.draft), "a draft while dragging");
    h.drop_at(c + egui::vec2(30.0, 0.0));
    h.run_steps(2);
    let last = jobs(&rig).pop().expect("a job on release");
    assert!(!last.draft);
    assert_eq!(last.settings.master.flatten, h.state().settings.as_ref().unwrap().master.flatten);
}

#[test]
fn a_height_source_has_no_flatten() {
    let (mut h, rig) = rig(1280.0);
    h.state_mut().accept(height_source("no-flatten"));
    rig.replies.send(Reply::Done(Box::new(outputs(1)))).unwrap();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Slider, "Depth");
    assert!(h.query_by_role_and_label(Role::Slider, "Flatten broad shapes").is_none());
    assert!(h.query_by_role_and_label(Role::Slider, SHARED).is_none());
}

#[test]
fn the_parallax_readouts_live_in_advanced() {
    let (mut h, _rig, _dir) = ready("readouts"); // outputs(1): 9.6 times, 61 levels, nothing clipped
    let lines = ["9.6× deeper than the normal implies", "Levels used: 61 of 256", "Clipped: 0.0 %"];
    assert!(lines.iter().all(|l| !label_exists(&h, l)));
    h.get_by_label("Parallax advanced").click_accesskit();
    h.run_steps(3);
    for l in lines {
        assert!(label_exists(&h, l), "{l}");
    }
}

#[test]
fn few_levels_warn_outside_advanced() {
    let (mut h, rig) = rig(1280.0);
    h.state_mut().accept(normal_source("levels"));
    let mut o = outputs(1);
    o.parallax.as_mut().unwrap().1.levels_used = 33;
    rig.replies.send(Reply::Done(Box::new(o))).unwrap();
    h.run_steps(3);
    let line = "Only 33 of 256 grey levels are used; raise Depth.";
    assert!(label_exists(&h, line));
    h.get_by_role_and_label(Role::Button, "Hide Parallax settings").click_accesskit();
    h.run_steps(3);
    assert!(label_exists(&h, line));
}

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
    h.state_mut().settings.as_mut().unwrap().ticks.height = true;
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
    h.state_mut().settings.as_mut().unwrap().ao.reach = 0.2;
    h.run_steps(2);
    assert!(off(&h)); // the full outputs are still shown, but a newer job is running
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

#[test]
fn compare_under_parallax_shows_its_strip_there() {
    let (mut h, rig, _dir) = ready("compare-shared");
    // Height closed: the strip under Height cannot be the one found.
    h.get_by_role_and_label(Role::Button, "Hide Height data settings").click_accesskit();
    h.run_steps(3);
    press(&mut h, "Compare (shared with Height)");
    let job = jobs(&rig).pop().expect("Compare sends a job");
    assert!(job.compare);
    let three = Some([gray(), gray(), gray()]);
    rig.replies.send(Reply::Done(Box::new(Outputs { compare: three, ..outputs(job.id) }))).unwrap();
    h.run_steps(2);
    press(&mut h, "Use Flatten 1 (shared with Height)");
    assert_eq!(h.state().settings.as_ref().unwrap().master.flatten, 1.0);
}

#[test]
fn both_compare_strips_have_their_own_names() {
    let (mut h, rig, _dir) = ready("compare-both");
    press(&mut h, "Compare");
    let job = jobs(&rig).pop().expect("Compare sends a job");
    let three = Some([gray(), gray(), gray()]);
    rig.replies.send(Reply::Done(Box::new(Outputs { compare: three, ..outputs(job.id) }))).unwrap();
    h.run_steps(2);
    for label in ["0", "Current", "1"] {
        h.get_by_role_and_label(Role::Button, &format!("Use Flatten {label}"));
        h.get_by_role_and_label(Role::Button, &format!("Use Flatten {label} (shared with Height)"));
    }
}

#[test]
fn picking_a_preset_leaves_its_section_open() {
    let (mut h, _rig, _dir) = ready("preset");
    h.get_by_role_and_label(Role::ComboBox, "Material").click_accesskit();
    h.run_steps(2);
    let (name, v) = texalith::from_normal::settings::PRESETS[0];
    h.get_by_role_and_label(Role::Button, &format!("{name} ({v})")).click_accesskit();
    h.run_steps(3);
    assert_eq!(h.state().settings.as_ref().unwrap().master.flatten, v);
    h.get_by_role_and_label(Role::Button, "Hide Height data settings");
    h.get_by_role_and_label(Role::Slider, "Flatten broad shapes");
}

#[test]
fn match_the_normal_keeps_the_exact_depth() {
    let (mut h, rig) = rig(1280.0);
    h.state_mut().accept(normal_source("match"));
    h.run_steps(2);
    let id = h.state().jobs_sent;
    // 1.465 %: between two whole percents, which is all the Depth slider shows.
    rig.replies.send(Reply::Done(Box::new(Outputs { match_depth: 0.01465, ..outputs(id) }))).unwrap();
    h.run_steps(2);
    jobs(&rig);
    h.get_by_role_and_label(Role::Button, "Match the normal").click_accesskit();
    h.run_steps(4);
    assert_eq!(h.state().settings.as_ref().unwrap().parallax.depth, 0.01465);
    let sent = jobs(&rig);
    assert_eq!(sent.len(), 1, "one job for the matched depth, none for a rounded copy of it");
    assert_eq!(sent[0].settings.parallax.depth, 0.01465);
}

#[test]
fn a_value_that_comes_back_after_its_job_was_stopped_is_sent_again() {
    let (mut h, rig) = rig_dt(1280.0, 1.0 / 60.0);
    h.state_mut().accept(normal_source("back"));
    h.run_steps(2);
    let first = jobs(&rig).pop().expect("the job for the opened file"); // still running: no reply
    // The button is held (on nothing), so a changed value waits and the running job is stopped.
    let held = egui::pos2(900.0, 450.0);
    h.drag_at(held);
    h.step();
    h.state_mut().settings.as_mut().unwrap().parallax.depth = 0.5;
    h.run_steps(2);
    assert!(first.cancel.load(std::sync::atomic::Ordering::Relaxed));
    h.state_mut().settings = Some(first.settings.clone());
    h.step();
    h.drop_at(held);
    h.run_steps(2);
    let again = jobs(&rig).pop().expect("the stopped job's value has no result, so it is sent again");
    assert_eq!(again.settings, first.settings);
}

#[test]
fn a_value_that_comes_back_with_its_result_on_screen_sends_no_job() {
    let (mut h, rig) = rig_dt(1280.0, 1.0 / 60.0);
    h.state_mut().accept(normal_source("back-shown"));
    h.run_steps(2);
    let first = jobs(&rig).pop().unwrap();
    rig.replies.send(Reply::Done(Box::new(outputs(first.id)))).unwrap();
    h.run_steps(2);
    let held = egui::pos2(900.0, 450.0);
    h.drag_at(held);
    h.step();
    h.state_mut().settings.as_mut().unwrap().parallax.depth = 0.5;
    h.run_steps(2);
    h.state_mut().settings = Some(first.settings.clone());
    h.step();
    h.drop_at(held);
    h.run_steps(2);
    assert!(jobs(&rig).is_empty(), "these settings have their result on screen already");
    assert!(!h.state().running());
}

#[test]
fn stopping_a_job_clears_its_progress() {
    let (mut h, rig) = rig_dt(1280.0, 1.0 / 60.0);
    h.state_mut().accept(normal_source("progress"));
    h.run_steps(2);
    let first = jobs(&rig).pop().unwrap();
    first.progress.store(600, std::sync::atomic::Ordering::Relaxed);
    h.run_steps(2);
    let bar = |h: &Harness<'_, Tab>| h.get_by_role(Role::ProgressIndicator).accesskit_node().numeric_value();
    assert_eq!(bar(&h), Some(60.0));
    let held = egui::pos2(900.0, 450.0);
    h.drag_at(held);
    h.step();
    h.state_mut().settings.as_mut().unwrap().parallax.depth = 0.5;
    h.run_steps(2);
    assert!(first.cancel.load(std::sync::atomic::Ordering::Relaxed));
    assert_eq!(bar(&h), Some(0.0), "the stopped job's 60 % is not the next job's progress");
}

#[test]
fn save_is_off_while_a_save_runs() {
    let (mut h, _rig, _dir) = ready("saving");
    let off = |h: &Harness<'_, Tab>, l: &str| h.get_by_role_and_label(Role::Button, l).accesskit_node().is_disabled();
    assert!(!off(&h, "Save 3 maps") && !off(&h, "Save AO"));
    let (gate, saving) = held_save(&h.ctx);
    h.state_mut().saving = Some(saving);
    h.run_steps(2);
    assert!(off(&h, "Save 3 maps") && off(&h, "Save AO"));
    assert!(label_exists(&h, "Saving…"));
    gate.send(()).unwrap();
    saved(&mut h);
    assert!(!off(&h, "Save 3 maps") && !label_exists(&h, "Saving…"));
    assert_eq!(h.state().status, Some(Ok("0 written, 0 kept".into())));
}

#[test]
fn a_save_still_asks_after_another_file_is_opened() {
    let (mut h, _rig, dir) = ready("saving-open");
    // A held save whose one file is on disk and not Texalith's, so it ends in a question.
    let path = dir.join("taken.png");
    std::fs::write(&path, b"someone else's file").unwrap();
    let (gate, wait) = channel::<()>();
    let say = Box::new(|w, k| format!("{w} written, {k} kept, as pressed"));
    let saving = texalith::save::start(&h.ctx, vec![], say, move || {
        let _ = wait.recv();
        vec![(MapKind::Ao, texalith::export::Planned { path, bytes: vec![1, 2, 3] })]
    });
    h.state_mut().saving = Some(saving);
    h.run_steps(2);
    h.state_mut().accept(normal_source("saving-open-other"));
    assert!(h.state().saving.is_some(), "the save still runs, so Save stays off until it answers");
    gate.send(()).unwrap();
    saved(&mut h);
    assert!(label_exists(&h, REVIEW), "the question is asked, not dropped");
    // The status line is the one of the Save press, not one made from the file open now. The dialog's Save is the
    // one that is on: the column's waits for the new file's maps.
    let on = |b: &egui_kittest::Node<'_>| b.accesskit_node().label().as_deref() == Some("Save") && !b.accesskit_node().is_disabled();
    h.query_all_by_role(Role::Button).find(on).expect("the dialog's Save").click_accesskit();
    h.run_steps(2);
    saved(&mut h);
    assert_eq!(h.state().status, Some(Ok("0 written, 1 kept, as pressed".into())));
    assert_eq!(std::fs::read(dir.join("taken.png")).unwrap(), b"someone else's file");
}

#[test]
fn a_file_still_being_read_already_counts_as_open() {
    // Another tab's Save asks for this list: a file picked here must be kept before its pixels have arrived.
    let (mut h, _rig) = rig(1280.0);
    let p = temp("opening").join("rock_n.png");
    image::RgbImage::from_fn(64, 64, |x, _| image::Rgb([(120 + x) as u8, 128, 230])).save(&p).unwrap();
    let ctx = h.ctx.clone();
    h.state_mut().start_open(p.clone(), &ctx);
    assert_eq!(h.state().open_files(), vec![p]);
}

#[test]
fn a_question_is_not_closed_by_the_arrival_of_a_file_picked_before_it() {
    let (mut h, _rig, dir) = ready("saving-pick");
    let path = dir.join("taken.png");
    std::fs::write(&path, b"someone else's file").unwrap();
    let (gate, wait) = channel::<()>();
    let saving = texalith::save::start(&h.ctx, vec![], Box::new(|w, k| format!("{w} written, {k} kept")), move || {
        let _ = wait.recv();
        vec![(MapKind::Ao, texalith::export::Planned { path, bytes: vec![1, 2, 3] })]
    });
    h.state_mut().saving = Some(saving);
    h.run_steps(2);
    // The save asks while the picked file is still being read.
    gate.send(()).unwrap();
    saved(&mut h);
    assert!(label_exists(&h, REVIEW));
    h.state_mut().status = Some(Err("a line from the save".into()));
    // The file arrives.
    h.state_mut().accept(normal_source("saving-pick-other"));
    h.run_steps(2);
    assert!(label_exists(&h, REVIEW), "the question is still there to answer");
    assert_eq!(h.state().status, Some(Err("a line from the save".into())));
}
