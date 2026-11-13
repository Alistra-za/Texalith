//! The Cubemap column with a real cube thread. Fixtures are small 8-bit PNGs in a temp folder.
use eframe::egui;
use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use std::path::{Path, PathBuf};
use std::time::Duration;
use texalith::cube::tab::{CubeTab, OPEN_SOURCE};
use texalith::cube::{worker, Ground, Settings, SourceType};
use texalith::marker::{self, MapKind};
use texalith::tab::COLUMN_WIDTH;

const EMPTY: &str = "Open a panorama, cross, strip, mirror ball, angular map, photo or one of six face files to make a cube.";

fn rig<'a>() -> Harness<'a, CubeTab> {
    rig_dt(0.25)
}

/// `rig` with `dt` seconds between frames; kittest's own step is a quarter second, longer than a slider's rest.
fn rig_dt<'a>(dt: f32) -> Harness<'a, CubeTab> {
    let (jobs, outputs) = worker::spawn(|| {});
    Harness::builder().with_size(egui::vec2(1280.0, 900.0)).with_step_dt(dt).build_ui_state(
        |ui, tab: &mut CubeTab| {
            tab.drain();
            egui::Panel::left("column").resizable(false).exact_size(COLUMN_WIDTH).show(ui, |ui| tab.draw_column(ui));
        },
        CubeTab::new(jobs, outputs),
    )
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("texalith-cube-tab-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A `w` x `h` 8-bit RGB PNG with a gradient, so no cross cell is flat by accident.
fn png(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
    let p = dir.join(name);
    image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 4) as u8, (y * 8) as u8, 128])).save(&p).unwrap();
    p
}

/// Runs frames until nothing loads and the newest job's output has arrived.
fn settle(h: &mut Harness<'_, CubeTab>) {
    for _ in 0..1000 {
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

fn open(h: &mut Harness<'_, CubeTab>, p: &Path) {
    let ctx = h.ctx.clone();
    h.state_mut().open(p.to_path_buf(), &ctx);
    settle(h);
}

fn shown(h: &Harness<'_, CubeTab>, text: &str) -> bool {
    h.query_by_label(text).is_some()
}

fn combo(h: &Harness<'_, CubeTab>) -> String {
    h.get_by_role_and_label(Role::ComboBox, "Source type").accesskit_node().value().map(|v| v.to_string()).unwrap_or_default()
}

fn pick(h: &mut Harness<'_, CubeTab>, name: &str) {
    h.get_by_role_and_label(Role::ComboBox, "Source type").click();
    h.run_steps(3);
    h.get_by_label(name).click();
    h.run_steps(3);
}

/// Steps until the running save has finished and its result is shown.
fn saved(h: &mut Harness<'_, CubeTab>) {
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

fn save_disabled(h: &Harness<'_, CubeTab>) -> bool {
    h.get_by_role_and_label(Role::Button, "Save").accesskit_node().is_disabled()
}

#[test]
fn an_empty_tab_shows_only_the_open_row() {
    let mut h = rig();
    h.run_steps(2);
    assert!(shown(&h, EMPTY));
    h.get_by_role_and_label(Role::Button, OPEN_SOURCE);
    assert!(h.query_by_role_and_label(Role::Button, "Save").is_none());
    assert_eq!(h.query_all_by_role(Role::Slider).count(), 0);
}

#[test]
fn a_panorama_is_detected_and_makes_a_cube() {
    let d = temp("pano");
    let p = png(&d, "pano.png", 64, 32);
    let mut h = rig();
    open(&mut h, &p);
    assert_eq!(combo(&h), "Panorama (2:1)");
    h.get_by_role_and_label(Role::Slider, "Reflection softness");
    h.get_by_role_and_label(Role::Slider, "Brightness");
    let o = h.state().outputs.clone().expect("an output");
    assert_eq!((o.n, o.cube.size), (16, 16));
}

#[test]
fn a_moving_brightness_slider_sends_no_job_until_it_rests() {
    let d = temp("rest");
    let p = png(&d, "pano.png", 64, 32);
    let mut h = rig_dt(1.0 / 60.0);
    open(&mut h, &p);
    let sent = h.state().jobs_sent;
    let c = h.get_by_role_and_label(Role::Slider, "Brightness").rect().center();
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
    let p = png(&d, "pano.png", 64, 32);
    // A stand-in for the cube thread, so a job can still be running when the slider moves.
    let (job_tx, job_rx) = std::sync::mpsc::channel::<worker::Job>();
    let (out_tx, out_rx) = std::sync::mpsc::channel();
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 900.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, tab: &mut CubeTab| {
            tab.drain();
            egui::Panel::left("column").resizable(false).exact_size(COLUMN_WIDTH).show(ui, |ui| tab.draw_column(ui));
        },
        CubeTab::new(job_tx, out_rx),
    );
    let ctx = h.ctx.clone();
    h.state_mut().open(p, &ctx);
    for _ in 0..500 {
        h.run_steps(1);
        if !h.state().busy() && h.state().jobs_sent > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    h.run_steps(2);
    let running = h.state().jobs_sent;
    assert!(running > 0 && job_rx.try_iter().count() > 0, "a job is out and has no result yet");
    let out = |id| {
        let faces = std::array::from_fn(|_| engine::image::Rgba32F::new(4, 4, [0.5; 4]));
        std::sync::Arc::new(worker::Output { id, n: 4, cube: engine::cube::Cube { size: 4, faces }, preview: None })
    };
    let c = h.get_by_role_and_label(Role::Slider, "Brightness").rect().center();
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
fn a_square_image_asks_for_a_source_type() {
    let d = temp("square");
    let p = png(&d, "ball.png", 32, 32);
    let mut h = rig();
    open(&mut h, &p);
    assert_eq!(combo(&h), "Pick one");
    assert_eq!(h.state().jobs_sent, 0);
    h.state_mut().folder = Some(temp("square-out"));
    h.run_steps(2);
    assert!(save_disabled(&h));
}

#[test]
fn a_type_that_does_not_fit_is_refused() {
    let d = temp("refuse");
    let p = png(&d, "pano.png", 64, 32);
    let mut h = rig();
    open(&mut h, &p);
    let sent = h.state().jobs_sent;
    pick(&mut h, "Horizontal strip (6:1)");
    settle(&mut h);
    assert_eq!(h.state().status, Some(Err("pano.png is 64x32, which does not fit Horizontal strip (6:1).".into())));
    assert_eq!(h.state().jobs_sent, sent);
    h.state_mut().folder = Some(temp("refuse-out"));
    h.run_steps(2);
    assert!(save_disabled(&h), "the shown cube was made as a panorama");
}

#[test]
fn single_photo_softens_to_27_degrees() {
    let d = temp("photo");
    let p = png(&d, "shot.png", 32, 32);
    let mut h = rig();
    open(&mut h, &p);
    pick(&mut h, "Single photo");
    settle(&mut h);
    assert_eq!(h.state().settings.softness, 0.6);
    h.get_by_role_and_label(Role::Button, "Reset Reflection softness to 27°");
    assert!(h.state().outputs.is_some());
}

#[test]
fn save_writes_one_marked_strip() {
    let d = temp("save");
    let p = png(&d, "pano.png", 64, 32);
    let out = temp("save-out");
    let mut h = rig();
    open(&mut h, &p);
    h.state_mut().folder = Some(out.clone());
    settle(&mut h);
    h.get_by_role_and_label(Role::Button, "Save").click_accesskit();
    h.run_steps(2);
    saved(&mut h);
    let bytes = std::fs::read(out.join("pano_e.png")).expect("the output was written");
    let r = png::Decoder::new(std::io::Cursor::new(&bytes)).read_info().unwrap();
    assert_eq!((r.info().width, r.info().height), (96, 16));
    assert_eq!((r.info().color_type, r.info().bit_depth), (png::ColorType::Rgb, png::BitDepth::Sixteen));
    assert_eq!(marker::read(&bytes).unwrap().kind, MapKind::Cube);
    match &h.state().status {
        Some(Ok(m)) => assert!(m.starts_with("Saved pano_e.png to "), "{m}"),
        other => panic!("{other:?}"),
    }
}

fn advanced(h: &mut Harness<'_, CubeTab>) {
    h.get_by_label("Advanced").click();
    h.run_steps(3);
}

fn value(h: &Harness<'_, CubeTab>, combo: &str) -> String {
    h.get_by_role_and_label(Role::ComboBox, combo).accesskit_node().value().map(|v| v.to_string()).unwrap_or_default()
}

fn has(h: &Harness<'_, CubeTab>, role: Role, label: &str) -> bool {
    h.query_by_role_and_label(role, label).is_some()
}

#[test]
fn advanced_is_closed_until_clicked() {
    let d = temp("adv-closed");
    let p = png(&d, "pano.png", 64, 32);
    let mut h = rig();
    open(&mut h, &p);
    assert!(!has(&h, Role::Slider, "Turn"));
    advanced(&mut h);
    for s in ["Turn", "Colour strength"] {
        assert!(has(&h, Role::Slider, s), "{s}");
    }
    for c in ["Mirror", "Highlight rolloff"] {
        assert!(has(&h, Role::CheckBox, c), "{c}");
    }
    assert_eq!(value(&h, "Side size"), "Auto (16 px)");
    assert_eq!(value(&h, "Ground"), "Keep the source");
    assert!(h.get_by_role_and_label(Role::Button, "No tint").accesskit_node().is_disabled());
    assert!(!has(&h, Role::ComboBox, "Fix one side"));
    assert!(!has(&h, Role::CheckBox, "Turn bottom cell"));
    assert!(!has(&h, Role::Slider, "Horizon line"));
}

#[test]
fn a_vertical_cross_offers_its_bottom_cell_and_face_fixes() {
    let d = temp("adv-vcross");
    let p = png(&d, "cross.png", 24, 32);
    let mut h = rig();
    open(&mut h, &p);
    pick(&mut h, "Vertical cross (3:4)");
    settle(&mut h);
    advanced(&mut h);
    let t = h.get_by_role_and_label(Role::CheckBox, "Turn bottom cell").accesskit_node().toggled();
    assert_eq!(t, Some(egui::accesskit::Toggled::True));
    assert_eq!(value(&h, "Fix one side"), "+X east");
    assert_eq!(value(&h, "Turn this face"), "None");
    assert_eq!(value(&h, "Flip this face"), "None");
}

#[test]
fn the_horizon_line_shows_once_a_ground_is_picked() {
    let d = temp("adv-horizon");
    let p = png(&d, "pano.png", 64, 32);
    let mut h = rig();
    open(&mut h, &p);
    h.state_mut().settings.ground = Ground::Darken;
    advanced(&mut h);
    assert!(has(&h, Role::Slider, "Horizon line"));
}

#[test]
fn a_side_size_of_256_makes_a_256_cube() {
    let d = temp("adv-side");
    let p = png(&d, "pano.png", 64, 32);
    let mut h = rig();
    open(&mut h, &p);
    h.state_mut().settings.side = Some(256);
    h.run_steps(1);
    settle(&mut h);
    assert_eq!(h.state().outputs.as_ref().expect("an output").n, 256);
}

#[test]
fn a_new_source_resets_the_advanced_settings() {
    let d = temp("adv-reset");
    let (a, b) = (png(&d, "a.png", 64, 32), png(&d, "b.png", 64, 32));
    let mut h = rig();
    open(&mut h, &a);
    let s = &mut h.state_mut().settings;
    (s.turn, s.mirror, s.face_turn[0]) = (90.0, true, 90);
    settle(&mut h);
    open(&mut h, &b);
    let want = Settings { source: Some(SourceType::Panorama), softness: 0.0, ..Settings::default() };
    assert_eq!(h.state().settings, want);
}

#[test]
fn help_sits_behind_buttons() {
    let d = temp("adv-help");
    let p = png(&d, "pano.png", 64, 32);
    let mut h = rig();
    open(&mut h, &p);
    advanced(&mut h);
    let turn = "Turns the surroundings around the up axis, and positive turns clockwise seen from above; use it to move the sun or a bright window to where the reflection should show it.";
    let ground = "Replaces everything below the horizon line; use it when the bottom of the source shows a tripod, a blank area or the wrong floor.";
    assert!(has(&h, Role::Button, &texalith::tab::hint_name("Turn", turn)));
    assert!(has(&h, Role::Button, &texalith::tab::hint_name("Ground", ground)));
    assert!(!shown(&h, turn));
}

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
    saved(&mut h);
    let bytes = std::fs::read(d.join("sky_e1.png")).expect("the output was written");
    assert_eq!(marker::read(&bytes).unwrap().kind, MapKind::Cube);
    assert_eq!(std::fs::read(&p).unwrap(), before);
}

#[test]
fn a_value_that_comes_back_after_its_result_was_dropped_is_sent_again() {
    let d = temp("back");
    let p = png(&d, "pano.png", 64, 32);
    let (job_tx, job_rx) = std::sync::mpsc::channel::<worker::Job>();
    let (_out_tx, out_rx) = std::sync::mpsc::channel();
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 900.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, tab: &mut CubeTab| {
            tab.drain();
            egui::Panel::left("column").resizable(false).exact_size(COLUMN_WIDTH).show(ui, |ui| tab.draw_column(ui));
        },
        CubeTab::new(job_tx, out_rx),
    );
    let ctx = h.ctx.clone();
    h.state_mut().open(p, &ctx);
    for _ in 0..500 {
        h.run_steps(1);
        if !h.state().busy() && h.state().jobs_sent > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    h.run_steps(2);
    let running = h.state().jobs_sent;
    assert!(running > 0 && job_rx.try_iter().count() > 0, "a job is out and has no result yet");
    // The button is held (on nothing), so a changed value waits and the running job's result is dropped.
    let held = egui::pos2(900.0, 450.0);
    h.drag_at(held);
    h.step();
    h.state_mut().settings.brightness = 1.0;
    h.run_steps(2);
    h.state_mut().settings.brightness = 0.0;
    h.step();
    h.drop_at(held);
    h.run_steps(2);
    assert_eq!(h.state().jobs_sent, running + 1, "the dropped result's value is sent again");
}

#[test]
fn save_is_off_while_a_save_runs() {
    let d = temp("saving");
    let mut h = rig();
    open(&mut h, &png(&d, "pano.png", 64, 32));
    settle(&mut h);
    assert!(!save_disabled(&h));
    let (gate, saving) = held_save(&h.ctx);
    h.state_mut().saving = Some(saving);
    h.run_steps(2);
    assert!(save_disabled(&h) && shown(&h, "Saving…"));
    // A new source while it runs: the save still answers, and Save stays off until it has.
    open(&mut h, &png(&d, "other.png", 64, 32));
    settle(&mut h);
    assert!(h.state().saving.is_some() && save_disabled(&h));
    gate.send(()).unwrap();
    saved(&mut h);
    assert_eq!(h.state().status, Some(Ok("0 written, 0 kept".into())));
    assert!(!save_disabled(&h));
}

#[test]
fn a_name_open_in_another_tab_is_passed_over() {
    let d = temp("elsewhere-name");
    let mut h = rig();
    open(&mut h, &png(&d, "pano.png", 64, 32));
    settle(&mut h);
    // pano_e.png is open in another tab.
    let other = png(&d, "pano_e.png", 48, 8);
    let before = std::fs::read(&other).unwrap();
    h.state_mut().elsewhere = vec![other.clone()];
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Save").click_accesskit();
    h.run_steps(2);
    saved(&mut h);
    assert!(d.join("pano_e1.png").is_file(), "status {:?}", h.state().status);
    assert_eq!(std::fs::read(&other).unwrap(), before);
}

#[test]
fn a_lone_picture_with_a_face_tag_keeps_its_whole_name() {
    // city_ny.png opened alone is not city's -Y face: city.png beside it also saves as city_e.png.
    let d = temp("lone-tag");
    let mut h = rig();
    open(&mut h, &png(&d, "city_ny.png", 64, 32));
    settle(&mut h);
    h.get_by_role_and_label(Role::Button, "Save").click_accesskit();
    h.run_steps(2);
    saved(&mut h);
    assert!(d.join("city_ny_e.png").is_file(), "status {:?}", h.state().status);
    assert!(!d.join("city_e.png").exists());
}

#[test]
fn a_file_still_being_read_already_counts_as_open() {
    // Another tab's Save asks for this list: a file picked here must be kept before its pixels have arrived.
    let d = temp("opening");
    let p = png(&d, "pano.png", 64, 32);
    let mut h = rig();
    let ctx = h.ctx.clone();
    h.state_mut().open(p.clone(), &ctx);
    assert_eq!(h.state().open_files(), vec![p.clone()]);
    settle(&mut h);
    assert_eq!(h.state().open_files(), vec![p]);
}

#[test]
fn a_question_is_not_closed_by_the_arrival_of_a_source_picked_before_it() {
    let d = temp("saving-pick");
    let mut h = rig();
    open(&mut h, &png(&d, "pano.png", 64, 32));
    settle(&mut h);
    let path = d.join("taken.png");
    std::fs::write(&path, b"someone else's file").unwrap();
    let (gate, wait) = std::sync::mpsc::channel::<()>();
    let saving = texalith::save::start(&h.ctx, vec![], Box::new(|w, k| format!("{w} written, {k} kept")), move || {
        let _ = wait.recv();
        vec![(MapKind::Cube, texalith::export::Planned { path, bytes: vec![1, 2, 3] })]
    });
    h.state_mut().saving = Some(saving);
    h.run_steps(2);
    // The pick, then the save's question, then the new source's arrival: all three before the next frame.
    let ctx = h.ctx.clone();
    h.state_mut().open(png(&d, "other.png", 64, 32), &ctx);
    gate.send(()).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    settle(&mut h);
    assert!(shown(&h, "Some files will be kept"), "the question is still there to answer");
}

#[test]
fn a_save_line_is_not_wiped_by_the_arrival_of_a_source_picked_before_it() {
    let d = temp("saving-line");
    let mut h = rig();
    open(&mut h, &png(&d, "pano.png", 64, 32));
    settle(&mut h);
    let (gate, saving) = held_save(&h.ctx);
    h.state_mut().saving = Some(saving);
    h.run_steps(2);
    // The pick, then the save's report, then the new source's arrival: all three before the next frame.
    let ctx = h.ctx.clone();
    h.state_mut().open(png(&d, "other.png", 64, 32), &ctx);
    gate.send(()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    settle(&mut h);
    assert!(h.state().outputs.is_some(), "the new source has its cube");
    assert_eq!(h.state().status, Some(Ok("0 written, 0 kept".into())));
}

#[test]
fn picking_another_source_closes_the_dialog_and_the_status_line() {
    let d = temp("pick-closes");
    let mut h = rig();
    open(&mut h, &png(&d, "pano.png", 64, 32));
    settle(&mut h);
    let path = d.join("taken.png");
    std::fs::write(&path, b"someone else's file").unwrap();
    let saving = texalith::save::start(&h.ctx, vec![], Box::new(|w, k| format!("{w} written, {k} kept")), move || {
        vec![(MapKind::Cube, texalith::export::Planned { path, bytes: vec![1, 2, 3] })]
    });
    h.state_mut().saving = Some(saving);
    saved(&mut h);
    assert!(shown(&h, "Some files will be kept"));
    h.state_mut().status = Some(Err("a line about the old source".into()));
    let ctx = h.ctx.clone();
    h.state_mut().open(png(&d, "other.png", 64, 32), &ctx);
    assert!(h.state().review.is_none() && h.state().status.is_none());
}

#[test]
fn the_refusal_line_goes_when_a_fitting_type_is_picked() {
    let d = temp("refuse-then-fit");
    let mut h = rig();
    open(&mut h, &png(&d, "pano.png", 64, 32));
    pick(&mut h, "Horizontal strip (6:1)");
    settle(&mut h);
    assert!(matches!(h.state().status, Some(Err(_))));
    pick(&mut h, "Panorama (2:1)");
    settle(&mut h);
    assert_eq!(h.state().status, None);
}

#[test]
fn the_save_button_stays_put_whatever_the_line_under_it_says() {
    let d = temp("status-row");
    let mut h = rig();
    open(&mut h, &png(&d, "pano.png", 64, 32));
    settle(&mut h);
    let top = |h: &Harness<'_, CubeTab>| h.get_by_role_and_label(Role::Button, "Save").rect().min.y;
    let idle = top(&h);
    let long = format!("Nothing to save: rock_n1.png in {} already matches", "a very long folder name ".repeat(12));
    h.state_mut().status = Some(Ok(long.clone()));
    h.run_steps(2);
    assert_eq!(top(&h), idle, "a long line");
    assert!(h.query_by_label(&long).is_some(), "the whole line is still there to read");
    h.state_mut().status = Some(Err(long));
    h.run_steps(2);
    assert_eq!(top(&h), idle, "a long error");
    let (_gate, saving) = held_save(&h.ctx);
    h.state_mut().status = None;
    h.state_mut().saving = Some(saving);
    h.run_steps(2);
    assert_eq!(top(&h), idle, "while a save runs");
}
