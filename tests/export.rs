//! The overwrite check and the all-or-nothing write, on real files in a temp folder.
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use texalith::export::{check, write, write_or_ask, Planned, Unasked, Verdict};
use texalith::from_normal::files::{self, Source};
use texalith::from_normal::maps::Gray;
use texalith::marker::{self, MapKind};

/// A fresh folder for one test; the test removes it at the end.
fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("texalith-export-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// A source path that is none of the planned outputs.
fn none() -> PathBuf {
    std::env::temp_dir().join("texalith-export-no-such-source.png")
}

fn src() -> Source {
    Source { path: "C:/m/rock_n.png".into(), sha256: "00".into() }
}

fn gray() -> Gray {
    Gray { width: 2, height: 1, px: vec![0.0, 1.0] }
}

fn gray_other() -> Gray {
    Gray { width: 2, height: 1, px: vec![1.0, 0.0] }
}

fn marked() -> Vec<u8> {
    files::gray_png(&gray(), MapKind::Ao, &src(), json!({}), None, None)
}

fn marked_other() -> Vec<u8> {
    files::gray_png(&gray_other(), MapKind::Ao, &src(), json!({}), None, None)
}

/// An 8-bit grey PNG with no text chunk, optionally carrying `text` as a `Texalith` iTXt chunk.
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

fn plain_png() -> Vec<u8> {
    png_with(&[10, 20], None)
}

/// Other pixels under `ours`'s marker, as an image editor that keeps text chunks would save it.
fn edited_keeping_marker(ours: &[u8]) -> Vec<u8> {
    png_with(&[7, 9], Some(serde_json::to_string(&marker::read(ours).unwrap()).unwrap()))
}

#[test]
fn the_check_follows_the_marker_rule() {
    let d = temp("check");
    let ours = marked();
    let p = |name: &str| d.join(name);
    assert!(matches!(check(&Planned { path: p("a_ao.png"), bytes: ours.clone() }, &[none()]), Verdict::New));
    fs::write(p("a_ao.png"), &ours).unwrap();
    assert!(matches!(check(&Planned { path: p("a_ao.png"), bytes: ours.clone() }, &[none()]), Verdict::Same));
    let newer = marked_other();
    assert!(matches!(check(&Planned { path: p("a_ao.png"), bytes: newer.clone() }, &[none()]), Verdict::Ours));
    fs::write(p("a_ao.png"), edited_keeping_marker(&ours)).unwrap();
    assert!(matches!(check(&Planned { path: p("a_ao.png"), bytes: newer.clone() }, &[none()]), Verdict::Foreign));
    fs::write(p("a_ao.png"), plain_png()).unwrap();
    assert!(matches!(check(&Planned { path: p("a_ao.png"), bytes: newer }, &[none()]), Verdict::Foreign));
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn foreign_is_kept_unless_asked_and_backed_up_when_overwritten() {
    let d = temp("keep");
    fs::write(d.join("a_n.png"), plain_png()).unwrap();
    let plan = [Planned { path: d.join("a_n.png"), bytes: marked() }];
    assert!(write(&plan, &[none()], &[false], true).unwrap().is_empty());
    assert_eq!(fs::read(d.join("a_n.png")).unwrap(), plain_png());
    assert_eq!(write(&plan, &[none()], &[true], true).unwrap(), vec![d.join("a_n.png")]);
    assert_eq!(fs::read(d.join("a_n.png")).unwrap(), marked());
    assert_eq!(fs::read(d.join("_sdc/texalith-backup/a_n.png")).unwrap(), plain_png());
    // Written again with the same bytes: Same, skipped.
    assert!(write(&plan, &[none()], &[true], true).unwrap().is_empty());
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn the_opened_file_is_never_written_even_when_ours() {
    // Final review Minor 0: a Texalith Height data opened as the master would otherwise be Ours at its own path.
    let d = temp("source");
    let opened = d.join("bark_height.png");
    fs::write(&opened, marked()).unwrap();
    for spelled in [opened.clone(), d.join("BARK_HEIGHT.PNG")] {
        let plan = [Planned { path: spelled, bytes: marked_other() }];
        assert!(matches!(check(&plan[0], std::slice::from_ref(&opened)), Verdict::Source));
        assert!(write(&plan, std::slice::from_ref(&opened), &[true], true).unwrap().is_empty());
    }
    assert_eq!(fs::read(&opened).unwrap(), marked());
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn a_second_backup_of_one_name_keeps_the_first() {
    // Final review Minor 1: the backup is the only copy of a foreign file.
    let d = temp("backup2");
    let plan = [Planned { path: d.join("a_ao.png"), bytes: marked() }];
    fs::write(d.join("a_ao.png"), plain_png()).unwrap();
    write(&plan, &[none()], &[true], true).unwrap();
    let second = png_with(&[3, 4], None);
    fs::write(d.join("a_ao.png"), &second).unwrap();
    write(&plan, &[none()], &[true], true).unwrap();
    let b = d.join("_sdc/texalith-backup");
    assert_eq!(fs::read(b.join("a_ao.png")).unwrap(), plain_png());
    assert_eq!(fs::read(b.join("a_ao-2.png")).unwrap(), second);
    // The same foreign bytes again reuse their backup instead of piling up copies.
    fs::write(d.join("a_ao.png"), &second).unwrap();
    write(&plan, &[none()], &[true], true).unwrap();
    assert!(!b.join("a_ao-3.png").exists());
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn an_unreadable_target_is_always_kept() {
    let d = temp("unreadable");
    fs::create_dir(d.join("a_ao.png")).unwrap(); // reading a directory fails with something other than NotFound
    let plan = [Planned { path: d.join("a_ao.png"), bytes: marked() }];
    assert!(matches!(check(&plan[0], &[none()]), Verdict::Unreadable(_)));
    assert!(write(&plan, &[none()], &[true], true).unwrap().is_empty());
    assert!(d.join("a_ao.png").is_dir());
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn a_failed_write_rolls_back_the_files_before_it() {
    let d = temp("roll");
    fs::write(d.join("a_ao.png"), marked()).unwrap();
    // Ruling 3: `blocker` is a file, so the path below reads as absent (New) and its write fails.
    fs::write(d.join("blocker"), b"x").unwrap();
    let plan = [
        Planned { path: d.join("a_ao.png"), bytes: marked_other() },
        Planned { path: d.join("a_curvature.png"), bytes: marked() },
        Planned { path: d.join("blocker").join("a_cavity.png"), bytes: marked() },
    ];
    assert!(matches!(check(&plan[2], &[none()]), Verdict::New));
    let err = write(&plan, &[none()], &[false; 3], false).unwrap_err();
    assert!(err.contains("a_cavity.png"), "{err}");
    assert_eq!(fs::read(d.join("a_ao.png")).unwrap(), marked());
    assert!(!d.join("a_curvature.png").exists());
    fs::remove_dir_all(&d).unwrap();
}

#[test]
fn an_unasked_write_stops_for_a_question_before_it_writes_anything() {
    let d = temp("unasked");
    let plan = [Planned { path: d.join("a_h.png"), bytes: marked() }, Planned { path: d.join("a_ao.png"), bytes: marked_other() }];
    // A new file and one of Texalith's own: written at once.
    fs::write(d.join("a_ao.png"), marked()).unwrap();
    assert!(matches!(write_or_ask(&plan, &[none()]).unwrap(), Unasked::Written(w) if w.len() == 2));
    assert_eq!(fs::read(d.join("a_ao.png")).unwrap(), marked_other());
    // A foreign file: nothing is written, the new file included.
    fs::remove_file(d.join("a_h.png")).unwrap();
    fs::write(d.join("a_ao.png"), plain_png()).unwrap();
    match write_or_ask(&plan, &[none()]).unwrap() {
        Unasked::Ask(v) => assert!(matches!(v[..], [Verdict::New, Verdict::Foreign]), "{v:?}"),
        Unasked::Written(w) => panic!("wrote {w:?}"),
    }
    assert!(!d.join("a_h.png").exists());
    assert_eq!(fs::read(d.join("a_ao.png")).unwrap(), plain_png());
    fs::remove_dir_all(&d).unwrap();
}
