//! Saving the From normal outputs: the plan, the review that decides what the dialog shows, and the write.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;
use texalith::export::Verdict;
use texalith::from_normal::files;
use texalith::from_normal::open::{open, Opened};
use texalith::from_normal::settings::{Settings, Ticks};
use texalith::from_normal::worker::{compute, Cache, Job, Outputs};
use texalith::marker::{self, MapKind};
use texalith::export::Planned;
use texalith::save::{self, plan, review};

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("texalith-save-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// A 64² plain 8-bit normal PNG with a ripple, so the outputs are not flat.
fn write_normal(p: &Path) {
    let img = image::RgbImage::from_fn(64, 64, |x, y| {
        let r = (128.0 + 40.0 * (x as f32 * 0.3).sin()) as u8;
        let g = (128.0 + 40.0 * (y as f32 * 0.2).cos()) as u8;
        image::Rgb([r, g, 230])
    });
    img.save(p).unwrap();
}

fn opened(d: &Path) -> Opened {
    let src = d.join("rock_n.png");
    write_normal(&src);
    open(&src).unwrap()
}

fn outputs(o: &Opened, s: &Settings) -> Outputs {
    let job = Job {
        id: 1,
        img: o.img.clone(),
        kind: o.kind.clone(),
        settings: s.clone(),
        draft: false,
        compare: false,
        cancel: Arc::new(AtomicBool::new(false)),
        progress: Arc::new(AtomicU32::new(0)),
    };
    compute(&mut Cache::default(), &job).unwrap().unwrap()
}

/// An 8-bit grey PNG, optionally carrying `text` as a `Texalith` iTXt chunk.
fn png_with(px: &[u8], text: Option<String>) -> Vec<u8> {
    let mut out = vec![];
    let mut e = png::Encoder::new(&mut out, px.len() as u32, 1);
    e.set_color(png::ColorType::Grayscale);
    e.set_depth(png::BitDepth::Eight);
    if let Some(t) = text {
        e.add_itxt_chunk(marker::KEYWORD.into(), t).unwrap();
    }
    e.write_header().unwrap().write_image_data(px).unwrap();
    out
}

/// The defaults with Height data ticked too, so a plan holds three maps.
fn with_height(o: &Opened) -> Settings {
    let mut s = Settings::for_source(&o.kind, (64, 64));
    s.ticks.height = true;
    s
}

#[test]
fn the_plan_names_and_marks_each_ticked_output() {
    let d = temp("plan");
    let o = opened(&d);
    let s = with_height(&o);
    let out = outputs(&o, &s);
    let p = plan(&o, &s, &out, &d);
    let paths: Vec<_> = p.iter().map(|(_, pl)| pl.path.clone()).collect();
    assert_eq!(paths, vec![d.join("rock_height.png"), d.join("rock_parallax.png"), d.join("rock_ao.png")]);
    for (kind, pl) in &p {
        let m = marker::read(&pl.bytes).unwrap();
        assert_eq!(m.kind, *kind);
        assert_eq!(m.settings, s.json(*kind, &out.report, out.size.0));
    }
    assert_eq!(marker::read(&p[0].1.bytes).unwrap().span_texels, Some(out.height.1));
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn an_unticked_height_still_ties_the_parallax_to_its_master() {
    let d = temp("tie");
    let o = opened(&d);
    let mut s = Settings::for_source(&o.kind, (64, 64));
    s.ticks = Ticks { height: false, parallax: true, normal: false, ao: false, curvature: false, cavity: false };
    let out = outputs(&o, &s);
    let p = plan(&o, &s, &out, &d);
    assert_eq!(p.len(), 1);
    let h = files::gray_png(
        &out.height.0,
        MapKind::HeightData,
        &o.source,
        s.json(MapKind::HeightData, &out.report, out.size.0),
        Some(out.height.1),
        None,
    );
    let master = marker::read(&h).unwrap().pixels_sha256;
    assert_eq!(marker::read(&p[0].1.bytes).unwrap().master_sha256, Some(master));
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn only_foreign_unreadable_or_source_rows_open_the_dialog() {
    let d = temp("dialog");
    let o = opened(&d);
    let s = with_height(&o);
    let out = outputs(&o, &s);
    assert!(!review(plan(&o, &s, &out, &d), std::slice::from_ref(&o.path)).needs_dialog());

    fs::write(d.join("rock_ao.png"), png_with(&[10, 20], None)).unwrap();
    let r = review(plan(&o, &s, &out, &d), std::slice::from_ref(&o.path));
    assert!(r.needs_dialog() && r.backup);
    let ao = r.rows.iter().find(|row| row.kind == MapKind::Ao).unwrap();
    assert!(matches!(ao.verdict, Verdict::Foreign) && !ao.overwrite);
    assert_eq!(ao.reason.as_deref(), Some("Not made by Texalith, or its marker was removed"));

    let ours = &plan(&o, &s, &out, &d)[0].1.bytes;
    let edited = png_with(&[7, 9], Some(serde_json::to_string(&marker::read(ours).unwrap()).unwrap()));
    fs::write(d.join("rock_height.png"), edited).unwrap();
    let r = review(plan(&o, &s, &out, &d), std::slice::from_ref(&o.path));
    let h = r.rows.iter().find(|row| row.kind == MapKind::HeightData).unwrap();
    assert_eq!(h.reason.as_deref(), Some("Made by Texalith, then changed"));
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn a_kept_row_is_not_written_and_an_overwritten_one_is_backed_up() {
    let d = temp("keep");
    let o = opened(&d);
    let s = with_height(&o);
    let out = outputs(&o, &s);
    let old = png_with(&[10, 20], None);
    fs::write(d.join("rock_ao.png"), &old).unwrap();

    let r = review(plan(&o, &s, &out, &d), std::slice::from_ref(&o.path));
    r.write().unwrap();
    assert_eq!(fs::read(d.join("rock_ao.png")).unwrap(), old);
    assert!(d.join("rock_height.png").exists());

    let mut r = review(plan(&o, &s, &out, &d), std::slice::from_ref(&o.path));
    r.rows.iter_mut().filter(|row| row.kind == MapKind::Ao).for_each(|row| row.overwrite = true);
    r.write().unwrap();
    assert_ne!(fs::read(d.join("rock_ao.png")).unwrap(), old);
    assert_eq!(fs::read(d.join("_sdc").join("texalith-backup").join("rock_ao.png")).unwrap(), old);
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn a_failed_write_reports_the_path_and_leaves_no_partial_set() {
    let d = temp("fail");
    let o = opened(&d);
    let s = with_height(&o);
    let out = outputs(&o, &s);
    let ao = d.join("rock_ao.png");
    fs::write(&ao, png_with(&[10, 20], None)).unwrap();
    let mut perms = fs::metadata(&ao).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&ao, perms.clone()).unwrap();

    let mut r = review(plan(&o, &s, &out, &d), std::slice::from_ref(&o.path));
    r.rows.iter_mut().filter(|row| row.kind == MapKind::Ao).for_each(|row| row.overwrite = true);
    let e = r.write().unwrap_err();
    assert!(e.contains("rock_ao.png"), "{e}");
    assert!(!d.join("rock_height.png").exists() && !d.join("rock_parallax.png").exists());

    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(&ao, perms).unwrap();
    fs::remove_dir_all(&d).unwrap();
}

/// Polls until the save has ended.
fn finish(saving: &mut Option<save::Saving>, review: &mut Option<save::Review>, status: &mut Option<Result<String, String>>) {
    let start = std::time::Instant::now();
    while saving.is_some() {
        assert!(start.elapsed().as_secs() < 60, "the save never finished");
        std::thread::sleep(std::time::Duration::from_millis(1));
        save::poll(saving, review, status);
    }
}

fn say() -> save::Say {
    Box::new(|w, k| format!("{w} written, {k} kept"))
}

#[test]
fn a_save_runs_on_its_own_thread_and_reports_when_polled() {
    let d = temp("thread");
    let path = d.join("x_ao.png");
    let planned = Planned { path: path.clone(), bytes: png_with(&[1, 2], None) };
    let (gate, wait) = std::sync::mpsc::channel::<()>();
    let ctx = eframe::egui::Context::default();
    let mut saving = Some(save::start(&ctx, vec![], say(), move || {
        wait.recv().unwrap();
        vec![(MapKind::Ao, planned)]
    }));
    let (mut review, mut status) = (None, None);
    save::poll(&mut saving, &mut review, &mut status);
    assert!(saving.is_some() && status.is_none() && !path.exists(), "the caller does not wait for the encode");
    gate.send(()).unwrap();
    finish(&mut saving, &mut review, &mut status);
    assert_eq!(status, Some(Ok("1 written, 0 kept".into())));
    assert!(review.is_none() && path.is_file());
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn a_save_that_needs_a_choice_comes_back_as_a_review_with_nothing_written() {
    let d = temp("ask");
    let old = png_with(&[10, 20], None);
    fs::write(d.join("x_ao.png"), &old).unwrap();
    let plan = vec![
        (MapKind::HeightData, Planned { path: d.join("x_height.png"), bytes: png_with(&[1, 2], None) }),
        (MapKind::Ao, Planned { path: d.join("x_ao.png"), bytes: png_with(&[3, 4], None) }),
    ];
    let ctx = eframe::egui::Context::default();
    let mut saving = Some(save::start(&ctx, vec![], say(), move || plan));
    let (mut review, mut status) = (None, None);
    finish(&mut saving, &mut review, &mut status);
    assert!(status.is_none() && !d.join("x_height.png").exists());
    let mut r = review.take().expect("the foreign file needs a choice");
    assert!(matches!(r.rows[1].verdict, Verdict::Foreign));
    // The dialog's Save, with Overwrite chosen.
    r.rows[1].overwrite = true;
    saving = Some(save::start_write(&ctx, r, vec![]));
    finish(&mut saving, &mut review, &mut status);
    assert_eq!(status, Some(Ok("2 written, 0 kept".into())));
    assert_ne!(fs::read(d.join("x_ao.png")).unwrap(), old);
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn closing_waits_for_a_running_save() {
    let (gate, held) = std::sync::mpsc::channel::<()>();
    let _saving = save::start(&eframe::egui::Context::default(), vec![], say(), move || {
        let _ = held.recv();
        vec![]
    });
    let done = Arc::new(AtomicBool::new(false));
    let flag = done.clone();
    // What main does after the window has closed.
    let waiter = std::thread::spawn(move || {
        save::wait();
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!done.load(std::sync::atomic::Ordering::SeqCst), "the process would have ended in the middle of a save");
    gate.send(()).unwrap();
    waiter.join().unwrap();
    assert!(done.load(std::sync::atomic::Ordering::SeqCst));
}
