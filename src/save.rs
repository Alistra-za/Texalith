//! Saving the From normal outputs: the files to write, what the overwrite dialog shows, and the write.
use crate::export::{self, check, Planned, Verdict};
use crate::from_normal::files::{base_name, gray_png, normal_png, out_name};
use crate::from_normal::input::Kind;
use crate::from_normal::open::Opened;
use crate::from_normal::settings::Settings;
use crate::from_normal::worker::Outputs;
use crate::marker::{self, MapKind};
use eframe::egui;
use std::path::{Path, PathBuf};

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

#[derive(Debug, Clone)]
pub struct Review {
    pub rows: Vec<Row>,
    /// Copy a foreign file to `_sdc/texalith-backup/` before writing over it.
    pub backup: bool,
}

/// Checks every planned file against the disk; every row starts on Keep, with a backup.
pub fn review(plan: Vec<(MapKind, Planned)>, source: &Path) -> Review {
    let rows = plan
        .into_iter()
        .map(|(kind, planned)| {
            let verdict = check(&planned, source);
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
                Verdict::Source => Some("This is the file you opened, so it is kept".into()),
                _ => None,
            };
            Row { kind, planned, verdict, reason, overwrite: false }
        })
        .collect();
    Review { rows, backup: true }
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
        self.rows
            .iter()
            .any(|r| matches!(r.verdict, Verdict::Foreign | Verdict::Unreadable(_) | Verdict::Source))
    }

    /// Rows that will not be written: the source, an unreadable file, or a foreign file left on Keep.
    pub fn kept(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| matches!(r.verdict, Verdict::Source | Verdict::Unreadable(_)) || (r.verdict == Verdict::Foreign && !r.overwrite))
            .count()
    }

    /// Writes all or nothing; foreign files only where their row says Overwrite.
    pub fn write(&self, source: &Path) -> Result<Vec<PathBuf>, String> {
        let plan: Vec<Planned> = self.rows.iter().map(|r| r.planned.clone()).collect();
        let flags: Vec<bool> = self.rows.iter().map(|r| r.overwrite).collect();
        export::write(&plan, source, &flags, self.backup)
    }
}
