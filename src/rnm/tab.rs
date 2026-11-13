//! The RNM tab: the base, the layer list, the jobs it sends, and the column of controls.
use super::blend::{decode_base, red};
use super::files::{self, rnm_png, Loaded, Plane, Recorded};
use super::worker::{Job, Output};
use super::{Input, Kind, Layer};
use crate::export::{file_name, free_name, Planned};
use crate::from_normal::files::{source, Source};
use crate::marker::MapKind;
use crate::save::{self, Review, Saving};
use crate::settle::{self, Settle};
use crate::tab::{error_line, folder_row, hint, line, slider_row_named, Row, EXTENSIONS};
use eframe::egui;
use engine::image::Rgba32F;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;

pub const OPEN_BASE: &str = "Open base…";
const EMPTY: &str = "Open a base normal (_n or _cnr) to blend layers into it.";
const NO_LAYERS: &str = "No layers yet. Add one to blend it into the base.";
const ALPHA: &str = "Alpha is kept as it is.";
// Help sentences, verbatim from the spec (reports/Texalith generation modes and settings.md, lines 155-163).
const HELP_STRENGTH: &str = "Sets how strongly this layer's bumps show over the base; raise it if the detail is hard to see, and lower it if it looks pasted on.";
const HELP_DEPTH: &str = "Sets how tall this layer's bumps are; raise it if the layer barely shows, which is common with low-contrast files, and lower it if it hides the base.";
const HELP_MASK: &str = "Limits the layer to the white parts of a mask image; add one when this detail should stay off some areas.";
// The spec gives these no help text; plan 3 fixes them.
const HELP_ORDER: &str = "Layers blend from the top down, so each layer sits over the ones above it.";
const HELP_FLIP: &str = "Flips this layer's green channel; turn it on if its bumps look like dents next to the base's.";
const HELP_SHADOWS: &str = "Uses the AO map so the bent normal shows most where the surface is shadowed; without it the bend is even everywhere.";
const HELP_FOCUS: &str = "Raise it to keep the bend in the darkest crevices, and lower it to spread it into lighter areas.";

const HANDLE: &str = "☰";
const UP: &str = "⏶";
const DOWN: &str = "⏷";
const REMOVE: &str = "🗙";
/// Every symbol the column draws as text. The bundled fonts lack many symbols, and a missing one draws as an empty
/// box, so a test checks the font has each of these.
pub const GLYPHS: [&str; 4] = [HANDLE, UP, DOWN, REMOVE];

/// What the "Add layer" menu says about a kind.
fn about(kind: Kind) -> &'static str {
    match kind {
        Kind::Bent => "A bent normal map; leans the base toward it, mostly where the AO map is dark.",
        Kind::Bump => "A grey height image; its slopes become bumps over the base.",
        Kind::Detail => "A tangent-space normal map; its detail is laid over the base.",
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Part {
    File,
    Mask,
    Ao,
}

/// One file place: a layer, by its `uid`, and which of its files.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Slot {
    pub layer: u64,
    pub part: Part,
}

pub struct LayerState {
    /// Never reused, so a file that finishes loading finds its layer after a reorder.
    pub uid: u64,
    pub set: Layer,
    /// What the strength reset goes back to.
    pub default: f32,
    pub file: Option<Loaded>,
    pub mask: Option<Plane>,
    pub ao: Option<Plane>,
}

/// What a load thread read, shaped for where it goes.
enum Read {
    /// The base's file and its image as `decode_base`d; the raw image is not kept.
    Base(Source, Arc<Rgba32F>),
    Layer(Loaded),
    /// A mask or an AO map.
    Plane(Plane),
}

/// A file being read on a thread: for the base (`None`) or a layer's file.
type Pending = (Option<Slot>, PathBuf, Receiver<Result<Read, String>>);

pub struct RnmTab {
    jobs: Sender<Job>,
    replies: Receiver<Arc<Output>>,
    /// The base file, and its image as `decode_base`d; the raw image is not kept.
    pub base: Option<(Source, Arc<Rgba32F>)>,
    /// The layers in blend order, top first.
    pub layers: Vec<LayerState>,
    pub outputs: Option<Arc<Output>>,
    /// Where Save writes; the base's folder until changed.
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
    loads: Vec<Pending>,
    /// The layers and settings of the newest job, so any change in the column sends a job. The uid is part of it, so
    /// two layers with equal settings swapping places is a change.
    sent: Option<Vec<(u64, Layer)>>,
    settle: Settle<Vec<(u64, Layer)>>,
    /// The id of the current base's first job; older outputs belong to another base.
    base_job: u64,
    next_uid: u64,
    /// The `_ao` file beside the base, for a Bent layer added later.
    sibling_ao: Option<PathBuf>,
    /// While a layer is dragged over the list: the insert place under the pointer.
    drop_at: Option<usize>,
}

/// `path` hashed and decoded for `part` of a layer, or for the base (`None`); the `Err` names the file. Runs on a
/// load thread, so the base is `decode_base`d here and not on the window's thread.
fn load(path: &Path, part: Option<Part>) -> Result<Read, String> {
    let fail = |e: &dyn std::fmt::Display| format!("Could not read {}: {e}", file_name(path));
    // The header alone gives the size, so a file too large is refused before it is hashed and decoded (a 16384
    // image is 4 GB decoded). A header this cannot read is left to the decoder, which names its own error.
    if let Some(why) = engine::load::image_size(path).ok().and_then(|size| files::oversize(path, size)) {
        return Err(why);
    }
    let source = source(path).map_err(|e| fail(&e))?;
    let (img, _) = engine::load::load_image(path).map_err(|e| fail(&e))?;
    Ok(match part {
        None => Read::Base(source, Arc::new(decode_base(&img))),
        Some(Part::File) => Read::Layer(Loaded { source, img: Arc::new(img) }),
        Some(Part::Mask | Part::Ao) => Read::Plane(Plane { source, img: Arc::new(red(&img)) }),
    })
}

/// `f`, with a panic turned into an `Err` that names the file, so a reader bug is reported instead of dropped.
pub(crate) fn guarded<T>(path: &Path, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err(format!("Could not read {}: the image reader stopped on it.", file_name(path))))
}

fn pick() -> Option<PathBuf> {
    rfd::FileDialog::new().add_filter("Images", &EXTENSIONS).pick_file()
}

/// The insert place, 0 to `centres.len()`, for a pointer at height `y`: the number of blocks whose centre is above it.
pub fn drop_slot(centres: &[f32], y: f32) -> usize {
    centres.iter().filter(|&&c| c < y).count()
}

/// Where the layer at `from` ends up when it is dropped at insert place `slot`.
pub fn drop_index(from: usize, slot: usize) -> usize {
    if slot > from {
        slot - 1
    } else {
        slot
    }
}

/// What a dragged layer header carries: the layer's uid.
struct DragLayer(u64);

/// What a click in the column asks for, applied after drawing.
enum Act {
    Choose(Slot),
    Clear(Slot),
    /// The layer at the first index goes to the second.
    Move(usize, usize),
    Remove(u64),
    Add(Kind),
}

/// A button that shows `text` and is named `what`.
fn named_button(ui: &mut egui::Ui, text: &str, what: &str, on: bool) -> bool {
    let b = ui.add_enabled(on, egui::Button::new(text)).on_hover_text(what);
    b.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, on, what));
    b.clicked()
}

/// A checkbox that shows `text` and is named `what`.
fn named_checkbox(ui: &mut egui::Ui, v: &mut bool, text: &str, what: &str) {
    let c = ui.checkbox(v, text);
    let on = *v;
    c.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, what));
}

/// The file name (full path on hover) or `empty`, then "Choose…" named `choose` and, when `clear` names one, a clear button.
fn slot_row(ui: &mut egui::Ui, file: Option<&Source>, empty: &str, choose: &str, clear: Option<&str>, slot: Slot, acts: &mut Vec<Act>) {
    egui::Sides::new().shrink_left().truncate().show(
        ui,
        |ui| match file {
            Some(s) => {
                ui.add(egui::Label::new(file_name(&s.path)).truncate()).on_hover_text(s.path.display().to_string());
            }
            None => {
                ui.weak(empty);
            }
        },
        |ui| {
            if let Some(clear) = clear {
                if named_button(ui, REMOVE, clear, file.is_some()) {
                    acts.push(Act::Clear(slot));
                }
            }
            if named_button(ui, "Choose…", choose, true) {
                acts.push(Act::Choose(slot));
            }
        },
    );
}

impl RnmTab {
    pub fn new(jobs: Sender<Job>, replies: Receiver<Arc<Output>>) -> RnmTab {
        RnmTab {
            jobs,
            replies,
            base: None,
            layers: vec![],
            outputs: None,
            folder: None,
            status: None,
            jobs_sent: 0,
            review: None,
            saving: None,
            elsewhere: vec![],
            loads: vec![],
            sent: None,
            settle: Settle::default(),
            base_job: 0,
            next_uid: 0,
            sibling_ao: None,
            drop_at: None,
        }
    }

    fn start_load(&mut self, slot: Option<Slot>, path: PathBuf, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let part = slot.map(|s| s.part);
        let file = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(guarded(&path, || load(&path, part)));
            ctx.request_repaint();
        });
        self.loads.push((slot, file, rx));
    }

    /// Reads `path` on a thread; on arrival a refused base sets the status and changes nothing else. A base still
    /// loading is dropped, so the last one opened wins; an `_msn` name is refused before anything is read.
    pub fn open_base(&mut self, path: PathBuf, ctx: &egui::Context) {
        if let Some(why) = files::refuse(&path, (0, 0)) {
            self.status = Some(Err(why));
            return;
        }
        // The pick closes the old file's dialog and status line, not the arrival: a running save may ask or report
        // while the file is read, and that answer is not the old file's dialog.
        (self.review, self.status) = (None, None);
        self.loads.retain(|(s, ..)| s.is_some());
        self.start_load(None, path, ctx);
    }

    /// Reads `path` into `slot` on a thread; on arrival it sends a job, or reports the file and leaves the slot. A
    /// file whose layer is gone by then is dropped, and so is a file still loading for the same slot: the last one
    /// chosen wins.
    pub fn fill(&mut self, slot: Slot, path: PathBuf, ctx: &egui::Context) {
        self.loads.retain(|(s, ..)| *s != Some(slot));
        self.start_load(Some(slot), path, ctx);
    }

    /// A file is still being read.
    pub fn busy(&self) -> bool {
        !self.loads.is_empty()
    }

    fn push_layer(&mut self, kind: Kind, strength: f32) -> u64 {
        self.next_uid += 1;
        let set = Layer { strength, ..Layer::new(kind) };
        self.layers.push(LayerState { uid: self.next_uid, set, default: strength, file: None, mask: None, ao: None });
        self.next_uid
    }

    /// Adds an empty layer at the end and returns its uid. A Bent layer loads the `_ao` file beside the base.
    pub fn add_layer(&mut self, kind: Kind, ctx: &egui::Context) -> u64 {
        let uid = self.push_layer(kind, kind.default_strength());
        if let (Kind::Bent, Some(p)) = (kind, self.sibling_ao.clone()) {
            self.fill(Slot { layer: uid, part: Part::Ao }, p, ctx);
        }
        uid
    }

    /// Takes the layer away, with any of its files still being read.
    pub fn remove_layer(&mut self, uid: u64) {
        self.loads.retain(|(s, ..)| s.is_none_or(|s| s.layer != uid));
        self.layers.retain(|l| l.uid != uid);
    }

    /// The layer at `from` ends at `to`; an index outside the list does nothing.
    pub fn move_layer(&mut self, from: usize, to: usize) {
        if from != to && from < self.layers.len() && to < self.layers.len() {
            let l = self.layers.remove(from);
            self.layers.insert(to, l);
        }
    }

    /// Empties a mask or an AO map, with a file still loading for it. A layer's own file is not cleared;
    /// `remove_layer` takes the layer away.
    pub fn clear(&mut self, slot: Slot) {
        if slot.part != Part::File {
            self.loads.retain(|(s, ..)| *s != Some(slot));
        }
        let Some(l) = self.layers.iter_mut().find(|l| l.uid == slot.layer) else { return };
        let had = match slot.part {
            Part::Mask => l.mask.take().is_some(),
            Part::Ao => l.ao.take().is_some(),
            Part::File => false,
        };
        if had {
            self.submit();
        }
    }

    fn key(&self) -> Vec<(u64, Layer)> {
        self.layers.iter().map(|l| (l.uid, l.set)).collect()
    }

    /// Sends the base and every layer that has a file as a new job.
    pub fn submit(&mut self) {
        let Some((_, decoded)) = &self.base else { return };
        self.jobs_sent += 1;
        let layers = self
            .layers
            .iter()
            .filter_map(|l| {
                let img = l.file.as_ref()?.img.clone();
                Some(Input { set: l.set, img, mask: l.mask.as_ref().map(|m| m.img.clone()), ao: l.ao.as_ref().map(|a| a.img.clone()) })
            })
            .collect();
        let job = Job { id: self.jobs_sent, base: decoded.clone(), layers };
        self.sent = Some(self.key());
        let _ = self.jobs.send(job);
    }

    /// Takes every output queued. Any output of the current base newer than the one shown is shown, so the view keeps
    /// moving while newer jobs wait; Save still waits for the newest job. True when new outputs arrived.
    pub fn drain(&mut self) -> bool {
        let mut new = false;
        loop {
            match self.replies.try_recv() {
                Ok(o) => {
                    if o.id >= self.base_job && self.outputs.as_ref().is_none_or(|cur| o.id > cur.id) {
                        self.outputs = Some(o);
                        new = true;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // The blend thread is gone; only a job still waiting on it needs saying.
                    if self.jobs_sent > 0 && self.outputs.as_ref().map(|o| o.id) != Some(self.jobs_sent) {
                        self.status = Some(Err("Blending stopped. Restart Texalith.".into()));
                    }
                    break;
                }
            }
        }
        new
    }

    fn accept_base(&mut self, source: Source, decoded: Arc<Rgba32F>, ctx: &egui::Context) {
        let path = source.path.clone();
        if let Some(why) = files::refuse(&path, (decoded.width, decoded.height)) {
            self.status = Some(Err(why));
            return;
        }
        // Layer files still loading were chosen for the old base.
        self.loads.retain(|(s, ..)| s.is_none());
        self.layers.clear();
        // A running save is left to finish: it reports, or asks, when it is done.
        self.outputs = None;
        self.folder = path.parent().map(PathBuf::from);
        self.base = Some((source, decoded));
        self.base_job = self.jobs_sent + 1;
        let found = files::siblings(&path);
        self.sibling_ao = found.ao;
        for f in found.layers {
            let uid = self.push_layer(f.kind, f.strength);
            self.fill(Slot { layer: uid, part: Part::File }, f.file, ctx);
            if let Some(m) = f.mask {
                self.fill(Slot { layer: uid, part: Part::Mask }, m, ctx);
            }
            if let (Kind::Bent, Some(a)) = (f.kind, self.sibling_ao.clone()) {
                self.fill(Slot { layer: uid, part: Part::Ao }, a, ctx);
            }
        }
        self.submit();
    }

    fn poll_loads(&mut self, ctx: &egui::Context) {
        let mut arrived = vec![];
        self.loads.retain(|(slot, _, rx)| match rx.try_recv() {
            Ok(r) => {
                arrived.push((*slot, r));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        });
        for (slot, r) in arrived {
            match (slot, r) {
                (_, Err(e)) => self.status = Some(Err(e)),
                (None, Ok(Read::Base(source, decoded))) => self.accept_base(source, decoded, ctx),
                (Some(s), Ok(read)) => {
                    // A layer removed while its file was read has no place for it.
                    let Some(layer) = self.layers.iter_mut().find(|x| x.uid == s.layer) else { continue };
                    match (s.part, read) {
                        (Part::File, Read::Layer(l)) => layer.file = Some(l),
                        (Part::Mask, Read::Plane(p)) => layer.mask = Some(p),
                        (Part::Ao, Read::Plane(p)) => layer.ao = Some(p),
                        // `load` shapes what it read by the part it was given.
                        _ => continue,
                    }
                    self.submit();
                }
                (None, Ok(_)) => {}
            }
        }
    }

    pub fn draw_column(&mut self, ui: &mut egui::Ui) {
        save::poll(&mut self.saving, &mut self.review, &mut self.status);
        self.poll_loads(ui.ctx());
        let mut acts = vec![];
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        if self.base.is_some() {
            // Save stays in view below the scrolling rows (egui-ui-rules 1.5).
            egui::Panel::bottom("rnm-save").show(ui, |ui| {
                ui.add_space(4.0);
                self.save_rows(ui);
                ui.add_space(4.0);
            });
        } else {
            ui.skip_ahead_auto_ids(1);
        }
        ui.spacing_mut().scroll = egui::style::ScrollStyle::thin();
        // Its own id: the Material column shares this panel and would otherwise reset this scroll.
        egui::ScrollArea::both().id_salt("rnm-column").auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(8.0);
            ui.heading("RNM");
            self.base_rows(ui);
            if self.base.is_some() {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Layers").strong());
                    hint(ui, "Layers", HELP_ORDER);
                });
                if self.layers.is_empty() {
                    ui.scope(|ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        ui.weak(NO_LAYERS);
                    });
                }
                let blocks: Vec<egui::Rect> = (0..self.layers.len()).map(|i| self.layer_rows(ui, i, &mut acts)).collect();
                ui.add_space(8.0);
                let add = ui.menu_button("Add layer", |ui| {
                    for kind in Kind::ALL {
                        if ui.button(kind.name()).on_hover_text(about(kind)).clicked() {
                            acts.push(Act::Add(kind));
                        }
                    }
                });
                self.drag_rows(ui, &blocks, add.response.rect, &mut acts);
                ui.add_space(8.0);
                line(ui, ALPHA);
            }
            ui.add_space(8.0);
        });
        for a in acts {
            match a {
                Act::Choose(s) => {
                    if let Some(p) = pick() {
                        self.fill(s, p, ui.ctx());
                    }
                }
                Act::Clear(s) => self.clear(s),
                Act::Move(from, to) => self.move_layer(from, to),
                Act::Remove(uid) => self.remove_layer(uid),
                // The layer is added only when a file is picked.
                Act::Add(kind) => {
                    if let Some(p) = pick() {
                        let uid = self.add_layer(kind, ui.ctx());
                        self.fill(Slot { layer: uid, part: Part::File }, p, ui.ctx());
                    }
                }
            }
        }
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
        let key = self.key();
        let pending = self.base.is_some() && self.sent.as_ref() != Some(&key);
        if settle::due(ui, &mut self.settle, pending, &key) {
            self.submit();
        } else if pending && self.jobs_sent >= self.base_job && self.outputs.as_ref().map(|o| o.id) != Some(self.jobs_sent) {
            // A job for older settings is still out, and showing its result would stall the drag. With `sent` empty a
            // job follows even when the slider comes back to the value that job had.
            self.base_job = self.jobs_sent + 1;
            self.sent = None;
        }
    }

    fn base_rows(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        if ui.button(OPEN_BASE).clicked() {
            if let Some(p) = pick() {
                self.open_base(p, ui.ctx());
            }
        }
        if self.busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Opening…");
            });
        }
        match &self.base {
            Some((s, _)) => {
                ui.add(egui::Label::new(egui::RichText::new(file_name(&s.path)).strong()).truncate())
                    .on_hover_text(s.path.display().to_string());
            }
            None => {
                line(ui, EMPTY);
                // With no base there is no Save area, so a refusal or a failed read shows here.
                if let Some(Err(e)) = &self.status {
                    error_line(ui, e);
                }
            }
        }
    }

    /// While a layer is dragged: keeps the insert place under the pointer, paints a line there, and on release asks
    /// for the move. `blocks` are this frame's layer blocks and `add` is the "Add layer" button.
    fn drag_rows(&mut self, ui: &egui::Ui, blocks: &[egui::Rect], add: egui::Rect, acts: &mut Vec<Act>) {
        let dragged = egui::DragAndDrop::payload::<DragLayer>(ui.ctx()).map(|d| d.0);
        let (Some(uid), Some(first), Some(last)) = (dragged, blocks.first(), blocks.last()) else {
            self.drop_at = None;
            return;
        };
        // Only the visible part of the list takes a drop, so a release over the Save area below a long list is none.
        let area = egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), first.top()..=add.bottom()).intersect(ui.clip_rect());
        // With no pointer position (it left the window on this frame) the place from the frame before stands.
        if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
            let centres: Vec<f32> = blocks.iter().map(|b| b.center().y).collect();
            self.drop_at = area.contains(p).then(|| drop_slot(&centres, p.y));
        }
        if ui.input(|i| i.pointer.any_released()) {
            // egui drops the payload at the end of this frame.
            let from = self.layers.iter().position(|l| l.uid == uid);
            if let (Some(slot), Some(from)) = (self.drop_at.take(), from) {
                acts.push(Act::Move(from, drop_index(from, slot)));
            }
        } else if let Some(slot) = self.drop_at {
            let y = blocks.get(slot).map_or(last.bottom(), |b| b.top());
            ui.painter().hline(area.x_range(), y, egui::Stroke::new(2.0, ui.visuals().selection.bg_fill));
        }
    }

    /// One layer's block: its header, its file, and every setting it has. Returns the block's rectangle.
    fn layer_rows(&mut self, ui: &mut egui::Ui, i: usize, acts: &mut Vec<Act>) -> egui::Rect {
        let count = self.layers.len();
        let l = &mut self.layers[i];
        let (n, uid, kind) = (i + 1, l.uid, l.set.kind);
        let slot = |part| Slot { layer: uid, part };
        // Keyed by the uid, so a slider keeps its state when its layer moves.
        ui.push_id(uid, |ui| {
            ui.add_space(6.0);
            ui.separator();
            egui::Sides::new().shrink_left().truncate().show(
                ui,
                |ui| {
                    // Not selectable, so a press on the title starts the drag and not a text selection.
                    let drag = ui.dnd_drag_source(egui::Id::new(("rnm-drag-layer", uid)), DragLayer(uid), |ui| {
                        ui.add(egui::Label::new(HANDLE).selectable(false));
                        ui.add(egui::Label::new(egui::RichText::new(format!("Layer {n}: {}", kind.name())).strong()).selectable(false));
                    });
                    let what = format!("Drag layer {n}");
                    drag.response.on_hover_text("Drag to reorder").widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, &what));
                },
                |ui| {
                    if named_button(ui, REMOVE, &format!("Remove layer {n}"), true) {
                        acts.push(Act::Remove(uid));
                    }
                    if named_button(ui, DOWN, &format!("Move layer {n} down"), i + 1 < count) {
                        acts.push(Act::Move(i, i + 1));
                    }
                    if named_button(ui, UP, &format!("Move layer {n} up"), i > 0) {
                        acts.push(Act::Move(i, i.saturating_sub(1)));
                    }
                },
            );
            slot_row(ui, l.file.as_ref().map(|f| &f.source), "None", &format!("Choose file for layer {n}"), None, slot(Part::File), acts);
            if l.file.is_none() {
                return;
            }
            let row = if kind == Kind::Bump {
                // Stored as k, shown as 2k: the bump's height in texels, which the label says once.
                Row { name: "Strength (texels)", range: 0.0..=2.0, default: l.default, help: HELP_DEPTH, scale: 2.0, suffix: "", decimals: 2, log: false }
            } else {
                Row { name: "Strength", range: 0.0..=2.0, default: l.default, help: HELP_STRENGTH, scale: 1.0, suffix: "", decimals: 2, log: false }
            };
            slider_row_named(ui, &row, &mut l.set.strength, &format!("Layer {n} strength"));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                named_checkbox(ui, &mut l.set.flip, "Flip this layer", &format!("Flip layer {n}"));
                hint(ui, &format!("Layer {n} flip"), HELP_FLIP);
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Where it applies");
                hint(ui, &format!("Layer {n} mask"), HELP_MASK);
            });
            let (choose, clear) = (format!("Choose mask for layer {n}"), format!("Clear mask for layer {n}"));
            slot_row(ui, l.mask.as_ref().map(|m| &m.source), "No mask", &choose, Some(&clear), slot(Part::Mask), acts);
            if kind != Kind::Bent {
                return;
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                named_checkbox(ui, &mut l.set.use_ao, "Bend mostly in shadows", &format!("Bend layer {n} mostly in shadows"));
                hint(ui, &format!("Layer {n} bend mostly in shadows"), HELP_SHADOWS);
            });
            if l.set.use_ao {
                ui.label("AO map");
                let (choose, clear) = (format!("Choose AO map for layer {n}"), format!("Clear AO map for layer {n}"));
                slot_row(ui, l.ao.as_ref().map(|a| &a.source), "No AO map", &choose, Some(&clear), slot(Part::Ao), acts);
                let row = Row { name: "Focus on crevices", range: 0.1..=8.0, default: 1.5, help: HELP_FOCUS, scale: 1.0, suffix: "", decimals: 2, log: true };
                slider_row_named(ui, &row, &mut l.set.ao_power, &format!("Layer {n} focus on crevices"));
            }
        })
        .response
        .rect
    }

    fn base_path(&self) -> Option<&Path> {
        self.base.as_ref().map(|(s, _)| s.path.as_path())
    }

    /// The files this tab has open: the base first, then every layer's files, then those still being read.
    pub fn open_files(&self) -> Vec<PathBuf> {
        let files = self
            .layers
            .iter()
            .flat_map(|l| [l.file.as_ref().map(|f| &f.source), l.mask.as_ref().map(|m| &m.source), l.ao.as_ref().map(|a| &a.source)])
            .flatten();
        let loading = self.loads.iter().map(|(_, p, _)| p.clone());
        self.base_path().into_iter().map(Path::to_path_buf).chain(files.map(|s| s.path.clone())).chain(loading).collect()
    }

    /// The output's file name in the save folder: the first numbered name that is no file open in Texalith.
    fn save_name(&self) -> Option<String> {
        let (b, f) = (self.base_path()?, self.folder.as_ref()?);
        let open = self.open_files();
        let taken: Vec<&Path> = open.iter().chain(&self.elsewhere).map(PathBuf::as_path).collect();
        Some(free_name(f, &files::out_name(b), &taken))
    }

    fn save_rows(&mut self, ui: &mut egui::Ui) {
        folder_row(ui, &mut self.folder);
        let name = self.save_name();
        let ready = self.outputs.as_ref().is_some_and(|o| o.id == self.jobs_sent)
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
        let (Some((base, _)), Some(out), Some(f), Some(name)) = (&self.base, &self.outputs, &self.folder, self.save_name()) else {
            return;
        };
        let (base, out, path) = (base.clone(), out.clone(), f.join(&name));
        // Only a layer with a file is blended, so only those are recorded.
        let layers: Vec<(Layer, Source, Option<Source>, Option<Source>)> = self
            .layers
            .iter()
            .filter_map(|l| {
                let of = |p: &Option<Plane>| p.as_ref().map(|p| p.source.clone());
                Some((l.set, l.file.as_ref()?.source.clone(), of(&l.mask), of(&l.ao)))
            })
            .collect();
        let sources = [self.open_files(), self.elsewhere.clone()].concat();
        self.status = None;
        self.saving = Some(save::start(ctx, sources, save::says_one(f.clone(), name), move || {
            let recorded: Vec<Recorded> =
                layers.iter().map(|(set, file, mask, ao)| Recorded { set: *set, file, mask: mask.as_ref(), ao: ao.as_ref() }).collect();
            let bytes = rnm_png(out.size.0, out.size.1, &out.px, &base, &recorded);
            vec![(MapKind::Rnm, Planned { path, bytes })]
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panicking_reader_is_reported_by_file_name() {
        let r: Result<(), String> = guarded(Path::new("C:/t/rock_n.png"), || panic!("decoder bug"));
        assert_eq!(r, Err("Could not read rock_n.png: the image reader stopped on it.".into()));
        assert_eq!(guarded(Path::new("C:/t/rock_n.png"), || Ok::<_, String>(7)), Ok(7));
    }

    #[test]
    fn the_drop_slot_counts_the_blocks_above_the_pointer() {
        let c = [100.0, 200.0, 300.0];
        assert_eq!([50.0, 150.0, 250.0, 999.0].map(|y| drop_slot(&c, y)), [0, 1, 2, 3]);
        assert_eq!(drop_slot(&[], 5.0), 0);
    }

    #[test]
    fn a_drop_beside_itself_keeps_the_place() {
        assert_eq!((drop_index(1, 1), drop_index(1, 2)), (1, 1));
        assert_eq!((drop_index(0, 3), drop_index(2, 0)), (2, 0));
    }
}
