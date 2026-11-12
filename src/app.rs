//! The window: the active tab's column on the left, preview controls on the right, the 3D or 2D view in the centre.
use crate::cube::tab::CubeTab;
use crate::from_normal::maps::FlatLevel;
use crate::marker::MapKind;
use crate::preview::{Cmd, Content, Frame, Reply, View};
use crate::preview_maps;
use crate::rnm::tab::RnmTab;
use crate::tab::{Tab, COLUMN_WIDTH};
use crate::view2d::{self, View2d};
use eframe::egui;
use render3d::scene::{AUTO_PITCH, AUTO_YAW};
use render3d::Builtin;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};

/// The accessible name of the frame.
pub const FRAME_LABEL: &str = "3D preview";
/// Beside the Cubemap tab's 3D switch: True PBR never reads this cube, so the ball is a vanilla material.
const CUBE_BALL: &str = "Vanilla environment map, not True PBR";
const PANEL_WIDTH: f32 = 280.0;
/// The From normal column's narrowest width, and the centre's.
const COLUMN_MIN: f32 = 300.0;
const CENTRE_MIN: f32 = 400.0;
/// Pixels drawn while the camera moves. Spike 1 timed 1.6 MP at 114 to 137 pictures a second.
pub const MOVING_PIXELS: f32 = 2_000_000.0;
/// The camera counts as resting this long after its last change.
const REST_SECONDS: f64 = 0.15;

/// What the last frame on screen cost, for the panel and the timing run.
#[derive(Clone, Copy, Default)]
pub struct FrameCost {
    pub render_ms: f64,
    /// From asking for the frame to its texture being set.
    pub latency_ms: f64,
}

/// Which tab's column is shown and feeds the preview.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    FromNormal,
    Rnm,
    Cube,
}

pub struct App {
    tx: Sender<Cmd>,
    rx: Receiver<Reply>,
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub form: Builtin,
    pub displacement: f32,
    /// The view of the frame being rendered or last rendered.
    sent: Option<View>,
    last_camera: Option<[f32; 3]>,
    moved_at: Option<f64>,
    /// [`MOVING_PIXELS`]; a timing run can lift it.
    pub moving_pixels: f32,
    in_flight: bool,
    texture: Option<egui::TextureHandle>,
    /// Build name and short fingerprint, shown so a preview can be tied to its shaders.
    pub shaders_label: String,
    adapter: Option<(String, bool)>,
    pub error: Option<String>,
    pub cost: FrameCost,
    /// Frames put on screen and frames asked for, counted for the timing run and tests.
    pub shown: u64,
    pub asked: u64,
    /// Device recoveries and the last one's duration.
    pub recoveries: u64,
    pub recovery_ms: f64,
    /// When set, a copy of the last frame, so a run can compare frames. Off in normal use.
    pub keep_pixels: bool,
    pub last_pixels: Option<egui::ColorImage>,
    /// This process is a restart after a lost graphics device: say so, or the window just blinked for no reason.
    pub restarted: bool,
    pub tab: Tab,
    pub rnm: RnmTab,
    pub cube: CubeTab,
    /// The labelled cross's texture, dropped when a new cube arrives.
    cube_texture: Option<egui::TextureHandle>,
    pub mode: Mode,
    /// The centre shows the 2D view instead of the 3D preview.
    flat: bool,
    /// The Cubemap tab's own choice. It starts on the labelled cross, which its help texts point at.
    cube_flat: bool,
    view2d: View2d,
    /// The RNM tab's own 2D choice, so switching tabs keeps From normal's.
    view2d_rnm: View2d,
    /// The 2D view's textures, dropped when new outputs arrive.
    textures: HashMap<MapKind, egui::TextureHandle>,
    /// The DDS bytes last sent to the preview, by slot.
    slots: Option<HashMap<&'static str, Vec<u8>>>,
    /// What the preview was last told about the `p` map's flat level.
    white: bool,
}

fn form_name(f: Builtin) -> &'static str {
    match f {
        Builtin::Sphere => "Sphere",
        Builtin::Plane => "Plane",
        Builtin::Cube => "Cube",
        Builtin::MaterialBall => "Material ball",
    }
}

impl App {
    pub fn new(tx: Sender<Cmd>, rx: Receiver<Reply>, shaders_label: String, tab: Tab, rnm: RnmTab, cube: CubeTab) -> App {
        App {
            tx,
            rx,
            yaw: AUTO_YAW,
            pitch: AUTO_PITCH,
            zoom: 1.0,
            form: Builtin::MaterialBall,
            displacement: 1.0,
            sent: None,
            last_camera: None,
            moved_at: None,
            moving_pixels: MOVING_PIXELS,
            in_flight: false,
            texture: None,
            shaders_label,
            adapter: None,
            error: None,
            cost: FrameCost::default(),
            shown: 0,
            asked: 0,
            recoveries: 0,
            recovery_ms: 0.0,
            keep_pixels: false,
            last_pixels: None,
            restarted: false,
            tab,
            rnm,
            cube,
            cube_texture: None,
            mode: Mode::FromNormal,
            flat: false,
            cube_flat: true,
            view2d: View2d::default(),
            view2d_rnm: View2d { map: MapKind::Rnm, repeat: false },
            textures: HashMap::new(),
            slots: None,
            white: false,
        }
    }

    /// The Material tab's parallax map rests on white.
    fn white(&self) -> bool {
        // The newest settings, not the ones behind the outputs: a job in between corrects it when it lands.
        let on = |s: &crate::from_normal::settings::Settings| s.ticks.parallax && s.parallax.flat == FlatLevel::White;
        self.mode == Mode::FromNormal && self.tab.settings.as_ref().is_some_and(on)
    }

    /// The active tab's maps for the preview, when it has outputs.
    fn active_maps(&self) -> Option<HashMap<&'static str, Vec<u8>>> {
        // shortcut: the DDS encode runs on the UI thread; move it to the generation thread if 2048 updates stutter.
        match self.mode {
            Mode::FromNormal => self.tab.outputs.as_ref().map(|o| preview_maps::maps_for(o)),
            Mode::Rnm => self.rnm.outputs.as_ref().map(|o| preview_maps::normal_only(o.size, &o.px)),
            Mode::Cube => self.cube.outputs.as_ref().map(|o| HashMap::from([("e", crate::cube::files::preview_dds(&o.cube))])),
        }
    }

    /// The active tab shows its 2D view.
    fn is_flat(&self) -> bool {
        if self.mode == Mode::Cube {
            self.cube_flat
        } else {
            self.flat
        }
    }

    /// New outputs to the preview: a reload when the set of slots changes, otherwise a swap per changed map. A tab
    /// with no outputs yet shows the flat ball, as at start, never the other tab's maps.
    fn push_maps(&mut self) {
        let maps = self.active_maps().unwrap_or_default();
        let white = self.white();
        match &self.slots {
            Some(old) if old.len() == maps.len() && maps.keys().all(|k| old.contains_key(k)) => {
                if white != self.white {
                    self.send(Cmd::SetWhite(white));
                }
                for (k, b) in &maps {
                    if old[k] != *b {
                        self.send(Cmd::SwapMap(k, b.clone()));
                    }
                }
            }
            _ => self.send(Cmd::SetContent(Content { form: self.form, displacement: self.displacement, maps: maps.clone(), white })),
        }
        self.slots = Some(maps);
        self.white = white;
    }

    pub fn send(&self, cmd: Cmd) {
        // A dead thread is reported by `drain`.
        let _ = self.tx.send(cmd);
    }

    /// For work that finishes off the UI thread and then sends its result.
    pub fn sender(&self) -> Sender<Cmd> {
        self.tx.clone()
    }

    pub fn adapter(&self) -> Option<&str> {
        self.adapter.as_ref().map(|a| a.0.as_str())
    }

    /// Size of the frame last asked for, in pixels.
    pub fn last_view_label(&self) -> String {
        self.sent.map_or(String::new(), |v| format!("{}x{}", v.w, v.h))
    }

    fn show_frame(&mut self, ctx: &egui::Context, f: Frame) {
        self.in_flight = false;
        self.error = None;
        self.shown += 1;
        self.cost = FrameCost {
            render_ms: f.render_ms,
            latency_ms: f.asked.elapsed().as_secs_f64() * 1e3,
        };
        if self.keep_pixels {
            self.last_pixels = Some(f.image.clone());
        }
        match &mut self.texture {
            Some(t) => t.set(f.image, egui::TextureOptions::LINEAR),
            None => self.texture = Some(ctx.load_texture("preview", f.image, egui::TextureOptions::LINEAR)),
        }
    }

    /// Takes every reply queued. Only the newest frame reaches the screen.
    fn drain(&mut self, ctx: &egui::Context) {
        let mut newest = None;
        loop {
            match self.rx.try_recv() {
                Ok(Reply::Ready { adapter, warp, fingerprint }) => {
                    self.adapter = Some((adapter, warp));
                    let short = fingerprint.get(..12).unwrap_or(&fingerprint);
                    self.shaders_label = format!("{} ({short})", self.shaders_label);
                }
                Ok(Reply::Frame(f)) => newest = Some(f),
                Ok(Reply::Recovered { adapter, warp, ms }) => {
                    self.adapter = Some((adapter, warp));
                    self.recoveries += 1;
                    self.recovery_ms = ms;
                }
                Ok(Reply::Failed(e)) => {
                    self.in_flight = false;
                    self.error = Some(e);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.in_flight = false;
                    self.error.get_or_insert_with(|| "The preview stopped. Restart Texalith.".into());
                    break;
                }
            }
        }
        if let Some(f) = newest {
            self.show_frame(ctx, f);
        }
    }

    pub fn draw(&mut self, ui: &mut egui::Ui) {
        self.drain(ui.ctx());
        // Every tab drains every frame; only the active tab's new outputs reach the preview.
        let (from_normal, rnm, cube) = (self.tab.drain(), self.rnm.drain(), self.cube.drain());
        if cube {
            self.cube_texture = None;
        }
        if (from_normal && self.mode == Mode::FromNormal) || (rnm && self.mode == Mode::Rnm) || (cube && self.mode == Mode::Cube) {
            self.textures.clear();
            self.push_maps();
        }
        // The column's maximum leaves the controls their width and the centre its minimum (egui-ui-rules 2.3).
        let max = (ui.available_width() - PANEL_WIDTH - CENTRE_MIN).max(COLUMN_MIN);
        let before = self.mode;
        egui::Panel::left("column")
            .resizable(true)
            .default_size(COLUMN_WIDTH)
            .size_range(COLUMN_MIN..=max)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.mode, Mode::FromNormal, "Material");
                    ui.selectable_value(&mut self.mode, Mode::Rnm, "RNM");
                    ui.selectable_value(&mut self.mode, Mode::Cube, "Cubemap");
                });
                match before {
                    Mode::FromNormal => self.tab.draw_column(ui),
                    Mode::Rnm => self.rnm.draw_column(ui),
                    Mode::Cube => self.cube.draw_column(ui),
                }
            });
        if self.mode != before {
            // A reload with the new tab's maps, so no swap lands on the other tab's slots.
            self.textures.clear();
            self.slots = None;
            self.push_maps();
        }
        egui::Panel::right("controls")
            .resizable(false)
            .exact_size(PANEL_WIDTH)
            .show(ui, |ui| self.controls(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.centre(ui));
    }

    fn centre(&mut self, ui: &mut egui::Ui) {
        let cube = self.mode == Mode::Cube;
        let flat = if cube { &mut self.cube_flat } else { &mut self.flat };
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.selectable_value(flat, false, "3D");
            ui.selectable_value(flat, true, "2D");
            if cube && !*flat {
                ui.label(CUBE_BALL);
            }
        });
        if cube && self.cube.outputs.is_none() {
            // In either view: without a cube the ball would be the plain True PBR one.
            // A source open with no type picked (every square image) is waiting on the user, not a file.
            let text = match (&self.cube.opened, self.cube.settings.source) {
                (Some(_), None) => "Pick a Source type to see its faces here.",
                _ => "Open a source to see its faces here.",
            };
            ui.add_space(16.0);
            ui.vertical_centered(|ui| ui.label(text));
            return;
        }
        if !self.is_flat() {
            return self.viewport(ui);
        }
        if let (true, Some(o)) = (cube, self.cube.outputs.clone()) {
            return crate::cube::view::draw(ui, &o.cube, &mut self.cube_texture);
        }
        let empty = match (self.mode, self.tab.outputs.clone(), self.rnm.outputs.clone()) {
            (Mode::FromNormal, Some(o), _) => {
                let image = |k| view2d::image(&o, k);
                view2d::draw(ui, &mut self.view2d, o.size, &view2d::kinds(&o), &image, &mut self.textures);
                None
            }
            (Mode::Rnm, _, Some(o)) => {
                let image = |_| view2d::rgb_image(o.size, &o.px);
                view2d::draw(ui, &mut self.view2d_rnm, o.size, &[MapKind::Rnm], &image, &mut self.textures);
                None
            }
            (Mode::FromNormal, None, _) => Some("Open a normal or height map to see its maps here."),
            (Mode::Rnm, _, None) => Some("Open a base normal to see the blend here."),
            (Mode::Cube, _, _) => unreachable!("the Cubemap centre returned above"), // the cross, or its empty line
        };
        if let Some(text) = empty {
            ui.add_space(16.0);
            ui.vertical_centered(|ui| ui.label(text));
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        ui.add_space(8.0);
        ui.heading("Preview");
        ui.add_space(4.0);
        // These act on the 3D view only; the 2D views have none.
        if !self.is_flat() {
            let before = self.form;
            egui::ComboBox::from_label("Form")
                .selected_text(form_name(self.form))
                .show_ui(ui, |ui| {
                    for f in Builtin::ALL {
                        ui.selectable_value(&mut self.form, f, form_name(f));
                    }
                });
            if self.form != before {
                self.send(Cmd::SetForm(self.form));
            }
            ui.add_space(4.0);
            // The cube's ball has no parallax map.
            if self.mode != Mode::Cube {
                let slider = ui.add(egui::Slider::new(&mut self.displacement, 0.0..=3.0).text("Parallax scale"));
                if slider.changed() {
                    self.send(Cmd::SetDisplacement(self.displacement));
                }
                ui.scope(|ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    ui.small("Preview only. How deep the parallax map looks here. It is not saved.");
                });
                ui.add_space(4.0);
            }
            if ui.button("Reset view").clicked() {
                self.reset_view();
            }
            ui.separator();
            ui.scope(|ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                ui.small("Drag to orbit, scroll to zoom, double-click to reset.");
            });
            ui.separator();
        }
        ui.small("Community Shaders");
        ui.add(egui::Label::new(egui::RichText::new(&self.shaders_label).small()).truncate());
        if let Some((name, warp)) = &self.adapter {
            ui.small("GPU");
            ui.add(egui::Label::new(egui::RichText::new(name).small()).truncate());
            if *warp {
                let warn = ui.visuals().warn_fg_color;
                ui.colored_label(warn, "Software rendering: the preview will be slow.");
            }
        }
        if !self.is_flat() && self.shown > 0 {
            ui.small(format!("Last frame {:.1} ms", self.cost.render_ms));
        }
        if self.restarted {
            let warn = ui.visuals().warn_fg_color;
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            ui.colored_label(
                warn,
                "The graphics device was lost, so Texalith restarted. Open your file again; maps that were not saved are gone.",
            );
        }
    }

    fn reset_view(&mut self) {
        self.yaw = AUTO_YAW;
        self.pitch = AUTO_PITCH;
        self.zoom = 1.0;
    }

    fn viewport(&mut self, ui: &mut egui::Ui) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, FRAME_LABEL));
        // Same gestures and rates as Tessalith's viewport.
        if resp.double_clicked() {
            self.reset_view();
        } else if resp.dragged() {
            let d = resp.drag_delta();
            self.yaw -= d.x * 0.01;
            self.pitch = (self.pitch + d.y * 0.01).clamp(-1.52, 1.52);
        }
        if resp.hovered() {
            let s = ui.input(|i| i.smooth_scroll_delta.y);
            if s != 0.0 {
                self.zoom = (self.zoom * (-s * 0.002).exp()).clamp(0.2, 10.0);
            }
        }
        self.ask(ui.ctx(), rect);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::BLACK);
        if let Some(t) = &self.texture {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(t.id(), rect, uv, egui::Color32::WHITE);
        }
        if let Some(e) = &self.error {
            let color = ui.visuals().error_fg_color;
            let inner = egui::UiBuilder::new().max_rect(rect.shrink(16.0));
            ui.scope_builder(inner, |ui| {
                ui.colored_label(color, format!("The preview cannot draw this: {e}"));
            });
        } else if self.texture.is_none() {
            let inner = egui::UiBuilder::new().max_rect(rect.shrink(16.0));
            ui.scope_builder(inner, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Compiling shaders…");
                });
            });
        }
    }

    /// One frame at a time: a view that changes while one renders waits for that frame's reply, then goes out.
    fn ask(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        let ppp = ctx.pixels_per_point();
        let (w, h) = (rect.width() * ppp, rect.height() * ppp);
        // Fewer pixels while the camera moves, the full size once it rests: at 2.75x display scaling a normal window
        // is 6 MP, which reads back at 28 pictures a second (spike 2).
        let now = ctx.input(|i| i.time);
        let camera = [self.yaw, self.pitch, self.zoom];
        if self.last_camera != Some(camera) {
            if self.last_camera.is_some() {
                self.moved_at = Some(now);
            }
            self.last_camera = Some(camera);
        }
        let moving = self.moved_at.is_some_and(|t| now - t < REST_SECONDS);
        if moving {
            ctx.request_repaint_after_secs(REST_SECONDS as f32);
        }
        let scale = if moving { (self.moving_pixels / (w * h).max(1.0)).sqrt().min(1.0) } else { 1.0 };
        let view = View {
            yaw: self.yaw,
            pitch: self.pitch,
            zoom: self.zoom,
            w: (w * scale).round() as u32,
            h: (h * scale).round() as u32,
        };
        if view.w == 0 || view.h == 0 || self.in_flight || self.sent == Some(view) {
            return;
        }
        self.sent = Some(view);
        self.in_flight = true;
        self.asked += 1;
        self.send(Cmd::Render(view));
    }
}
