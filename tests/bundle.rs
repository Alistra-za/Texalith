//! The pin, and the render thread's recovery from a lost device. Uses the GPU; no install needed.
use eframe::egui;
use render3d::cs_settings::preview_defines;
use render3d::Builtin;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;
use texalith::preview::{spawn, Cmd, Content, Frame, Reply, View};
use texalith::{bundle, testmaps};

#[test]
fn pin_matches_the_folder() {
    let dir = bundle::dir();
    let hash = render3d::locate::fingerprint(&bundle::shaders(&dir), None).unwrap().hash;
    assert_eq!(
        bundle::pin(&dir, "fingerprint").as_deref(),
        Some(hash.as_str()),
        "CS build changed: compare and decide (see assets/cs-shaders/PIN.toml)"
    );
}

#[test]
fn pinned_settings_give_the_pinned_defines() {
    let s = bundle::settings(&bundle::dir());
    assert!(s.notes.is_empty() && !s.parse_failed, "{:?}", s.notes);
    assert_eq!(
        preview_defines(&s, true),
        ["CS_HAIR", "CS_SKIN", "DYNAMIC_CUBEMAPS", "ENABLE_LL", "EXTENDED_MATERIALS", "EXTENDED_TRANSLUCENCY", "VANILLA_FRESNEL"]
    );
}

#[test]
fn licence_files_ship_with_the_bundle() {
    let dir = bundle::dir().join("licenses");
    for f in ["COPYING.txt", "EXCEPTIONS.md", "THIRD_PARTY.md", "LICENSE-Perapechka-MIT.txt", "LICENSE-SebH-MIT.txt"] {
        assert!(dir.join(f).is_file(), "{f} is missing");
    }
}

fn next_frame(rx: &Receiver<Reply>, recovered: &mut u32) -> Frame {
    loop {
        match rx.recv_timeout(Duration::from_secs(120)).expect("a reply") {
            Reply::Frame(f) => return f,
            Reply::Failed(e) => panic!("{e}"),
            Reply::Recovered { .. } => *recovered += 1,
            Reply::Ready { .. } => {}
        }
    }
}

fn start(tmp: &str) -> (Sender<Cmd>, Receiver<Reply>) {
    let dir = bundle::dir();
    let cache = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(tmp);
    std::fs::create_dir_all(&cache).unwrap();
    spawn(bundle::shaders(&dir), bundle::settings(&dir), cache, egui::Context::default())
}

// 3.14 is the yaw the frames were pinned at, not a rounded pi.
#[allow(clippy::approx_constant)]
const VIEW: View = View { yaw: 3.14, pitch: 0.25, zoom: 1.0, w: 320, h: 240 };

#[test]
fn a_lost_device_comes_back_with_the_same_picture_and_the_latest_maps() {
    let (tx, rx) = start("recovery");
    let mut recovered = 0;
    tx.send(Cmd::SetContent(Content { form: Builtin::Sphere, ..testmaps::content(64) })).unwrap();
    tx.send(Cmd::Render(VIEW)).unwrap();
    let before = next_frame(&rx, &mut recovered);

    tx.send(Cmd::LoseDevice).unwrap();
    let after = next_frame(&rx, &mut recovered);
    assert_eq!(recovered, 1);
    assert_eq!(before.image.pixels, after.image.pixels, "recovery changed the picture");

    // A map swapped before the loss must survive it: the reload uses the kept bytes, not the original maps.
    let flat = testmaps::dds(1, &[[20, 200, 20, 255]], true);
    tx.send(Cmd::SwapMap("a", flat)).unwrap();
    let swapped = next_frame(&rx, &mut recovered);
    assert_ne!(before.image.pixels, swapped.image.pixels, "the swap must show");
    tx.send(Cmd::LoseDevice).unwrap();
    let again = next_frame(&rx, &mut recovered);
    assert_eq!(recovered, 2);
    assert_eq!(swapped.image.pixels, again.image.pixels, "recovery lost the swapped map");
}

/// The real window hung on "Compiling shaders…": its first request reached the thread before the maps did.
#[test]
fn a_view_asked_for_before_the_content_is_drawn_when_the_content_arrives() {
    let (tx, rx) = start("early-view");
    let mut recovered = 0;
    tx.send(Cmd::Render(VIEW)).unwrap();
    // Long enough for the thread to start and take the request alone.
    std::thread::sleep(Duration::from_secs(2));
    tx.send(Cmd::SetContent(Content { form: Builtin::Plane, ..testmaps::content(64) })).unwrap();
    assert_eq!(next_frame(&rx, &mut recovered).view, VIEW);
}

/// A frame after a failure would clear the message and show the old picture as if the edit had worked.
#[test]
fn a_failed_edit_sends_its_error_and_no_frame() {
    let (tx, rx) = start("failed-edit");
    let mut recovered = 0;
    tx.send(Cmd::SetContent(Content { form: Builtin::Plane, ..testmaps::content(64) })).unwrap();
    tx.send(Cmd::Render(VIEW)).unwrap();
    next_frame(&rx, &mut recovered);

    tx.send(Cmd::SwapMap("a", b"not a dds file".to_vec())).unwrap();
    let reply = rx.recv_timeout(Duration::from_secs(120)).expect("a reply");
    assert!(matches!(reply, Reply::Failed(_)), "the bad map must report an error");
    assert!(rx.recv_timeout(Duration::from_secs(2)).is_err(), "nothing may follow the error");
}

/// The Cubemap tab's preview DDS of a cube whose +Z face is `up` and −Z face is `down`, grey elsewhere.
fn cube_dds(up: [f32; 4], down: [f32; 4]) -> Vec<u8> {
    let colour = |i| match i {
        4 => up,
        5 => down,
        _ => [0.2, 0.2, 0.2, 1.0],
    };
    let faces = std::array::from_fn(|i| engine::image::Rgba32F::new(16, 16, colour(i)));
    texalith::cube::files::preview_dds(&engine::cube::Cube { size: 16, faces })
}

/// World up is cube +Z (Tessalith Note DS13): the ball's top reflects the +Z face and its bottom the −Z face.
#[test]
fn the_cube_ball_reflects_up_at_its_top() {
    const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
    const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
    let (tx, rx) = start("cube-ball");
    let mut recovered = 0;
    let maps = std::collections::HashMap::from([("e", cube_dds(RED, BLUE))]);
    tx.send(Cmd::SetContent(Content { form: Builtin::Sphere, displacement: 1.0, maps, white: false })).unwrap();
    tx.send(Cmd::Render(View { pitch: 0.0, w: 256, h: 256, ..VIEW })).unwrap();
    // Seen from level, the ball covers about rows 8..247; a quarter of the way in, the mirror direction is
    // straight up or down.
    let ends = |f: &Frame| (f.image.pixels[43 * 256 + 128], f.image.pixels[213 * 256 + 128]);
    let (top, bottom) = ends(&next_frame(&rx, &mut recovered));
    println!("red up: top {top:?} bottom {bottom:?}");
    assert!(top.r() > top.b() && bottom.b() > bottom.r(), "top {top:?} bottom {bottom:?}");
    // A new cube on the same slot is swapped in, as the tab does for every change.
    tx.send(Cmd::SwapMap("e", cube_dds(BLUE, RED))).unwrap();
    let (top, bottom) = ends(&next_frame(&rx, &mut recovered));
    println!("blue up: top {top:?} bottom {bottom:?}");
    assert!(top.b() > top.r() && bottom.r() > bottom.b(), "top {top:?} bottom {bottom:?}");
}

#[test]
fn queued_views_collapse_to_the_newest() {
    let (tx, rx) = start("coalesce");
    let mut recovered = 0;
    tx.send(Cmd::SetContent(Content { form: Builtin::Plane, ..testmaps::content(64) })).unwrap();
    tx.send(Cmd::Render(VIEW)).unwrap();
    next_frame(&rx, &mut recovered);
    for i in 1..=50 {
        tx.send(Cmd::Render(View { yaw: i as f32 * 0.01, ..VIEW })).unwrap();
    }
    // The thread may take the burst in pieces, but the last frame must be the newest view and few frames are drawn.
    let mut frames = 0;
    let newest = loop {
        let f = next_frame(&rx, &mut recovered);
        frames += 1;
        if f.view.yaw == 50.0 * 0.01 {
            break f;
        }
    };
    assert!(frames <= 50);
    assert_eq!(newest.image.size, [320, 240]);
}
