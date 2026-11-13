//! The From normal tab: what is open, its settings and outputs, the jobs it sends, and the column of controls.
use crate::from_normal::input::Kind;
use crate::from_normal::maps::{default_crevice, FlatLevel};
use crate::from_normal::master::{Edges, Green};
use crate::from_normal::open::{open, Opened};
use crate::from_normal::settings::{fade_pct, Settings, PRESETS};
use crate::from_normal::worker::{Job, Outputs, Reply};
use crate::marker::MapKind;
use crate::save::{self, Review, Saving};
use crate::settle::{self, Settle};
use crate::view2d;
use eframe::egui;
use egui::collapsing_header::{paint_default_icon, CollapsingState};
use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;

/// The column's width in points.
pub const COLUMN_WIDTH: f32 = 340.0;
pub const OPEN_LABEL: &str = "Open a normal or height map…";
pub(crate) const EXTENSIONS: [&str; 12] = ["png", "tga", "tif", "tiff", "bmp", "jpg", "jpeg", "webp", "exr", "hdr", "psd", "dds"];

// Purpose sentences, one per output tick (plan 2, Global Constraints).
const PURPOSE_HEIGHT: &str = "The true heights, lowest black and highest white; the source for normals and other tools.";
const PURPOSE_PARALLAX: &str = "A display map for parallax shaders, made from Height data.";
const PURPOSE_NORMAL: &str = "A normal map made from this height.";
const PURPOSE_AO: &str = "Soft shadow that gathers in crevices; white is open.";
const PURPOSE_CURVATURE: &str = "Bright edges and dark creases on grey.";
const PURPOSE_CAVITY: &str = "Dark only in narrow cracks; white is open.";

// Help sentences, verbatim from the spec (reports/Texalith generation modes and settings.md, lines 119-132).
const HELP_SIZE: &str = "Sets how sharp the maps are and how long they take; lower it for quick tries, and raise it for the final maps.";
const HELP_EDGES: &str = "Says whether opposite edges join up; leave it on Repeats for tiling textures, and choose Does not repeat only for a crop or single photo, because the wrong choice on a tile leaves a hard seam.";
const HELP_FLATTEN: &str = "Fades hills and slopes wider than the size shown; stay near 0 for rock, ground and maps whose height truly matches the normal, move toward 1 for brick, bark, weave and carved parts, and use Compare if unsure.";
const HELP_DEPTH: &str = "Sets how far the surface appears to rise and sink; raise it if the effect is hard to see, and lower it if the surface warps or swims at low angles.";
const HELP_MID_GREY: &str = "Details rise and sink around mid-grey; use this for ordinary parallax shaders, including True PBR.";
const HELP_WHITE: &str = "White is the untouched surface and everything sinks below it; use this only if the target shader treats white as flat, such as a multilayer inner layer.";
const HELP_NORMAL_DEPTH: &str = "Sets how far white sits above black, which makes every bump steeper; raise it if the surface looks flat, and lower it if lighting looks harsh or noisy.";
const HELP_GREEN: &str = "Sets which way the green channel points, and the wrong one makes bumps look like dents; set it once to match where the map will be used.";
const HELP_REACH: &str = "Sets how far shadow spreads from each crevice; it matters most on deep surfaces like bark, so raise it for soft broad shading and lower it if whole areas turn grey.";
const HELP_DARKNESS: &str = "Makes the shadows this map already has darker or lighter without moving them; leave it at 1 if your renderer has its own AO strength setting, and raise it only if the map will be multiplied in directly.";
const HELP_SURFACE_DEPTH: &str = "Treats the surface as deeper, so shadows reach shallower dips and change shape; raise it if AO stays nearly white, which is common because a normal map implies only shallow relief.";
const HELP_LINE: &str = "Sets how wide the bright edges and dark creases are; raise it to catch large forms, and lower it for thin crisp lines.";
const HELP_CONTRAST: &str = "Makes edges and creases stand out from the grey; raise it if the map looks nearly flat, and lower it if large areas hit pure black or white.";
const HELP_CREVICE: &str = "Sets the widest crack that goes dark; raise it if grooves stay too light, and lower it if it starts to look like broad AO.";
// Advanced controls have no spec sentence; these follow the same pattern.
const HELP_SMOOTH: &str = "Softens hard steps before the map is made; raise it if parallax shows stair steps at sharp edges.";
const HELP_INVERT: &str = "Swaps high and low; use it only if the target shader reads the map the other way up.";
const HELP_GREEN_IN: &str = "Says which way the source's green points; leave it on Guess unless the guess is marked undetermined or bumps look like dents.";
const SEAM_HINT: &str = "These edges may not join; choose Does not repeat if this is a crop or single photo.";
const AO_LINES: [&str; 2] = ["Estimated from the normal, not baked from a mesh.", "If you have a real height, load it."];
const NOT_SAVED: &str = "Height data is still made for the Parallax map, but it will not be saved.";

pub struct Tab {
    jobs: Sender<Job>,
    replies: Receiver<Reply>,
    pub opened: Option<Opened>,
    pub settings: Option<Settings>,
    pub outputs: Option<Arc<Outputs>>,
    /// Where Save writes; the opened file's folder until changed.
    pub folder: Option<PathBuf>,
    pub status: Option<Result<String, String>>,
    /// The newest job's id.
    pub jobs_sent: u64,
    pub compare: bool,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<Arc<AtomicU32>>,
    running: bool,
    /// The settings of the newest job, so any change in the column sends a job.
    sent: Option<Settings>,
    settle: Settle<Option<Settings>>,
    last_draft: bool,
    /// A slider whose drags send drafts is held this frame.
    dragging: bool,
    opening: Option<(PathBuf, Receiver<Result<Opened, String>>)>,
    compare_tex: Option<(u64, [egui::TextureHandle; 3])>,
    /// The section thumbnails of the outputs with this id.
    thumbs: Option<(u64, HashMap<MapKind, egui::TextureHandle>)>,
    /// A Save waiting on the overwrite dialog.
    pub review: Option<Review>,
    /// A Save encoding or writing on its thread.
    pub saving: Option<Saving>,
    /// The files open in the other tabs, set by the app: Save keeps them as it keeps its own source.
    pub elsewhere: Vec<PathBuf>,
}

/// One slider row's fixed parts. `scale` maps the stored value to the shown one (0.9 → 90 %).
pub(crate) struct Row<'a> {
    pub(crate) name: &'a str,
    pub(crate) range: RangeInclusive<f32>,
    pub(crate) default: f32,
    pub(crate) help: &'a str,
    pub(crate) scale: f32,
    pub(crate) suffix: &'a str,
    pub(crate) decimals: usize,
    pub(crate) log: bool,
}

impl Row<'_> {
    fn shown(&self, v: f32) -> String {
        let s = format!("{:.*}", self.decimals, v * self.scale);
        let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
        format!("{s}{}", self.suffix)
    }
}

#[derive(Default)]
pub(crate) struct Moved {
    pub(crate) dragging: bool,
    pub(crate) reset: bool,
}

/// The accessible name of an option's help button. It holds the sentence, so the help is reachable without hovering.
pub fn hint_name(what: &str, text: &str) -> String {
    format!("Help for {what}: {text}")
}

/// A "?" button beside the option `what`; its help sentence shows on hover.
pub(crate) fn hint(ui: &mut egui::Ui, what: &str, text: &str) {
    let b = ui.add(egui::Button::new("?").min_size(egui::vec2(20.0, 20.0))).on_hover_text(text);
    // After `add`: widget_info replaces the label egui set.
    b.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, hint_name(what, text)));
}

/// A wrapped, faint line that is guidance and not an option's help.
pub(crate) fn faint(ui: &mut egui::Ui, text: &str) {
    ui.scope(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.label(egui::RichText::new(text).small().weak());
    });
}

/// A wrapped line of small text; readouts drawn with it share one size.
fn small(ui: &mut egui::Ui, text: &str) {
    ui.scope(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.label(egui::RichText::new(text).small());
    });
}

pub(crate) fn line(ui: &mut egui::Ui, text: &str) {
    ui.scope(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.label(text);
    });
}

pub(crate) fn warn(ui: &mut egui::Ui, text: &str) {
    let c = ui.visuals().warn_fg_color;
    ui.scope(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.colored_label(c, text);
    });
}

pub(crate) fn error_line(ui: &mut egui::Ui, text: &str) {
    let c = ui.visuals().error_fg_color;
    ui.scope(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.colored_label(c, text);
    });
}

/// Name with its help button, slider, and a reset naming the default.
pub(crate) fn slider_row(ui: &mut egui::Ui, r: &Row, v: &mut f32) -> Moved {
    slider_row_named(ui, r, v, r.name)
}

/// `slider_row` for a row whose visible name repeats in its column: the label shows `r.name`, and the help button,
/// the slider and the reset are named `named`.
pub(crate) fn slider_row_named(ui: &mut egui::Ui, r: &Row, v: &mut f32, named: &str) -> Moved {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(r.name);
        hint(ui, named, r.help);
    });
    let mut shown = *v * r.scale;
    let reset_name = format!("Reset {} to {}", named, r.shown(r.default));
    let (s, reset) = ui
        .horizontal(|ui| {
            // Room for the widest value text, its box and the reset button, so a long suffix stays in the column.
            let widest = format!("{:.*}{}", r.decimals, *r.range.end() * r.scale, r.suffix);
            let font = egui::TextStyle::Body.resolve(ui.style());
            let text = ui.painter().layout_no_wrap(widest, font, egui::Color32::PLACEHOLDER).size().x;
            ui.spacing_mut().slider_width = (ui.available_width() - text - 60.0).max(60.0);
            let range = *r.range.start() * r.scale..=*r.range.end() * r.scale;
            let s = ui.add(
                egui::Slider::new(&mut shown, range)
                    .logarithmic(r.log)
                    .suffix(r.suffix)
                    .max_decimals(r.decimals)
                    // Not `Always`: that writes the value back rounded to `decimals` on every frame, so a value set
                    // from outside the slider (Match the normal, a marker's depth) would lose its digits.
                    .clamping(egui::SliderClamping::Edits),
            );
            s.widget_info(|| egui::WidgetInfo::slider(true, f64::from(shown), named));
            let b = ui.small_button("↺").on_hover_text(&reset_name);
            b.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &reset_name));
            (s, b.clicked())
        })
        .inner;
    if s.changed() {
        *v = (shown / r.scale).clamp(*r.range.start(), *r.range.end());
    }
    if reset {
        *v = r.default;
    }
    Moved { dragging: s.dragged(), reset }
}

/// "Save to", the folder's last part (full path on hover) and Change….
pub(crate) fn folder_row(ui: &mut egui::Ui, folder: &mut Option<PathBuf>) {
    ui.label("Save to");
    ui.horizontal(|ui| {
        match folder {
            Some(f) => {
                let tail = f.file_name().map_or(f.display().to_string(), |n| format!("…\\{}", n.to_string_lossy()));
                ui.add(egui::Label::new(tail).truncate()).on_hover_text(f.display().to_string());
            }
            None => {
                ui.weak("No folder yet");
            }
        }
        if ui.button("Change…").clicked() {
            if let Some(f) = rfd::FileDialog::new().pick_folder() {
                *folder = Some(f);
            }
        }
    });
}

fn purpose(k: MapKind) -> &'static str {
    match k {
        MapKind::HeightData => PURPOSE_HEIGHT,
        MapKind::Parallax => PURPOSE_PARALLAX,
        MapKind::Normal => PURPOSE_NORMAL,
        MapKind::Ao => PURPOSE_AO,
        MapKind::Curvature => PURPOSE_CURVATURE,
        MapKind::Cavity => PURPOSE_CAVITY,
        MapKind::Rnm | MapKind::Cube => unreachable!("RNM and Cube are not From normal outputs"),
    }
}

/// The side of a section's thumbnail, in points.
const THUMB: f32 = 36.0;

/// A section's thumbnail, fitted into a square; an empty outlined square when the map is not made.
fn thumb_box(ui: &mut egui::Ui, name: &str, tex: Option<&egui::TextureHandle>) {
    let (rect, r) = ui.allocate_exact_size(egui::Vec2::splat(THUMB), egui::Sense::hover());
    match tex {
        Some(t) => {
            let size = t.size_vec2();
            let fit = egui::Rect::from_center_size(rect.center(), size * (THUMB / size.max_elem().max(1.0)));
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().image(t.id(), fit, uv, egui::Color32::WHITE);
        }
        None => {
            let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
            ui.painter().rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
        }
    }
    let label = format!("{name} {}", if tex.is_some() { "thumbnail" } else { "not made" });
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &label));
}

impl Tab {
    pub fn new(jobs: Sender<Job>, replies: Receiver<Reply>) -> Tab {
        Tab {
            jobs,
            replies,
            opened: None,
            settings: None,
            outputs: None,
            folder: None,
            status: None,
            jobs_sent: 0,
            compare: false,
            cancel: None,
            progress: None,
            running: false,
            sent: None,
            settle: Settle::default(),
            last_draft: false,
            dragging: false,
            opening: None,
            compare_tex: None,
            thumbs: None,
            review: None,
            saving: None,
            elsewhere: vec![],
        }
    }

    /// Takes `o` as the source: settings for it, Save into its folder, and a full job.
    pub fn accept(&mut self, o: Opened) {
        self.settings = Some(Settings::for_source(&o.kind, (o.img.width, o.img.height)));
        self.folder = o.path.parent().map(PathBuf::from);
        self.outputs = None;
        self.compare_tex = None;
        self.thumbs = None;
        // A running save is left to finish: it reports, or asks, when it is done, and Save stays off until then.
        self.opened = Some(o);
        self.submit(false);
    }

    /// Sends the current settings as a new job and cancels the one before it.
    pub fn submit(&mut self, draft: bool) {
        let (Some(o), Some(s)) = (&self.opened, &self.settings) else { return };
        if let Some(c) = &self.cancel {
            c.store(true, Ordering::Relaxed);
        }
        self.jobs_sent += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicU32::new(0));
        let job = Job {
            id: self.jobs_sent,
            img: o.img.clone(),
            kind: Settings::job_kind(&o.kind),
            settings: s.clone(),
            draft,
            compare: self.compare,
            cancel: cancel.clone(),
            progress: progress.clone(),
        };
        (self.cancel, self.progress, self.running) = (Some(cancel), Some(progress), true);
        self.sent = Some(s.clone());
        self.last_draft = draft;
        let _ = self.jobs.send(job);
    }

    /// Takes every reply queued; only the newest job's counts. True when new outputs arrived.
    pub fn drain(&mut self) -> bool {
        let mut new = false;
        loop {
            match self.replies.try_recv() {
                Ok(Reply::Done(o)) if o.id == self.jobs_sent => {
                    self.outputs = Some(Arc::from(o));
                    self.running = false;
                    new = true;
                }
                Ok(Reply::Failed { id, message }) if id == self.jobs_sent => {
                    self.status = Some(Err(message));
                    self.running = false;
                }
                Ok(_) => {}
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if self.running {
                        self.running = false;
                        self.status = Some(Err("Map generation stopped. Restart Texalith.".into()));
                    }
                    break;
                }
            }
        }
        new
    }

    /// A job runs whose outputs are not here yet.
    pub fn running(&self) -> bool {
        self.running
    }

    /// Reads `path` on a thread; `poll_open` takes it when it has arrived.
    pub fn start_open(&mut self, path: PathBuf, ctx: &egui::Context) {
        // The pick closes the old file's dialog and status line, not the arrival: a running save may ask or report
        // while the file is read, and that answer is not the old file's dialog.
        (self.review, self.status) = (None, None);
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let file = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(open(&path));
            ctx.request_repaint();
        });
        self.opening = Some((file, rx));
    }

    fn poll_open(&mut self) {
        let Some((_, rx)) = &self.opening else { return };
        match rx.try_recv() {
            Ok(Ok(o)) => {
                self.opening = None;
                self.accept(o);
            }
            Ok(Err(e)) => {
                self.opening = None;
                self.status = Some(Err(e));
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.opening = None,
        }
    }

    pub fn draw_column(&mut self, ui: &mut egui::Ui) {
        self.poll_open();
        save::poll(&mut self.saving, &mut self.review, &mut self.status);
        let dropped = ui.ctx().input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(p) = dropped {
            self.start_open(p, ui.ctx());
        }
        self.dragging = false;
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        // Save stays in view below the scrolling rows (egui-ui-rules 1.5).
        egui::Panel::bottom("from-normal-save").show(ui, |ui| {
            ui.add_space(4.0);
            self.save_rows(ui);
            ui.add_space(4.0);
        });
        // The default floating bar is invisible at rest; a thin one shows there is more below (egui-ui-rules 3.2).
        ui.spacing_mut().scroll = egui::style::ScrollStyle::thin();
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(8.0);
            ui.heading("Material");
            self.source_rows(ui);
            if self.settings.is_some() {
                self.shared_rows(ui);
                self.section(ui, MapKind::Parallax, Tab::parallax_always, Tab::parallax_rows);
                if self.opened.as_ref().is_some_and(|o| o.kind != Kind::Normal) {
                    self.section(ui, MapKind::Normal, Tab::normal_always, Tab::normal_rows);
                }
                self.section(ui, MapKind::Ao, Tab::ao_always, Tab::ao_rows);
                self.section(ui, MapKind::Curvature, |_, _| {}, Tab::curvature_rows);
                self.section(ui, MapKind::Cavity, |_, _| {}, Tab::cavity_rows);
                // Last: few people save it.
                self.section(ui, MapKind::HeightData, Tab::height_always, Tab::height_rows);
            }
            ui.add_space(8.0);
        });
        self.review_dialog(ui.ctx());
        let pending = self.settings != self.sent;
        if settle::due(ui, &mut self.settle, pending, &self.settings) {
            self.submit(self.dragging);
        } else if pending {
            // Only a job still running: a finished one's result is on screen, and stays right if the value comes back.
            if let (true, Some(c)) = (self.running, self.cancel.take()) {
                // The running job is for older settings, and its result would land in the middle of the drag. With
                // `sent` empty a job follows even when the slider comes back to the value that job had.
                c.store(true, Ordering::Relaxed);
                self.sent = None;
                // The thread may be past its last cancel check and still answer: no reply has this id.
                self.jobs_sent += 1;
                // Its progress is not the next job's.
                self.progress = None;
            }
        } else if self.last_draft && !self.dragging {
            self.submit(false);
        }
        if self.running {
            ui.ctx().request_repaint_after_secs(0.1);
        }
    }

    fn source_rows(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        if ui.button(OPEN_LABEL).clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("Images", &EXTENSIONS).pick_file() {
                self.start_open(p, ui.ctx());
            }
        }
        if self.opening.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Opening…");
            });
        }
        if let Some(o) = &self.opened {
            let name = crate::export::file_name(&o.path);
            ui.add(egui::Label::new(egui::RichText::new(name).strong()).truncate())
                .on_hover_text(o.path.display().to_string());
            let kind = match o.kind {
                Kind::Normal => "Normal map",
                _ => "Height map",
            };
            ui.label(format!("{kind}, {} × {}", o.img.width, o.img.height));
            if o.marker.as_ref().is_some_and(|m| m.kind == crate::marker::MapKind::Rnm) {
                warn(ui, "This normal was blended by RNM, so its height comes out less exact; open the base normal instead if you have it.");
            }
        } else {
            faint(ui, "Drop a file here, or open one. A normal map makes height, parallax, AO and the rest; a height map also makes a normal.");
        }
    }

    fn shared_rows(&mut self, ui: &mut egui::Ui) {
        let sizes = self.opened.as_ref().map(|o| o.sizes.clone()).unwrap_or_default();
        let s = self.settings.as_mut().expect("checked");
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("Output size")
                .selected_text(format!("{} px", s.master.size))
                .show_ui(ui, |ui| {
                    for n in sizes {
                        ui.selectable_value(&mut s.master.size, n, format!("{n} px"));
                    }
                });
            hint(ui, "Output size", HELP_SIZE);
        });
        let edges = |e: Edges| if e == Edges::Repeats { "Repeats" } else { "Does not repeat" };
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("Edges").selected_text(edges(s.master.edges)).show_ui(ui, |ui| {
                for e in [Edges::Repeats, Edges::DoesNotRepeat] {
                    ui.selectable_value(&mut s.master.edges, e, edges(e));
                }
            });
            hint(ui, "Edges", HELP_EDGES);
        });
        let Some(out) = &self.outputs else { return };
        let r = &out.report;
        if r.seam_hint {
            warn(ui, SEAM_HINT);
        }
        if let (Some(mean), Some(p95)) = (r.fit_mean_deg, r.fit_p95_deg) {
            let green = if r.green == Green::Opengl { "OpenGL" } else { "DirectX" };
            let how = match (r.green_guessed, r.green_sure) {
                (false, _) => format!("Green: {green}"),
                (true, true) => format!("Green: {green} (guessed)"),
                (true, false) => format!("Green: undetermined, using {green}"),
            };
            line(ui, &how);
            line(ui, &format!("Rebuilds the normal to within {mean:.1}° (95 % within {p95:.1}°)"));
        }
    }

    /// The thumbnail of `kind` from the newest outputs; the textures are made once per result.
    fn thumb(&mut self, ctx: &egui::Context, kind: MapKind) -> Option<egui::TextureHandle> {
        let out = self.outputs.as_ref()?;
        if self.thumbs.as_ref().is_none_or(|(id, _)| *id != out.id) {
            use MapKind::*;
            let made = [HeightData, Parallax, Normal, Ao, Curvature, Cavity]
                .into_iter()
                .filter_map(|k| {
                    let name = format!("thumb-{}", view2d::name(k));
                    Some((k, ctx.load_texture(name, view2d::thumb(out, k)?, egui::TextureOptions::LINEAR)))
                })
                .collect();
            self.thumbs = Some((out.id, made));
        }
        self.thumbs.as_ref()?.1.get(&kind).cloned()
    }

    /// One output's section: a rule, the header (collapse toggle, thumbnail, tick, help), then `always` (warnings and
    /// progress, drawn open or closed), then `body` when open.
    fn section(&mut self, ui: &mut egui::Ui, kind: MapKind, always: fn(&mut Tab, &mut egui::Ui), body: fn(&mut Tab, &mut egui::Ui)) {
        let name = view2d::name(kind);
        let thumb = self.thumb(ui.ctx(), kind);
        let tick = self.settings.as_mut().expect("checked").ticks.get_mut(kind);
        let was = *tick;
        ui.add_space(6.0);
        ui.separator();
        // Open the first time it is drawn when its map is ticked; egui keeps the state for the session.
        let id = ui.make_persistent_id(("section", name));
        let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, was);
        let toggle = ui
            .horizontal(|ui| {
                let toggle = state.show_toggle_button(ui, paint_default_icon);
                thumb_box(ui, name, thumb.as_ref());
                ui.checkbox(tick, egui::RichText::new(name).strong().size(15.0));
                hint(ui, name, purpose(kind));
                toggle
            })
            .inner;
        let on = *tick;
        if on && !was {
            state.set_open(true);
        }
        // The arrow has no accessible name of its own.
        let label = format!("{} {name} settings", if state.is_open() { "Hide" } else { "Show" });
        toggle.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
        always(self, ui);
        state.show_body_indented(&toggle, ui, |ui| {
            // Height data and Normal shape the master, so their rows are there ticked or not.
            if on || matches!(kind, MapKind::HeightData | MapKind::Normal) {
                body(self, ui);
                ui.add_space(4.0);
                let ready = on
                    && !self.running
                    && self.folder.is_some()
                    && self.saving.is_none()
                    && self.outputs.as_ref().is_some_and(|o| !o.draft && view2d::made(o, kind));
                if ui.add_enabled(ready, egui::Button::new(format!("Save {name}"))).clicked() {
                    self.save(Some(kind), ui.ctx());
                }
            } else {
                line(ui, &format!("Tick {name} to make this map."));
            }
        });
    }

    fn height_always(&mut self, ui: &mut egui::Ui) {
        let t = self.settings.as_ref().expect("checked").ticks;
        if !t.height && t.parallax {
            // A plain line, not a warning: unticked is how Height data starts.
            line(ui, NOT_SAVED);
        }
    }

    /// The Flatten controls and, with Compare on, its strip. Height shows them and so does Parallax, as one setting;
    /// `suffix` keeps the widget names of the second copy apart from the first's.
    fn flatten_rows(&mut self, ui: &mut egui::Ui, suffix: &str) {
        ui.push_id(suffix, |ui| {
            let s = self.settings.as_mut().expect("checked");
            let name = format!("Flatten broad shapes{suffix}");
            let row = Row {
                name: &name,
                range: 0.0..=1.0,
                default: 0.3,
                help: HELP_FLATTEN,
                scale: 1.0,
                suffix: "",
                decimals: 2,
                log: false,
            };
            let m = slider_row(ui, &row, &mut s.master.flatten);
            self.dragging |= m.dragging;
            ui.horizontal(|ui| {
                ui.small("Keep large forms");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.small("Fine surface detail"));
            });
            line(ui, &match fade_pct(s.master.flatten) {
                Some(p) => format!("Fades shapes wider than {p:.0} % of the width"),
                None => "Keeps every shape".into(),
            });
            // Wrapped: the second copy's longer names do not fit one row.
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_label(format!("Material{suffix}")).selected_text("Choose…").show_ui(ui, |ui| {
                    for (name, v) in PRESETS {
                        if ui.selectable_label(false, format!("{name} ({v})")).clicked() {
                            s.master.flatten = v;
                        }
                    }
                });
                if ui.selectable_label(self.compare, format!("Compare{suffix}")).clicked() {
                    self.compare = !self.compare;
                    self.sent = None; // the job must carry the new Compare state
                }
            });
            // Under each copy, so the strip appears next to the button that was pressed.
            if self.compare {
                self.compare_strip(ui, suffix);
            }
        });
    }

    fn height_rows(&mut self, ui: &mut egui::Ui) {
        let from_normal = self.opened.as_ref().is_some_and(|o| o.kind == Kind::Normal);
        let out = self.outputs.clone();
        if from_normal {
            self.flatten_rows(ui, "");
        }
        let s = self.settings.as_mut().expect("checked");
        if let Some(out) = &out {
            let pct = out.height.1 / f64::from(out.size.0.max(1)) * 100.0;
            line(ui, &format!("Black to white spans {:.1} texels ({pct:.1} % of width)", out.height.1));
        }
        if from_normal {
            egui::CollapsingHeader::new("Height advanced").id_salt("height-advanced").show(ui, |ui| {
                let name = |g: Option<Green>| match g {
                    None => "Guess",
                    Some(Green::Directx) => "DirectX",
                    Some(Green::Opengl) => "OpenGL",
                };
                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("Source green").selected_text(name(s.master.green)).show_ui(ui, |ui| {
                        for g in [None, Some(Green::Directx), Some(Green::Opengl)] {
                            ui.selectable_value(&mut s.master.green, g, name(g));
                        }
                    });
                    hint(ui, "Source green", HELP_GREEN_IN);
                });
            });
        }
    }

    fn compare_strip(&mut self, ui: &mut egui::Ui, suffix: &str) {
        let Some(out) = self.outputs.clone() else { return };
        let Some(three) = &out.compare else {
            ui.small("Compare appears with the next result.");
            return;
        };
        if self.compare_tex.as_ref().is_none_or(|(id, _)| *id != out.id) {
            let t = |i: usize| {
                ui.ctx().load_texture(format!("compare-{i}"), view2d::gray_image(&three[i]), egui::TextureOptions::LINEAR)
            };
            self.compare_tex = Some((out.id, [t(0), t(1), t(2)]));
        }
        let (_, tex) = self.compare_tex.as_ref().expect("set above");
        let s = self.settings.as_mut().expect("checked");
        let current = s.master.flatten;
        ui.horizontal(|ui| {
            for (i, (label, v)) in [("0", 0.0), ("Current", current), ("1", 1.0)].into_iter().enumerate() {
                ui.vertical(|ui| {
                    let img = egui::Image::new((tex[i].id(), egui::vec2(90.0, 90.0))).sense(egui::Sense::click());
                    let r = ui.add(img);
                    let name = format!("Use Flatten {label}{suffix}");
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
                    if r.clicked() {
                        s.master.flatten = v;
                    }
                    ui.small(label);
                });
            }
        });
    }

    fn parallax_always(&mut self, ui: &mut egui::Ui) {
        let s = self.settings.as_ref().expect("checked");
        if !s.ticks.parallax {
            return;
        }
        // Outside Parallax advanced, where the plain readout is, so few levels are seen with it closed.
        if let Some((_, r)) = self.outputs.as_ref().and_then(|o| o.parallax.as_ref()) {
            if r.levels_used < 60 {
                warn(ui, &format!("Only {} of 256 grey levels are used; raise Depth.", r.levels_used));
            }
        }
    }

    fn parallax_rows(&mut self, ui: &mut egui::Ui) {
        let from_normal = self.opened.as_ref().is_some_and(|o| o.kind == Kind::Normal);
        let out = self.outputs.clone();
        let p = &mut self.settings.as_mut().expect("checked").parallax;
        let row = Row {
            name: "Depth",
            range: 0.01..=1.0,
            default: 0.9,
            help: HELP_DEPTH,
            scale: 100.0,
            suffix: " %",
            decimals: 0,
            log: true,
        };
        slider_row(ui, &row, &mut p.depth);
        if let Some(out) = &out {
            if ui.button("Match the normal").clicked() {
                p.depth = out.match_depth;
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Flat level");
            hint(ui, "Flat level", if p.flat == FlatLevel::White { HELP_WHITE } else { HELP_MID_GREY });
        });
        ui.horizontal(|ui| {
            ui.radio_value(&mut p.flat, FlatLevel::MidGrey, "Mid-grey");
            ui.radio_value(&mut p.flat, FlatLevel::White, "White");
        });
        if from_normal {
            self.flatten_rows(ui, " (shared with Height)");
        }
        let p = &mut self.settings.as_mut().expect("checked").parallax;
        egui::CollapsingHeader::new("Parallax advanced").id_salt("parallax-advanced").show(ui, |ui| {
            let row = Row {
                name: "Smooth sharp steps",
                range: 0.0..=8.0,
                default: 0.0,
                help: HELP_SMOOTH,
                scale: 1.0,
                suffix: " texels",
                decimals: 1,
                log: false,
            };
            slider_row(ui, &row, &mut p.smooth_texels);
            ui.horizontal(|ui| {
                ui.checkbox(&mut p.invert, "Invert");
                hint(ui, "Invert", HELP_INVERT);
            });
            if let Some((_, r)) = out.as_ref().and_then(|o| o.parallax.as_ref()) {
                ui.add_space(6.0);
                small(ui, &format!("{:.1}× deeper than the normal implies", r.times_normal));
                small(ui, &format!("Levels used: {} of 256", r.levels_used));
                small(ui, &format!("Clipped: {:.1} %", r.clipped_share * 100.0));
            }
        });
    }

    fn normal_always(&mut self, ui: &mut egui::Ui) {
        if self.opened.as_ref().is_some_and(|o| o.kind == Kind::Height { span_texels: None }) {
            warn(ui, "Scale unknown");
        }
    }

    fn normal_rows(&mut self, ui: &mut egui::Ui) {
        let s = self.settings.as_mut().expect("checked");
        let row = Row {
            name: "Normal depth",
            range: 0.05..=5.0,
            default: 1.0,
            help: HELP_NORMAL_DEPTH,
            scale: 1.0,
            suffix: " % of width",
            decimals: 2,
            log: true,
        };
        let m = slider_row(ui, &row, &mut s.master.normal_depth_pct);
        self.dragging |= m.dragging;
        let name = |g: Green| if g == Green::Opengl { "OpenGL" } else { "DirectX" };
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("Green direction").selected_text(name(s.normal_green)).show_ui(ui, |ui| {
                for g in [Green::Directx, Green::Opengl] {
                    ui.selectable_value(&mut s.normal_green, g, name(g));
                }
            });
            hint(ui, "Green direction", HELP_GREEN);
        });
    }

    fn ao_always(&mut self, ui: &mut egui::Ui) {
        if self.running && self.settings.as_ref().is_some_and(|s| s.ticks.ao) {
            let p = self.progress.as_ref().map_or(0, |p| p.load(Ordering::Relaxed));
            ui.add(egui::ProgressBar::new(p as f32 / 1000.0).text("Making AO…"));
        }
    }

    fn ao_rows(&mut self, ui: &mut egui::Ui) {
        let width = self.master_width();
        let s = self.settings.as_mut().expect("checked");
        let row = Row {
            name: "Reach",
            range: 0.015625..=0.25,
            default: 0.0625,
            help: HELP_REACH,
            scale: 100.0,
            suffix: " %",
            decimals: 2,
            log: false,
        };
        let m = slider_row(ui, &row, &mut s.ao.reach);
        self.dragging |= m.dragging;
        line(ui, &format!("About {:.0} texels", s.ao.reach * width as f32));
        let row = Row {
            name: "AO strength",
            range: 0.5..=3.0,
            default: 1.0,
            help: HELP_DARKNESS,
            scale: 1.0,
            suffix: "",
            decimals: 2,
            log: false,
        };
        slider_row(ui, &row, &mut s.ao.darkness);
        let m = slider_row(ui, &surface_depth_row("Surface depth"), &mut s.ao.surface_depth);
        self.dragging |= m.dragging;
        for l in AO_LINES {
            line(ui, l);
        }
    }

    fn curvature_rows(&mut self, ui: &mut egui::Ui) {
        let s = self.settings.as_mut().expect("checked");
        let row = Row {
            name: "Line thickness",
            range: 1.0..=8.0,
            default: 2.0,
            help: HELP_LINE,
            scale: 1.0,
            suffix: " texels",
            decimals: 1,
            log: false,
        };
        slider_row(ui, &row, &mut s.curvature.line_texels);
        let row = Row {
            name: "Contrast",
            range: 0.5..=2.0,
            default: 1.0,
            help: HELP_CONTRAST,
            scale: 1.0,
            suffix: "",
            decimals: 2,
            log: false,
        };
        slider_row(ui, &row, &mut s.curvature.contrast);
    }

    fn cavity_rows(&mut self, ui: &mut egui::Ui) {
        let width = self.master_width();
        let s = self.settings.as_mut().expect("checked");
        let row = Row {
            name: "Crevice width",
            range: 2.0..=16.0,
            default: default_crevice(width),
            help: HELP_CREVICE,
            scale: 1.0,
            suffix: " texels",
            decimals: 1,
            log: false,
        };
        let mut v = s.crevice_at(width);
        let m = slider_row(ui, &row, &mut v);
        if m.reset {
            s.crevice = None;
        } else if v != s.crevice_at(width) {
            s.crevice = Some(v);
        }
        egui::CollapsingHeader::new("Cavity advanced").id_salt("cavity-advanced").show(ui, |ui| {
            let m = slider_row(ui, &surface_depth_row("Surface depth (shared with AO)"), &mut s.ao.surface_depth);
            self.dragging |= m.dragging;
        });
    }

    /// The master's width: the newest outputs', or the chosen size before any.
    fn master_width(&self) -> u32 {
        match (&self.outputs, &self.settings) {
            (Some(o), _) => o.size.0,
            (None, Some(s)) => s.master.size,
            _ => 1024,
        }
    }

    fn ticked(&self) -> usize {
        let Some(s) = &self.settings else { return 0 };
        let t = s.ticks;
        let normal = t.normal && self.opened.as_ref().is_some_and(|o| o.kind != Kind::Normal);
        [t.height, t.parallax, normal, t.ao, t.curvature, t.cavity].iter().filter(|&&b| b).count()
    }

    fn save_rows(&mut self, ui: &mut egui::Ui) {
        folder_row(ui, &mut self.folder);
        let n = self.ticked();
        let ready = self.outputs.as_ref().is_some_and(|o| !o.draft)
            && !self.running
            && n > 0
            && self.folder.is_some()
            && self.saving.is_none();
        let label = match (self.outputs.is_some(), n) {
            (true, 1) => "Save 1 map".to_string(),
            (true, n) if n > 0 => format!("Save {n} maps"),
            _ => "Save".to_string(),
        };
        if ui.add_enabled(ready, egui::Button::new(label)).clicked() {
            self.save(None, ui.ctx());
        }
        save::status_row(ui, self.saving.is_some(), &self.status);
    }

    /// The files this tab has open, with one still being read.
    pub fn open_files(&self) -> Vec<PathBuf> {
        self.opened.iter().map(|o| &o.path).chain(self.opening.iter().map(|(p, _)| p)).cloned().collect()
    }

    /// Saves every ticked map, or only the map of kind `only`, on a thread (`save::start`). The files are written at
    /// once when nothing on disk needs a choice; otherwise the dialog opens when the thread answers.
    fn save(&mut self, only: Option<MapKind>, ctx: &egui::Context) {
        let (Some(o), Some(s), Some(out), Some(f)) = (&self.opened, &self.settings, &self.outputs, &self.folder) else {
            return;
        };
        let (o, s, out, f) = (o.clone(), s.clone(), out.clone(), f.clone());
        let sources = [self.open_files(), self.elsewhere.clone()].concat();
        self.status = None;
        self.saving = Some(save::start(ctx, sources, says(f.clone()), move || {
            let mut plan = save::plan(&o, &s, &out, &f);
            plan.retain(|(k, _)| only.is_none_or(|only| *k == only));
            plan
        }));
    }

    fn review_dialog(&mut self, ctx: &egui::Context) {
        let Some(r) = &mut self.review else { return };
        match save::dialog(ctx, r) {
            Some(true) => {
                let r = self.review.take().expect("the dialog is open");
                // All or nothing; an `Err` names the failing file and says nothing was changed.
                self.saving = Some(save::start_write(ctx, r, [self.open_files(), self.elsewhere.clone()].concat()));
            }
            Some(false) => self.review = None,
            None => {}
        }
    }
}

/// The status line after a write into `f`.
fn says(f: PathBuf) -> save::Say {
    Box::new(move |written, kept| match written {
        0 if kept > 0 => format!("Nothing saved: the files in {} were kept", f.display()),
        0 => format!("Nothing to save: the files in {} already match", f.display()),
        1 => format!("Saved 1 map to {}", f.display()),
        n => format!("Saved {n} maps to {}", f.display()),
    })
}

fn surface_depth_row(name: &str) -> Row<'_> {
    Row {
        name,
        range: 0.25..=4.0,
        default: 1.0,
        help: HELP_SURFACE_DEPTH,
        scale: 1.0,
        suffix: "",
        decimals: 2,
        log: false,
    }
}
