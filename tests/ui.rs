//! The window's own logic, with a stand-in for the render thread. No GPU.
use eframe::egui;
use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use render3d::scene::AUTO_YAW;
use render3d::Builtin;
use std::collections::HashSet;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;
use texalith::app::{App, FRAME_LABEL, MOVING_PIXELS};
use texalith::from_normal::maps::{FlatLevel, Gray, ParallaxReport};
use texalith::from_normal::master::MasterReport;
use texalith::from_normal::worker::{self, Outputs};
use texalith::preview::{Cmd, Frame, Reply, View};
use std::path::PathBuf;
use std::sync::Arc;
use texalith::from_normal::files::Source;
use texalith::rnm::blend::decode_base;
use texalith::rnm::files::Loaded;
use texalith::rnm::tab::{RnmTab, OPEN_BASE};
use texalith::rnm::{worker as rnm_worker, Kind};
use texalith::tab::Tab;
use texalith::cube::open::Opened;
use texalith::cube::tab::{CubeTab, OPEN_SOURCE};
use texalith::cube::view::FACE_LABELS;
use texalith::cube::{worker as cube_worker, SourceType};

struct Rig {
    cmds: Receiver<Cmd>,
    replies: Sender<Reply>,
    /// Stands in for the generation thread.
    outputs: Sender<worker::Reply>,
    _jobs: Receiver<worker::Job>,
    /// Stands in for the RNM thread.
    rnm_outputs: Sender<Arc<rnm_worker::Output>>,
    _rnm_jobs: Receiver<rnm_worker::Job>,
    /// Stands in for the cube thread.
    cube_outputs: Sender<Arc<cube_worker::Output>>,
    _cube_jobs: Receiver<cube_worker::Job>,
}

const LONG: &str = "CommunityShaders_AIO-2026-10-03T06-46Z and a very long build name that cannot possibly fit the panel";

fn rig<'a>(size: (f32, f32)) -> (Harness<'a, App>, Rig) {
    let (cmd_tx, cmd_rx) = channel();
    let (reply_tx, reply_rx) = channel();
    let (job_tx, job_rx) = channel();
    let (out_tx, out_rx) = channel();
    let (rnm_job_tx, rnm_job_rx) = channel();
    let (rnm_out_tx, rnm_out_rx) = channel();
    let (cube_job_tx, cube_job_rx) = channel();
    let (cube_out_tx, cube_out_rx) = channel();
    let app = App::new(
        cmd_tx,
        reply_rx,
        LONG.into(),
        Tab::new(job_tx, out_rx),
        RnmTab::new(rnm_job_tx, rnm_out_rx),
        CubeTab::new(cube_job_tx, cube_out_rx),
    );
    let h = Harness::builder()
        .with_size(egui::vec2(size.0, size.1))
        .build_ui_state(|ui, app: &mut App| app.draw(ui), app);
    (
        h,
        Rig {
            cmds: cmd_rx,
            replies: reply_tx,
            outputs: out_tx,
            _jobs: job_rx,
            rnm_outputs: rnm_out_tx,
            _rnm_jobs: rnm_job_rx,
            cube_outputs: cube_out_tx,
            _cube_jobs: cube_job_rx,
        },
    )
}

/// Sends a 4-texel cube output as the reply to the Cubemap tab's newest job.
fn deliver_cube(h: &mut Harness<'_, App>, rig: &Rig, id: u64) {
    h.state_mut().cube.jobs_sent = id;
    let faces = std::array::from_fn(|k| engine::image::Rgba32F::new(4, 4, [0.1 * k as f32, 0.5, 0.3, 1.0]));
    let o = cube_worker::Output { id, n: 4, cube: engine::cube::Cube { size: 4, faces } };
    rig.cube_outputs.send(Arc::new(o)).unwrap();
    h.run_steps(2);
}

/// The Cubemap tab with a long-named panorama, saved into its own folder.
fn cube_filled(h: &mut Harness<'_, App>) {
    let path = PathBuf::from("C:/textures/a rather long folder name/a_very_long_panorama_name_for_a_chrome_e.png");
    let o = Opened {
        sources: vec![Source { path: path.clone(), sha256: "00".into() }],
        imgs: vec![Arc::new(engine::image::Rgba32F::new(8, 4, [0.5; 4]))],
        detected: Some(SourceType::Panorama),
    };
    let t = &mut h.state_mut().cube;
    t.folder = path.parent().map(PathBuf::from);
    t.settings.source = Some(SourceType::Panorama);
    t.opened = Some(Arc::new(o));
}

/// Sends a 4x4 RNM output whose red is `v` as the reply to the RNM tab's newest job.
fn deliver_rnm(h: &mut Harness<'_, App>, rig: &Rig, id: u64, v: f32) {
    h.state_mut().rnm.jobs_sent = id;
    let o = rnm_worker::Output { id, size: (4, 4), px: vec![[v, 0.5, 1.0, 1.0]; 16] };
    rig.rnm_outputs.send(Arc::new(o)).unwrap();
    h.run_steps(2);
}

fn tab_button(h: &mut Harness<'_, App>, name: &str) {
    h.get_by_role_and_label(Role::Button, name).click();
    h.run_steps(2);
}

/// The SetContent map sets and SwapMap names sent since the last call.
fn pushes(cmds: &Receiver<Cmd>) -> (Vec<HashSet<&'static str>>, Vec<&'static str>) {
    let (mut set, mut swap) = (vec![], vec![]);
    for c in cmds.try_iter() {
        match c {
            Cmd::SetContent(c) => set.push(c.maps.keys().copied().collect()),
            Cmd::SwapMap(k, _) => swap.push(k),
            _ => {}
        }
    }
    (set, swap)
}

/// The RNM tab with a long-named base, a Bent layer with a mask and an AO map, and a Bump layer, in the base's
/// own folder.
fn rnm_filled(h: &mut Harness<'_, App>) {
    let loaded = |name: &str| Loaded {
        source: Source { path: PathBuf::from(format!("C:/textures/a rather long folder name/{name}")), sha256: "00".into() },
        img: Arc::new(engine::image::Rgba32F::new(4, 4, [0.5, 0.5, 1.0, 1.0])),
    };
    let base = loaded("a_very_long_texture_set_name_for_a_rock_wall_n.png");
    let ctx = h.ctx.clone();
    let t = &mut h.state_mut().rnm;
    t.folder = base.source.path.parent().map(PathBuf::from);
    t.base = Some((base.source.clone(), Arc::new(decode_base(&base.img))));
    t.add_layer(Kind::Bent, &ctx);
    let l = t.layers.last_mut().unwrap();
    l.file = Some(loaded("a_very_long_texture_set_name_for_a_rock_wall_bent.png"));
    l.mask = Some(loaded("a_very_long_texture_set_name_for_a_rock_wall_bent_mask.png"));
    l.ao = Some(loaded("a_very_long_texture_set_name_for_a_rock_wall_ao.png"));
    t.add_layer(Kind::Bump, &ctx);
    t.layers.last_mut().unwrap().file = Some(loaded("a_very_long_texture_set_name_for_a_rock_wall_bump.png"));
}

fn gray(v: f32) -> Gray {
    Gray { width: 4, height: 4, px: vec![v; 16] }
}

/// Hand-built outputs with parallax and AO at 4x4; `ao` sets the AO map's value.
fn outputs(id: u64, ao: f32) -> Outputs {
    Outputs {
        id,
        draft: false,
        size: (4, 4),
        report: MasterReport::default(),
        match_depth: 0.4,
        height: (gray(0.5), 12.4),
        parallax: Some((gray(0.5), ParallaxReport { times_normal: 9.6, shader_depth_pct: 1.0, levels_used: 61, clipped_share: 0.0 })),
        normal: None,
        preview_normal: vec![[0.5, 0.5, 1.0, 1.0]; 16],
        ao: Some(gray(ao)),
        cavity: None,
        curvature: None,
        compare: None,
    }
}

/// Sends `o` as the reply to the newest job.
fn deliver(h: &mut Harness<'_, App>, rig: &Rig, o: Outputs) {
    h.state_mut().tab.jobs_sent = o.id;
    rig.outputs.send(worker::Reply::Done(Box::new(o))).unwrap();
    h.run_steps(2);
}

fn frame(view: View) -> Reply {
    Reply::Frame(Frame {
        image: egui::ColorImage::filled([1, 1], egui::Color32::RED),
        view,
        asked: Instant::now(),
        render_ms: 1.0,
    })
}

fn views(cmds: &Receiver<Cmd>) -> Vec<View> {
    cmds.try_iter()
        .filter_map(|c| match c {
            Cmd::Render(v) => Some(v),
            _ => None,
        })
        .collect()
}

fn frame_centre(h: &Harness<'_, App>) -> egui::Pos2 {
    h.get_by_label(FRAME_LABEL).rect().center()
}

#[test]
fn dragging_orbits_and_asks_for_the_new_view_once_the_last_frame_is_back() {
    let (mut h, rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    let first = views(&rig.cmds);
    assert_eq!(first.len(), 1, "one frame asked for at start");

    let c = frame_centre(&h);
    h.hover_at(c);
    h.step();
    h.drag_at(c);
    h.step();
    h.hover_at(c + egui::vec2(40.0, 20.0));
    h.run_steps(2);
    assert!((h.state().yaw - (AUTO_YAW - 0.4)).abs() < 0.01, "yaw {}", h.state().yaw);
    assert!(views(&rig.cmds).is_empty(), "no second request while one is in flight");

    rig.replies.send(frame(first[0])).unwrap();
    h.run_steps(2);
    let next = views(&rig.cmds);
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].yaw, h.state().yaw);
    assert_eq!(h.state().shown, 1);
    h.drop_at(c + egui::vec2(40.0, 20.0));
}

#[test]
fn a_moving_camera_renders_fewer_pixels_and_a_resting_one_goes_back_to_full_size() {
    let (mut h, rig) = rig((3000.0, 2000.0));
    h.run_steps(2);
    let full = views(&rig.cmds)[0];
    assert!((full.w * full.h) as f32 > 2.0 * MOVING_PIXELS, "full size at rest: {full:?}");
    rig.replies.send(frame(full)).unwrap();
    h.step();

    let c = frame_centre(&h);
    h.hover_at(c);
    h.step();
    h.drag_at(c);
    h.step();
    h.hover_at(c + egui::vec2(40.0, 0.0));
    h.step();
    let moving = *views(&rig.cmds).last().expect("a view while dragging");
    assert!(moving.yaw != full.yaw);
    let pixels = (moving.w * moving.h) as f32;
    assert!(pixels <= MOVING_PIXELS * 1.01, "{moving:?}");
    let aspect = moving.w as f32 / moving.h as f32;
    assert!((aspect - full.w as f32 / full.h as f32).abs() < 0.01, "keeps the shape: {moving:?}");

    // Stop moving: the next request after the rest period is sharp, at the same camera.
    h.drop_at(c + egui::vec2(40.0, 0.0));
    rig.replies.send(frame(moving)).unwrap();
    let rest = (0..30).find_map(|_| {
        h.step();
        views(&rig.cmds).into_iter().find(|v| v.w == full.w)
    });
    let rest = rest.expect("a full size frame after the camera rests");
    assert_eq!((rest.w, rest.h, rest.yaw), (full.w, full.h, h.state().yaw));
}

#[test]
fn an_idle_window_asks_for_nothing() {
    let (mut h, rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    let first = views(&rig.cmds);
    rig.replies.send(frame(first[0])).unwrap();
    h.run_steps(5);
    assert!(views(&rig.cmds).is_empty());
    assert_eq!(h.state().asked, 1);
}

#[test]
fn scrolling_zooms_only_over_the_frame() {
    let (mut h, _rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    let scroll = egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 200.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    };
    h.hover_at(egui::pos2(1270.0, 700.0));
    h.step();
    h.event(scroll.clone());
    h.run_steps(30);
    assert_eq!(h.state().zoom, 1.0, "over the panel");
    h.hover_at(frame_centre(&h));
    h.step();
    h.event(scroll);
    h.run_steps(30);
    assert!(h.state().zoom < 1.0, "zoom {}", h.state().zoom);
}

#[test]
fn picking_a_form_sends_it() {
    let (mut h, rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    h.get_by_role_and_label(Role::ComboBox, "Form").click();
    h.run_steps(3);
    h.get_by_label("Plane").click();
    h.run_steps(3);
    assert_eq!(h.state().form, Builtin::Plane);
    assert!(rig.cmds.try_iter().any(|c| matches!(c, Cmd::SetForm(Builtin::Plane))));
}

#[test]
fn errors_are_shown_and_a_dead_thread_is_reported() {
    let (mut h, rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    assert!(h.query_by_label("Compiling shaders…").is_some(), "loading state before the first frame");
    rig.replies.send(Reply::Failed("no fxc".into())).unwrap();
    h.run_steps(2);
    assert!(h.query_by_label("The preview cannot draw this: no fxc").is_some());
    let Rig { cmds, replies, .. } = rig;
    drop(replies);
    drop(cmds);
    h.state_mut().error = None;
    h.run_steps(2);
    assert!(h.query_by_label("The preview cannot draw this: The preview stopped. Restart Texalith.").is_some());
}

#[test]
fn new_outputs_reach_the_preview() {
    let (mut h, rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    rig.cmds.try_iter().for_each(drop);
    deliver(&mut h, &rig, outputs(1, 0.5));
    let sent: Vec<Cmd> = rig.cmds.try_iter().collect();
    let contents: Vec<HashSet<&str>> = sent
        .iter()
        .filter_map(|c| match c {
            Cmd::SetContent(c) => Some(c.maps.keys().copied().collect()),
            _ => None,
        })
        .collect();
    assert_eq!(contents, vec![HashSet::from(["n", "p", "r"])]);
    assert!(!sent.iter().any(|c| matches!(c, Cmd::SwapMap(..))));

    deliver(&mut h, &rig, outputs(2, 0.2));
    let sent: Vec<Cmd> = rig.cmds.try_iter().collect();
    assert!(!sent.iter().any(|c| matches!(c, Cmd::SetContent(_))), "same slots: no reload");
    let swapped: Vec<&str> = sent
        .iter()
        .filter_map(|c| match c {
            Cmd::SwapMap(name, _) => Some(*name),
            _ => None,
        })
        .collect();
    assert_eq!(swapped, vec!["r"], "only the changed map");
}

#[test]
fn the_2d_view_shows_the_chosen_map_and_repeats_it() {
    let (mut h, rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    deliver(&mut h, &rig, outputs(1, 0.5));
    h.get_by_role_and_label(Role::Button, "2D").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::ComboBox, "Map").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "AO").click();
    h.run_steps(2);
    let images = |h: &Harness<'_, App>| {
        h.query_all_by_label("AO").filter(|n| n.accesskit_node().role() == Role::Image).count()
    };
    assert_eq!(images(&h), 1);
    h.get_by_role_and_label(Role::CheckBox, "Repeat 2x2").click();
    h.run_steps(2);
    assert_eq!(images(&h), 4);
}

#[test]
fn the_three_columns_fit_at_the_checked_widths() {
    for (width, tab) in [1100.0, 1280.0, 1600.0].into_iter().flat_map(|w| [(w, "Material"), (w, "RNM"), (w, "Cubemap")]) {
        let (mut h, rig) = rig((width, 720.0));
        h.run_steps(2);
        if tab == "RNM" {
            tab_button(&mut h, "RNM");
            rnm_filled(&mut h);
        }
        if tab == "Cubemap" {
            tab_button(&mut h, "Cubemap");
            cube_filled(&mut h);
            deliver_cube(&mut h, &rig, 1);
        }
        h.run_steps(3);
        let rect = |id: &str| {
            egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new(id)).expect(id).outer_rect
        };
        let (left, right) = (rect("column"), rect("controls"));
        // The Cubemap tab has no 3D frame; its centre is what the two side panels leave.
        let centre = match tab {
            "Cubemap" => egui::Rect::from_x_y_ranges(left.max.x..=right.min.x, left.y_range()),
            _ => h.get_by_label(FRAME_LABEL).rect(),
        };
        assert!(left.max.x <= centre.min.x + 0.5, "left over the centre at {width}: {left:?} {centre:?}");
        assert!(centre.max.x <= right.min.x + 0.5, "centre under the controls at {width}: {centre:?} {right:?}");
        assert!(centre.width() >= 400.0, "centre {} wide at {width}", centre.width());
    }
}

#[test]
fn nothing_leaves_the_window_at_the_checked_widths() {
    for (width, tab) in [1100.0, 1280.0, 1600.0].into_iter().flat_map(|w| [(w, "Material"), (w, "RNM"), (w, "Cubemap")]) {
        let (mut h, rig) = rig((width, 720.0));
        rig.replies
            .send(Reply::Ready {
                adapter: "An adapter with an unreasonably long name that goes on and on and on and on".into(),
                warp: true,
                fingerprint: "0123456789abcdef".into(),
            })
            .unwrap();
        h.run_steps(2);
        if tab == "RNM" {
            tab_button(&mut h, "RNM");
            rnm_filled(&mut h);
            deliver_rnm(&mut h, &rig, 1, 0.5);
        }
        if tab == "Cubemap" {
            tab_button(&mut h, "Cubemap");
            cube_filled(&mut h);
            deliver_cube(&mut h, &rig, 1);
        }
        h.run_steps(3);
        let panel_left = width - 280.0;
        // The Cubemap tab has no 3D view, so no 3D options either.
        let options = [(Role::Button, "Reset view"), (Role::Slider, "Parallax scale")];
        for (role, label) in options.into_iter().filter(|_| tab != "Cubemap") {
            let r = h.get_by_role_and_label(role, label).rect();
            assert!(r.min.x >= panel_left && r.max.x <= width, "{label} at {width}: {r:?}");
        }
        let left = egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("column")).unwrap().outer_rect;
        let open_label = match tab {
            "RNM" => OPEN_BASE,
            "Cubemap" => OPEN_SOURCE,
            _ => texalith::tab::OPEN_LABEL,
        };
        let open = h.get_by_role_and_label(Role::Button, open_label).rect();
        assert!(open.min.x >= 0.0 && open.max.x <= left.max.x, "open at {width}: {open:?} {left:?}");
        let widgets = h.ctx.viewport(|v| v.this_pass.widgets.clone());
        for (_, layer) in widgets.layers() {
            for w in layer {
                assert!(w.interact_rect.max.x <= width + 0.5, "a widget past the window at {width}: {:?}", w.interact_rect);
            }
        }
    }
}

#[test]
fn the_cubemap_tab_shows_the_labelled_cross() {
    let (mut h, rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    tab_button(&mut h, "Cubemap");
    cube_filled(&mut h);
    deliver_cube(&mut h, &rig, 1);
    for label in FACE_LABELS {
        h.get_by_label(label);
    }
    h.get_by_role_and_label(Role::Button, "3D");
}

#[test]
fn the_cubemap_tab_draws_its_cube_on_a_ball_in_3d() {
    let (mut h, rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    tab_button(&mut h, "Cubemap");
    cube_filled(&mut h);
    rig.cmds.try_iter().for_each(drop);
    deliver_cube(&mut h, &rig, 1);
    // The cube reaches the preview in either view, so the ball is ready when 3D is picked.
    assert_eq!(pushes(&rig.cmds).0, [HashSet::from(["e"])]);
    // The labelled cross comes first: the help texts point at it.
    h.get_by_label(FACE_LABELS[0]);
    h.get_by_role_and_label(Role::Button, "3D").click();
    h.run_steps(2);
    h.get_by_label("Vanilla environment map, not True PBR");
    h.get_by_label(FRAME_LABEL);
    assert!(h.query_by_label(FACE_LABELS[0]).is_none());
    h.get_by_role_and_label(Role::Button, "Reset view");
    assert!(h.query_by_role_and_label(Role::Slider, "Parallax scale").is_none());
    // A changed cube is swapped in.
    let faces = std::array::from_fn(|_| engine::image::Rgba32F::new(4, 4, [0.9, 0.1, 0.1, 1.0]));
    h.state_mut().cube.jobs_sent = 2;
    rig.cube_outputs.send(Arc::new(cube_worker::Output { id: 2, n: 4, cube: engine::cube::Cube { size: 4, faces } })).unwrap();
    h.run_steps(2);
    assert_eq!(pushes(&rig.cmds).1, ["e"]);
    // The Material tab keeps its own choice of view.
    tab_button(&mut h, "Material");
    h.get_by_label(FRAME_LABEL);
    assert!(h.query_by_label("Vanilla environment map, not True PBR").is_none());
}

#[test]
fn the_cubemap_centre_says_what_to_do_when_empty() {
    let (mut h, _rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    tab_button(&mut h, "Cubemap");
    h.get_by_label("Open a source to see its faces here.");
}

#[test]
fn the_cubemap_centre_asks_for_a_type_when_none_is_picked() {
    let (mut h, _rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    tab_button(&mut h, "Cubemap");
    cube_filled(&mut h);
    h.state_mut().cube.settings.source = None;
    h.run_steps(2);
    h.get_by_label("Pick a Source type to see its faces here.");
    assert!(h.query_by_label("Open a source to see its faces here.").is_none());
}

#[test]
fn the_rnm_column_keeps_its_own_scroll() {
    let (mut h, _rig) = rig((1280.0, 400.0));
    h.run_steps(2);
    tab_button(&mut h, "RNM");
    rnm_filled(&mut h);
    h.run_steps(3);
    let top = h.get_by_role_and_label(Role::Button, OPEN_BASE).rect().min.y;
    // Above the Save panel at the column's foot.
    h.hover_at(egui::pos2(100.0, 150.0));
    h.step();
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -150.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(30);
    let scrolled = h.get_by_role_and_label(Role::Button, OPEN_BASE).rect().min.y;
    assert!(scrolled < top - 50.0, "the column scrolled: {top} -> {scrolled}");
    tab_button(&mut h, "Material");
    h.run_steps(3);
    tab_button(&mut h, "RNM");
    h.run_steps(3);
    let back = h.get_by_role_and_label(Role::Button, OPEN_BASE).rect().min.y;
    assert!((back - scrolled).abs() < 1.0, "scroll kept: {scrolled} -> {back}");
}

#[test]
fn the_tab_bar_switches_columns() {
    let (mut h, _rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    tab_button(&mut h, "RNM");
    h.get_by_role_and_label(Role::Button, OPEN_BASE);
    assert!(h.query_by_role_and_label(Role::Button, texalith::tab::OPEN_LABEL).is_none());
    tab_button(&mut h, "Material");
    h.get_by_role_and_label(Role::Button, texalith::tab::OPEN_LABEL);
    assert!(h.query_by_role_and_label(Role::Button, OPEN_BASE).is_none());
}

#[test]
fn rnm_outputs_reach_the_preview_as_n_only() {
    let (mut h, rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    tab_button(&mut h, "RNM");
    pushes(&rig.cmds);
    deliver_rnm(&mut h, &rig, 1, 0.5);
    let (set, swap) = pushes(&rig.cmds);
    assert_eq!(set, vec![HashSet::from(["n"])]);
    assert!(swap.is_empty());
    deliver_rnm(&mut h, &rig, 2, 0.7);
    let (set, swap) = pushes(&rig.cmds);
    assert!(set.is_empty(), "same slots: no reload");
    assert_eq!(swap, vec!["n"]);
}

#[test]
fn switching_tabs_reloads_the_preview() {
    let (mut h, rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    deliver(&mut h, &rig, outputs(1, 0.5));
    deliver_rnm(&mut h, &rig, 1, 0.5);
    let (set, _) = pushes(&rig.cmds);
    assert_eq!(set, vec![HashSet::from(["n", "p", "r"])], "RNM outputs wait while From normal is shown");
    tab_button(&mut h, "RNM");
    assert_eq!(pushes(&rig.cmds).0, vec![HashSet::from(["n"])]);
    tab_button(&mut h, "Material");
    assert_eq!(pushes(&rig.cmds).0, vec![HashSet::from(["n", "p", "r"])]);
}

#[test]
fn the_2d_view_shows_the_rnm_normal() {
    let (mut h, rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    tab_button(&mut h, "RNM");
    h.get_by_role_and_label(Role::Button, "2D").click();
    h.run_steps(2);
    assert!(h.query_by_label("Open a base normal to see the blend here.").is_some());
    deliver_rnm(&mut h, &rig, 1, 0.5);
    assert_eq!(h.query_all_by_label("RNM normal").filter(|n| n.accesskit_node().role() == Role::Image).count(), 1);
}

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

#[test]
fn a_white_parallax_map_is_previewed_as_white() {
    let (mut h, rig) = rig((1280.0, 800.0));
    h.run_steps(2);
    let d = std::env::temp_dir().join(format!("texalith-ui-white-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let file = d.join("rock_n.png");
    image::RgbImage::from_fn(64, 64, |x, _| image::Rgb([(120 + x) as u8, 128, 230])).save(&file).unwrap();
    h.state_mut().tab.accept(texalith::from_normal::open::open(&file).unwrap());
    h.state_mut().tab.settings.as_mut().unwrap().parallax.flat = FlatLevel::White;
    h.run_steps(2);
    rig.cmds.try_iter().for_each(drop);
    deliver(&mut h, &rig, outputs(1, 0.5));
    let white: Vec<bool> = rig
        .cmds
        .try_iter()
        .filter_map(|c| match c {
            Cmd::SetContent(c) => Some(c.white),
            _ => None,
        })
        .collect();
    assert_eq!(white, [true]);
    // Back to Mid-grey with the same slots: the preview is told, and the app sends no new content.
    h.state_mut().tab.settings.as_mut().unwrap().parallax.flat = FlatLevel::MidGrey;
    deliver(&mut h, &rig, outputs(2, 0.2));
    let sent: Vec<Cmd> = rig.cmds.try_iter().collect();
    assert!(sent.iter().any(|c| matches!(c, Cmd::SetWhite(false))));
    assert!(!sent.iter().any(|c| matches!(c, Cmd::SetContent(_))));
    let _ = std::fs::remove_dir_all(&d);
}
