//! The Cubemap tab: the source, the three settings, the jobs it sends, and Save.
use super::files::{self, cube_png};
use super::import::{cell, fits};
use super::make::face_size;
use super::open::{self, Opened};
use super::view::FACE_LABELS;
use super::worker::{Job, Output};
use super::{default_softness, Flip, Ground, Settings, SourceType};
use crate::export::{file_name, free_name, Planned};
use crate::from_normal::files::Source;
use crate::marker::MapKind;
use crate::rnm::tab::guarded;
use crate::save::{self, Review, Saving};
use crate::settle::{self, Settle};
use crate::tab::{error_line, folder_row, hint, line, slider_row, Row, EXTENSIONS};
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;

pub const OPEN_SOURCE: &str = "Open source…";
const EMPTY: &str = "Open a panorama, cross, strip, mirror ball, angular map, photo or one of six face files to make a cube.";
const PICK_ONE: &str = "Pick one";
// Help sentences, verbatim from plan 5's Global Constraints.
const HELP_SOURCE: &str = "Tells the tool how your picture wraps around the sphere, and a wrong choice scrambles the reflection; change it if the labelled preview looks wrong or the image is square.";
const HELP_SOFTNESS: &str = "Blurs the reflection evenly with no seams; raise it for rough metal or a low-detail source, and lower it for mirror-like surfaces.";
const HELP_BRIGHTNESS: &str = "Makes all reflected light brighter or darker, and +1 doubles it; change it if reflections look dim or overpower the surface.";
// Advanced help sentences, verbatim from plan 6's Global Constraints.
const HELP_SIDE: &str = "Sets the width of each face in pixels; leave it on Auto, and raise it only for a sharp mirror seen up close, because a bigger cube costs memory and a soft reflection gains nothing.";
const HELP_TURN: &str = "Turns the surroundings around the up axis, and positive turns clockwise seen from above; use it to move the sun or a bright window to where the reflection should show it.";
const HELP_MIRROR: &str = "Swaps east and west; tick it if writing or a known landmark reads backwards in the preview.";
const HELP_ROLLOFF: &str = "Squeezes very bright light such as the sun so it fades into the sky instead of clipping to a hard white patch; tick it for HDR sources, and lower the white point to squeeze harder.";
const HELP_COLOUR: &str = "Sets how colourful the reflection is, and 0 makes it grey; lower it if a coloured sky tints every shiny surface, and raise it for a deliberately vivid look.";
const HELP_TINT: &str = "Multiplies the reflection by a colour, and white changes nothing; use it to warm or cool reflections to match the scene's light.";
const HELP_GROUND: &str = "Replaces everything below the horizon line; use it when the bottom of the source shows a tripod, a blank area or the wrong floor.";
const HELP_HORIZON: &str = "Sets where the ground starts, in degrees above or below level; raise it if a strip of the old floor still shows above the fill.";
const HELP_FIX: &str = "Turns or flips one face of a cross, strip or six face files; use it when one face in the labelled preview is rotated or mirrored against its neighbours.";
const HELP_BOTTOM: &str = "A vertical cross usually stores the bottom face upside down, so it is turned back; untick it if the down face in the preview is upside down.";
const SIDES: [u32; 10] = [4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048];
const TURNS: [(u16, &str); 4] = [(0, "None"), (90, "90° clockwise"), (180, "180°"), (270, "90° anticlockwise")];
const FLIPS: [(Flip, &str); 3] = [(Flip::None, "None"), (Flip::LeftRight, "Left to right"), (Flip::TopBottom, "Top to bottom")];
/// Tessalith's first ground colour.
const GROUND_COLOUR: [u8; 3] = [40, 30, 20];
/// The marker keys of six face files, +X..-Z.
const FACE_KEYS: [&str; 6] = ["posx", "negx", "posy", "negy", "posz", "negz"];

pub struct CubeTab {
    jobs: Sender<Job>,
    replies: Receiver<Arc<Output>>,
    pub opened: Option<Arc<Opened>>,
    pub settings: Settings,
    pub outputs: Option<Arc<Output>>,
    /// Where Save writes; the source's folder until changed.
    pub folder: Option<PathBuf>,
    pub status: Option<Result<String, String>>,
    /// The newest job's id.
    pub jobs_sent: u64,
    /// A Save waiting on the overwrite dialog.
    pub review: Option<Review>,
    /// A Save encoding or writing on its thread.
    pub saving: Option<Saving>,
    /// The files open in the other tabs, set by the app: Save keeps them as it keeps its own sources.
    pub elsewhere: Vec<PathBuf>,
    /// The source being read; a newer open replaces it, so the last one opened wins.
    load: Option<(PathBuf, Receiver<Result<Opened, String>>)>,
    /// The settings last acted on (sent, or refused as not fitting), so any change in the column is acted on once.
    sent: Option<Settings>,
    /// The "does not fit" line `act` put in `status`, so it clears that line and no other.
    misfit_said: Option<String>,
    settle: Settle<Settings>,
    /// The id of the current source's first job; older outputs belong to another source.
    first_job: u64,
    /// The face Fix one side edits.
    fix_face: usize,
}

fn ground_name(g: Ground) -> &'static str {
    match g {
        Ground::Keep => "Keep the source",
        Ground::Darken => "Darken toward the bottom",
        Ground::Colour(_) => "Solid colour",
    }
}

impl CubeTab {
    pub fn new(jobs: Sender<Job>, replies: Receiver<Arc<Output>>) -> CubeTab {
        CubeTab {
            jobs,
            replies,
            opened: None,
            settings: Settings::default(),
            outputs: None,
            folder: None,
            status: None,
            jobs_sent: 0,
            review: None,
            saving: None,
            elsewhere: vec![],
            load: None,
            sent: None,
            misfit_said: None,
            settle: Settle::default(),
            first_job: 0,
            fix_face: 0,
        }
    }

    /// Reads `path` (and its five siblings for a face file) on a thread. A source still loading is dropped.
    pub fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
        // The pick closes the old file's dialog and status line, not the arrival: a running save may ask or report
        // while the file is read, and that answer is not the old file's dialog.
        (self.review, self.status) = (None, None);
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let file = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(guarded(&path, || open::open(&path)));
            ctx.request_repaint();
        });
        self.load = Some((file, rx));
    }

    /// A source is still being read.
    pub fn busy(&self) -> bool {
        self.load.is_some()
    }

    /// Takes every output queued; any output of the current source newer than the one shown is shown. True when new
    /// outputs arrived.
    pub fn drain(&mut self) -> bool {
        let mut new = false;
        loop {
            match self.replies.try_recv() {
                Ok(o) => {
                    if o.id >= self.first_job && self.outputs.as_ref().is_none_or(|cur| o.id > cur.id) {
                        self.outputs = Some(o);
                        new = true;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // The cube thread is gone; only a job still waiting on it needs saying.
                    if self.jobs_sent > 0 && self.outputs.as_ref().map(|o| o.id) != Some(self.jobs_sent) {
                        self.status = Some(Err("Making the cube stopped. Restart Texalith.".into()));
                    }
                    break;
                }
            }
        }
        new
    }

    fn poll_load(&mut self) {
        let Some((_, rx)) = &self.load else { return };
        let r = match rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Could not read the source: the reader stopped.".into()),
        };
        self.load = None;
        match r {
            Err(e) => self.status = Some(Err(e)),
            Ok(o) => {
                // A running save is left to finish: it reports, or asks, when it is done.
                (self.outputs, self.sent) = (None, None);
                self.folder = o.sources[0].path.parent().map(PathBuf::from);
                self.settings =
                    Settings { source: o.detected, softness: default_softness(o.detected), ..Settings::default() };
                self.first_job = self.jobs_sent + 1;
                self.fix_face = 0;
                self.opened = Some(Arc::new(o));
            }
        }
    }

    fn first_source(&self) -> Option<&Source> {
        self.opened.as_ref().map(|o| &o.sources[0])
    }

    /// Why the picked type cannot read the source, or `None` when it can. Six face files read only as Six face files.
    fn misfit(&self, t: SourceType) -> Option<String> {
        let o = self.opened.as_ref()?;
        let img = &o.imgs[0];
        let ok = if o.imgs.len() == 6 { t == SourceType::Faces } else { fits(t, img.width, img.height) };
        (!ok).then(|| format!("{} is {}x{}, which does not fit {}.", file_name(&o.sources[0].path), img.width, img.height, t.name()))
    }

    /// The picked type reads the source.
    fn fitting(&self) -> Option<SourceType> {
        self.settings.source.filter(|&t| self.misfit(t).is_none())
    }

    /// Acts once on settings that changed: sends a job when the type fits, else says why not.
    fn act(&mut self, ui: &egui::Ui) {
        let Some(opened) = self.opened.clone() else { return };
        let pending = self.sent.as_ref() != Some(&self.settings);
        if !settle::due(ui, &mut self.settle, pending, &self.settings) {
            if pending && self.jobs_sent >= self.first_job && self.outputs.as_ref().map(|o| o.id) != Some(self.jobs_sent) {
                // A job for older settings is still out, and showing its result would stall the drag. With `sent`
                // empty a job follows even when the slider comes back to the value that job had.
                self.first_job = self.jobs_sent + 1;
                self.sent = None;
            }
            return;
        }
        self.sent = Some(self.settings.clone());
        let Some(t) = self.settings.source else { return };
        if let Some(why) = self.misfit(t) {
            self.misfit_said = Some(why.clone());
            self.status = Some(Err(why));
            return;
        }
        // A save's line stays, as in the other tabs: it may have landed while this source was being read.
        if self.misfit_said.take().is_some_and(|m| self.status == Some(Err(m))) {
            self.status = None;
        }
        self.jobs_sent += 1;
        let _ = self.jobs.send(Job { id: self.jobs_sent, opened, source: t, settings: self.settings.clone() });
    }

    pub fn draw_column(&mut self, ui: &mut egui::Ui) {
        save::poll(&mut self.saving, &mut self.review, &mut self.status);
        self.poll_load();
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        if self.opened.is_some() {
            // Save stays in view below the scrolling rows (egui-ui-rules 1.5).
            egui::Panel::bottom("cube-save").show(ui, |ui| {
                ui.add_space(4.0);
                self.save_rows(ui);
                ui.add_space(4.0);
            });
        } else {
            ui.skip_ahead_auto_ids(1);
        }
        ui.spacing_mut().scroll = egui::style::ScrollStyle::thin();
        egui::ScrollArea::both().id_salt("cube-column").auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(8.0);
            ui.heading("Cubemap");
            self.source_rows(ui);
            if self.opened.is_some() {
                self.setting_rows(ui);
            }
            ui.add_space(8.0);
        });
        if let Some(r) = &mut self.review {
            match save::dialog(ui.ctx(), r) {
                Some(true) => {
                    let r = self.review.take().expect("the dialog is open");
                    let open = [self.open_files(), self.elsewhere.clone()].concat();
                    self.saving = Some(save::start_write(ui.ctx(), r, open));
                }
                Some(false) => self.review = None,
                None => {}
            }
        }
        self.act(ui);
    }

    fn source_rows(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        if ui.button(OPEN_SOURCE).clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("Images", &EXTENSIONS).pick_file() {
                self.open(p, ui.ctx());
            }
        }
        if self.busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Opening…");
            });
        }
        match self.first_source() {
            Some(s) => {
                ui.add(egui::Label::new(egui::RichText::new(file_name(&s.path)).strong()).truncate())
                    .on_hover_text(s.path.display().to_string());
            }
            None => {
                line(ui, EMPTY);
                // With no source there is no Save area, so a failed read shows here.
                if let Some(Err(e)) = &self.status {
                    error_line(ui, e);
                }
            }
        }
    }

    fn setting_rows(&mut self, ui: &mut egui::Ui) {
        let six = self.opened.as_ref().is_some_and(|o| o.detected == Some(SourceType::Faces));
        let before = self.settings.source;
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("Source type")
                .selected_text(before.map_or(PICK_ONE, SourceType::name))
                .show_ui(ui, |ui| {
                    for t in SourceType::ALL {
                        ui.add_enabled_ui(t != SourceType::Faces || six, |ui| {
                            ui.selectable_value(&mut self.settings.source, Some(t), t.name());
                        });
                    }
                });
            hint(ui, "Source type", HELP_SOURCE);
        });
        // Softness follows the type while it is still at the old type's default.
        if self.settings.source != before && self.settings.softness == default_softness(before) {
            self.settings.softness = default_softness(self.settings.source);
        }
        let soft = Row {
            name: "Reflection softness",
            range: 0.0..=1.0,
            default: default_softness(self.settings.source),
            help: HELP_SOFTNESS,
            scale: 45.0,
            suffix: "°",
            decimals: 0,
            log: false,
        };
        slider_row(ui, &soft, &mut self.settings.softness);
        let bright = Row {
            name: "Brightness",
            range: -10.0..=10.0,
            default: 0.0,
            help: HELP_BRIGHTNESS,
            scale: 1.0,
            suffix: " stops",
            decimals: 1,
            log: false,
        };
        slider_row(ui, &bright, &mut self.settings.brightness);
        ui.add_space(6.0);
        egui::CollapsingHeader::new("Advanced").id_salt("cube-advanced").show(ui, |ui| self.advanced_rows(ui));
    }

    /// The 10 Advanced items, in the spec's order; the ones a type does not read are hidden.
    fn advanced_rows(&mut self, ui: &mut egui::Ui) {
        let auto = self.fitting().zip(self.opened.as_ref()).map(|(t, o)| {
            let img = &o.imgs[0];
            face_size(self.settings.softness, cell(t, img.width, img.height))
        });
        let t = self.settings.source;
        let s = &mut self.settings;

        let auto_name = auto.map_or_else(|| "Auto".to_string(), |n| format!("Auto ({n} px)"));
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("Side size")
                .selected_text(s.side.map_or_else(|| auto_name.clone(), |n| format!("{n} px")))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.side, None, auto_name.as_str());
                    for n in SIDES {
                        ui.selectable_value(&mut s.side, Some(n), format!("{n} px"));
                    }
                });
            hint(ui, "Side size", HELP_SIDE);
        });

        let turn = Row { name: "Turn", range: -180.0..=180.0, default: 0.0, help: HELP_TURN, scale: 1.0, suffix: "°", decimals: 0, log: false };
        slider_row(ui, &turn, &mut s.turn);

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.mirror, "Mirror");
            hint(ui, "Mirror", HELP_MIRROR);
        });

        ui.add_space(6.0);
        let mut on = s.rolloff.is_some();
        ui.horizontal(|ui| {
            if ui.checkbox(&mut on, "Highlight rolloff").changed() {
                s.rolloff = on.then_some(4.0);
            }
            hint(ui, "Highlight rolloff", HELP_ROLLOFF);
        });
        if let Some(w) = &mut s.rolloff {
            let white = Row { name: "White point", range: 1.0..=64.0, default: 4.0, help: HELP_ROLLOFF, scale: 1.0, suffix: "×", decimals: 1, log: true };
            slider_row(ui, &white, w);
        }

        let colour = Row { name: "Colour strength", range: 0.0..=2.0, default: 1.0, help: HELP_COLOUR, scale: 100.0, suffix: " %", decimals: 0, log: false };
        slider_row(ui, &colour, &mut s.colour);

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Tint");
            let b = ui.color_edit_button_srgb(&mut s.tint);
            b.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ColorButton, true, "Tint colour"));
            if ui.add_enabled(s.tint != [255; 3], egui::Button::new("No tint")).clicked() {
                s.tint = [255; 3];
            }
            hint(ui, "Tint", HELP_TINT);
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("Ground").selected_text(ground_name(s.ground)).show_ui(ui, |ui| {
                for g in [Ground::Keep, Ground::Darken, Ground::Colour(GROUND_COLOUR)] {
                    let on = ground_name(s.ground) == ground_name(g);
                    if ui.selectable_label(on, ground_name(g)).clicked() && !on {
                        s.ground = g;
                    }
                }
            });
            if let Ground::Colour(c) = &mut s.ground {
                let b = ui.color_edit_button_srgb(c);
                b.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ColorButton, true, "Ground colour"));
            }
            hint(ui, "Ground", HELP_GROUND);
        });
        if s.ground != Ground::Keep {
            let horizon = Row { name: "Horizon line", range: -45.0..=45.0, default: 0.0, help: HELP_HORIZON, scale: 1.0, suffix: "°", decimals: 1, log: false };
            slider_row(ui, &horizon, &mut s.horizon);
        }

        if t.is_some_and(SourceType::has_cells) {
            ui.add_space(6.0);
            let f = &mut self.fix_face;
            ui.horizontal(|ui| {
                egui::ComboBox::from_label("Fix one side").selected_text(FACE_LABELS[*f]).show_ui(ui, |ui| {
                    for (k, label) in FACE_LABELS.iter().enumerate() {
                        ui.selectable_value(f, k, *label);
                    }
                });
                hint(ui, "Fix one side", HELP_FIX);
            });
            let f = *f;
            let turn_name = TURNS.iter().find(|(d, _)| *d == s.face_turn[f]).map_or("None", |&(_, n)| n);
            egui::ComboBox::from_label("Turn this face").selected_text(turn_name).show_ui(ui, |ui| {
                for (d, n) in TURNS {
                    ui.selectable_value(&mut s.face_turn[f], d, n);
                }
            });
            let flip_name = FLIPS.iter().find(|(v, _)| *v == s.face_flip[f]).map_or("None", |&(_, n)| n);
            egui::ComboBox::from_label("Flip this face").selected_text(flip_name).show_ui(ui, |ui| {
                for (v, n) in FLIPS {
                    ui.selectable_value(&mut s.face_flip[f], v, n);
                }
            });
            let changed: Vec<&str> =
                (0..6).filter(|&k| s.face_turn[k] != 0 || s.face_flip[k] != Flip::None).map(|k| FACE_LABELS[k]).collect();
            if !changed.is_empty() {
                line(ui, &format!("Changed faces: {}", changed.join(", ")));
            }
        }

        if t == Some(SourceType::VCross) {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut s.turn_bottom, "Turn bottom cell");
                hint(ui, "Turn bottom cell", HELP_BOTTOM);
            });
        }
    }

    /// The files this tab has open, with one still being read.
    // shortcut: the five siblings of a face file are listed only once the set has been read.
    pub fn open_files(&self) -> Vec<PathBuf> {
        let loading = self.load.iter().map(|(p, _)| p.clone());
        self.opened.iter().flat_map(|o| o.sources.iter().map(|s| s.path.clone())).chain(loading).collect()
    }

    /// The output's file name in the save folder: the first numbered name that is no file open in Texalith.
    fn save_name(&self) -> Option<String> {
        let (o, f) = (self.opened.as_ref()?, self.folder.as_ref()?);
        let taken: Vec<&Path> = o.sources.iter().map(|s| s.path.as_path()).chain(self.elsewhere.iter().map(PathBuf::as_path)).collect();
        Some(free_name(f, &files::out_name(&o.sources.first()?.path, o.sources.len() == 6), &taken))
    }

    fn save_rows(&mut self, ui: &mut egui::Ui) {
        folder_row(ui, &mut self.folder);
        let name = self.save_name();
        let ready = self.outputs.as_ref().is_some_and(|o| o.id == self.jobs_sent)
            && self.fitting().is_some()
            && !self.busy()
            && name.is_some()
            && self.saving.is_none();
        let save = ui.add_enabled(ready, egui::Button::new("Save"));
        if save.on_hover_text(format!("Saves as {}", name.unwrap_or_default())).clicked() {
            self.save(ui.ctx());
        }
        save::status_row(ui, self.saving.is_some(), &self.status);
    }

    /// Encodes and writes on a thread (`save::start`): at once when nothing on disk needs a choice, otherwise the
    /// dialog opens when the thread answers.
    fn save(&mut self, ctx: &egui::Context) {
        let (Some(o), Some(out), Some(f), Some(name)) = (&self.opened, &self.outputs, &self.folder, self.save_name()) else {
            return;
        };
        let (o, out, settings, path) = (o.clone(), out.clone(), self.settings.clone(), f.join(&name));
        let sources = [self.open_files(), self.elsewhere.clone()].concat();
        self.status = None;
        self.saving = Some(save::start(ctx, sources, save::says_one(f.clone(), name), move || {
            let keys: &[&'static str] = if o.sources.len() == 6 { &FACE_KEYS } else { &["source"] };
            let inputs: Vec<(&'static str, &Source)> = keys.iter().copied().zip(&o.sources).collect();
            vec![(MapKind::Cube, Planned { path, bytes: cube_png(&out.cube, &inputs, &settings) })]
        }));
    }
}
