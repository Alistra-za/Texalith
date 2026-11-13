//! Saving the From normal outputs: the files to write, what the overwrite dialog shows, and the write.
use crate::export::{self, check, Planned, Unasked, Verdict};
use crate::from_normal::files::{base_name, gray_png, normal_png, out_name};
use crate::from_normal::input::Kind;
use crate::from_normal::open::Opened;
use crate::from_normal::settings::Settings;
use crate::from_normal::worker::Outputs;
use crate::marker::{self, MapKind};
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};

/// One file per ticked output, in the order Height data, Parallax, Normal, AO, Curvature, Cavity, named from the
/// opened file into `folder`. Height data is always encoded, so a Parallax map carries its master's pixel hash.
pub fn plan(o: &Opened, s: &Settings, out: &Outputs, folder: &Path) -> Vec<(MapKind, Planned)> {
    let base = base_name(&o.path);
    let json = |k| s.json(k, &out.report, out.size.0);
    let at = |k| folder.join(out_name(&base, k));
    let (w, h) = out.size;
    let src = &o.source;
    let height = gray_png(&out.height.0, MapKind::HeightData, src, json(MapKind::HeightData), Some(out.height.1), None);
    let t = s.ticks;
    let mut v = vec![];
    let mut add = |k: MapKind, bytes: Vec<u8>| v.push((k, Planned { path: at(k), bytes }));
    let master = marker::read(&height).map(|m| m.pixels_sha256);
    if t.height {
        add(MapKind::HeightData, height);
    }
    if let (true, Some((g, _))) = (t.parallax, &out.parallax) {
        add(MapKind::Parallax, gray_png(g, MapKind::Parallax, src, json(MapKind::Parallax), None, master));
    }
    if t.normal && o.kind != Kind::Normal {
        if let Some(n) = &out.normal {
            add(MapKind::Normal, normal_png(w, h, n, src, json(MapKind::Normal)));
        }
    }
    let rest = [(MapKind::Ao, t.ao, &out.ao), (MapKind::Curvature, t.curvature, &out.curvature), (MapKind::Cavity, t.cavity, &out.cavity)];
    for (k, on, g) in rest {
        if let (true, Some(g)) = (on, g) {
            add(k, gray_png(g, k, src, json(k), None, None));
        }
    }
    v
}

/// A planned file, what is at its path now, and whether to write over it.
#[derive(Debug, Clone)]
pub struct Row {
    pub kind: MapKind,
    pub planned: Planned,
    pub verdict: Verdict,
    /// Why the file is kept, for the dialog; `None` when it is written without asking.
    pub reason: Option<String>,
    /// Foreign files only: the person chose Overwrite. Keep is the default.
    pub overwrite: bool,
}

pub struct Review {
    pub rows: Vec<Row>,
    /// Copy a foreign file to `_sdc/texalith-backup/` before writing over it.
    pub backup: bool,
    /// The files open in Texalith when Save was pressed: never written.
    sources: Vec<PathBuf>,
    /// The status line of the Save press that asked, set by `poll`.
    say: Option<Say>,
}

/// Checks every planned file against the disk; every row starts on Keep, with a backup. `sources` are the files
/// open in Texalith.
pub fn review(plan: Vec<(MapKind, Planned)>, sources: &[PathBuf]) -> Review {
    let verdicts = plan.iter().map(|(_, p)| check(p, sources)).collect();
    rows(plan, verdicts, sources.to_vec())
}

/// The review of `plan` with the `verdicts` a check gave, one per planned file.
fn rows(plan: Vec<(MapKind, Planned)>, verdicts: Vec<Verdict>, sources: Vec<PathBuf>) -> Review {
    let rows = plan
        .into_iter()
        .zip(verdicts)
        .map(|((kind, planned), verdict)| {
            let reason = match &verdict {
                Verdict::Foreign => Some(
                    match std::fs::read(&planned.path)
                        .ok()
                        .is_some_and(|b| marker::fits(&b) && marker::read(&b).is_some())
                    {
                        true => "Made by Texalith, then changed",
                        false => "Not made by Texalith, or its marker was removed",
                    }
                    .to_string(),
                ),
                Verdict::Unreadable(e) => Some(format!("Cannot be read, so it is kept: {e}")),
                Verdict::Source => Some("This file is open in Texalith, so it is kept".into()),
                _ => None,
            };
            Row { kind, planned, verdict, reason, overwrite: false }
        })
        .collect();
    Review { rows, backup: true, sources, say: None }
}

// egui-ui-rules 5.1 and 7 prefer a non-modal panel; the spec (research report line 203) names egui::Modal.
/// The overwrite dialog: `Some(true)` for Save, `Some(false)` for Cancel, Esc or a click outside, `None` while open.
/// No button takes Enter.
pub fn dialog(ctx: &egui::Context, r: &mut Review) -> Option<bool> {
    let mut answer = None;
    let shown = egui::Modal::new(egui::Id::new("save-review")).show(ctx, |ui| {
        ui.set_max_width(420.0);
        ui.heading("Some files will be kept");
        for row in r.rows.iter_mut() {
            let Some(reason) = &row.reason else { continue };
            let name = row.planned.path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            ui.add_space(8.0);
            ui.label(egui::RichText::new(&name).strong());
            crate::tab::line(ui, reason);
            if row.verdict == Verdict::Foreign {
                ui.horizontal(|ui| {
                    for (text, value) in [("Keep", false), ("Overwrite", true)] {
                        let b = ui.selectable_value(&mut row.overwrite, value, text);
                        let selected = row.overwrite == value;
                        b.widget_info(|| {
                            egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, format!("{text} {name}"))
                        });
                    }
                });
            }
        }
        if r.rows.iter().any(|row| row.verdict == Verdict::Foreign) {
            ui.add_space(8.0);
            ui.checkbox(&mut r.backup, "Keep a backup in _sdc\\texalith-backup");
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                answer = Some(true);
            }
            if ui.button("Cancel").clicked() {
                answer = Some(false);
            }
        });
    });
    if answer.is_none() && shown.should_close() {
        answer = Some(false);
    }
    answer
}

impl Review {
    /// Some file will be kept or needs a choice.
    pub fn needs_dialog(&self) -> bool {
        self.rows.iter().any(|r| r.verdict.asks())
    }

    /// Rows that will not be written: the source, an unreadable file, or a foreign file left on Keep.
    pub fn kept(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| matches!(r.verdict, Verdict::Source | Verdict::Unreadable(_)) || (r.verdict == Verdict::Foreign && !r.overwrite))
            .count()
    }

    /// Writes all or nothing; foreign files only where their row says Overwrite.
    pub fn write(&self) -> Result<Vec<PathBuf>, String> {
        let flags: Vec<bool> = self.rows.iter().map(|r| r.overwrite).collect();
        export::write(self.rows.iter().map(|r| &r.planned), &self.sources, &flags, self.backup)
    }
}

/// The status line for a finished write, from the number of files written and of files kept.
pub type Say = Box<dyn Fn(usize, usize) -> String + Send>;

/// The status line of a tab that saves one file, `file`, into `folder`.
pub fn says_one(folder: PathBuf, file: String) -> Say {
    Box::new(move |written, kept| match (written, kept) {
        (0, 0) => format!("Nothing to save: {file} in {} already matches", folder.display()),
        (0, _) => format!("Nothing saved: {file} was kept in {}", folder.display()),
        _ => format!("Saved {file} to {}", folder.display()),
    })
}

/// How a save step ended.
enum Saved {
    /// Some file needs a choice; nothing was written.
    Ask(Review),
    /// The write's result, and how many files were kept.
    Done(Result<Vec<PathBuf>, String>, usize),
}

/// A save step running on its own thread, so encoding and writing a large PNG does not freeze the window.
pub struct Saving {
    rx: Receiver<Saved>,
    say: Say,
}

fn run(ctx: &egui::Context, say: Say, step: impl FnOnce() -> Saved + Send + 'static) -> Saving {
    let (tx, rx) = channel();
    let ctx = ctx.clone();
    RUNNING.fetch_add(1, Ordering::SeqCst);
    let running = Running;
    std::thread::spawn(move || {
        // Dropped when the thread ends, also by a panic.
        let _running = running;
        let _ = tx.send(step());
        ctx.request_repaint();
    });
    Saving { rx, say }
}

/// Saves whose threads have not ended.
static RUNNING: AtomicUsize = AtomicUsize::new(0);

struct Running;

impl Drop for Running {
    fn drop(&mut self) {
        RUNNING.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Save pressed: `encode` makes the files, then they are written at once when nothing on disk needs a choice.
/// `sources` are the files open in Texalith.
pub fn start(
    ctx: &egui::Context,
    sources: Vec<PathBuf>,
    say: Say,
    encode: impl FnOnce() -> Vec<(MapKind, Planned)> + Send + 'static,
) -> Saving {
    run(ctx, say, move || {
        let plan = encode();
        match export::write_or_ask(plan.iter().map(|(_, p)| p), &sources) {
            Ok(Unasked::Written(w)) => Saved::Done(Ok(w), 0),
            Ok(Unasked::Ask(verdicts)) => Saved::Ask(rows(plan, verdicts, sources)),
            Err(e) => Saved::Done(Err(e), 0),
        }
    })
}

/// The dialog's Save: writes `r` as its rows say, and reports with the status line of the Save press: the folder or
/// the file open may have changed since. `open` are the files open now, so one opened since the press is kept too.
pub fn start_write(ctx: &egui::Context, mut r: Review, open: Vec<PathBuf>) -> Saving {
    r.sources.extend(open);
    let say = r.say.take().unwrap_or_else(|| Box::new(|written, kept| format!("{written} written, {kept} kept")));
    run(ctx, say, move || Saved::Done(r.write(), r.kept()))
}

/// Takes a finished step out of `saving`: a question goes to `review`, a result to `status`.
pub fn poll(saving: &mut Option<Saving>, review: &mut Option<Review>, status: &mut Option<Result<String, String>>) {
    let Some(s) = saving else { return };
    let answer = match s.rx.try_recv() {
        Err(TryRecvError::Empty) => return,
        a => a,
    };
    let Some(Saving { say, .. }) = saving.take() else { return };
    match answer {
        Ok(Saved::Ask(mut r)) => {
            r.say = Some(say);
            *review = Some(r);
        }
        Ok(Saved::Done(result, kept)) => *status = Some(result.map(|written| say(written.len(), kept))),
        // The thread ended without an answer, so it panicked.
        Err(_) => *status = Some(Err("Saving stopped on an error inside Texalith. Check the files in the save folder.".into())),
    }
}

/// Returns once no save is running. `main` calls it after the window has closed: the process would otherwise end in
/// the middle of a save, between two files of a set.
pub fn wait() {
    while RUNNING.load(Ordering::SeqCst) > 0 {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// The "Saving…" row a tab shows while its save runs.
pub fn saving_row(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label("Saving…");
    });
}
