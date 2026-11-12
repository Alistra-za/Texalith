//! Spike 2: drives the real Texalith window by itself and times it. Opens a window for about a minute, then closes.
//! Run with: cargo run --release --example window_timing -- <out dir> [--no-vsync] [--no-cap] [--ppp <n>]
//!           [--soak <seconds> | --lose-wgpu]
//! --no-cap renders full size while the camera moves. --ppp overrides the display scale, in physical pixels per point.
//! Writes frames.csv (one row per UI frame), summary.txt and a screenshot per window size.
//! --soak orbits at 1600x1000 for that long, to see whether the wgpu device stays alive.
//! --lose-wgpu destroys egui's wgpu device after the first picture, which affects this process only, and checks the
//! watcher catches it and restarts: the restart writes restarted.txt to the out dir and exits.
use eframe::egui;
use eframe::egui_wgpu::wgpu;
use render3d::scene::AUTO_YAW;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use texalith::app::App;
use texalith::preview::Cmd;
use texalith::{bundle, preview, restart, testmaps};

/// In points, at the display's own scale: these fit the owner's 3840x2160 screen at 2.75x, the last one being about
/// a maximised window.
const NATIVE_SIZES: [(f32, f32); 3] = [(1100.0, 720.0), (1280.0, 760.0), (1396.0, 760.0)];
/// At 1 pixel per point these are spike 1's window sizes in physical pixels.
const PIXEL_SIZES: [(f32, f32); 4] = [(1100.0, 720.0), (1280.0, 800.0), (1600.0, 1000.0), (2560.0, 1440.0)];
const ORBIT_FRAMES: u32 = 300;
const IDLE_SECONDS: f64 = 2.0;
const SWAPS: u32 = 12;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Step {
    FirstFrame,
    Resize(usize),
    Screenshot(usize),
    Orbit(usize),
    Idle,
    SwapLoad(usize),
    Swap(usize),
    Recover,
    Soak,
    Destroy,
    Done,
}

const SWAP_MAPS: [u32; 3] = [1024, 2048, 4096];

// Read by a heartbeat thread, so a stalled UI thread still shows where it stopped.
static UI_FRAMES: AtomicU64 = AtomicU64::new(0);
static IN_UI: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<String> = Mutex::new(String::new());

fn heartbeat() {
    let start = Instant::now();
    std::thread::spawn(move || {
        let mut before = 0;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let frames = UI_FRAMES.load(Ordering::Relaxed);
            let state = STATE.lock().map(|s| s.clone()).unwrap_or_default();
            eprintln!(
                "[{:>5.0} s] UI frames +{:<4} in ui: {} | {state}",
                start.elapsed().as_secs_f64(),
                frames - before,
                IN_UI.load(Ordering::Relaxed)
            );
            before = frames;
        }
    });
}

struct Sample {
    step: String,
    dt_ms: f64,
    cpu_ms: f64,
    render_ms: f64,
    latency_ms: f64,
    new_frame: bool,
    view: String,
}

struct Driver {
    app: App,
    out: PathBuf,
    step: Step,
    /// UI frames since this step began.
    frames: u32,
    began: Instant,
    last_ui: Instant,
    shown_at_step: u64,
    asked_at_step: u64,
    last_shown: u64,
    samples: Vec<Sample>,
    summary: String,
    swap_bytes: Option<Arc<(Vec<u8>, Vec<u8>)>>,
    swap_sent: u32,
    swap_waiting_for: u64,
    pixels_before: Option<egui::ColorImage>,
    recover_sent: bool,
    recoveries_before: u64,
    soak_seconds: Option<f64>,
    lose_wgpu: bool,
    sizes: Vec<(f32, f32)>,
    ppp: Option<f32>,
}

impl Driver {
    fn go(&mut self, step: Step) {
        self.step = step;
        self.frames = 0;
        self.began = Instant::now();
        self.shown_at_step = self.app.shown;
        self.asked_at_step = self.app.asked;
    }

    fn step_name(&self) -> String {
        format!("{:?}", self.step)
    }

    fn line(&mut self, text: String) {
        println!("{text}");
        writeln!(self.summary, "{text}").unwrap();
    }

    /// One row per kind of number, over this step's samples, then a line in the summary.
    fn close_orbit(&mut self, name: &str, ctx: &egui::Context) {
        let step = self.step_name();
        let rows: Vec<&Sample> = self.samples.iter().filter(|s| s.step == step).collect();
        let pick = |f: &dyn Fn(&Sample) -> f64, only_new: bool| {
            let mut v: Vec<f64> = rows.iter().filter(|s| !only_new || s.new_frame).map(|s| f(s)).collect();
            v.sort_by(f64::total_cmp);
            if v.is_empty() {
                (0.0, 0.0)
            } else {
                (v[v.len() / 2], v[(v.len() * 95 / 100).min(v.len() - 1)])
            }
        };
        let secs = self.began.elapsed().as_secs_f64();
        let (dt, dt95) = pick(&|s| s.dt_ms, false);
        let (cpu, cpu95) = pick(&|s| s.cpu_ms, false);
        let (ren, ren95) = pick(&|s| s.render_ms, true);
        let (lat, lat95) = pick(&|s| s.latency_ms, true);
        let shown = self.app.shown - self.shown_at_step;
        let view = rows.last().map_or(String::new(), |s| s.view.clone());
        let ppp = ctx.pixels_per_point();
        self.line(format!(
            "{name}: frame {view} at {ppp} ppp | UI frame {dt:.1} ms (p95 {dt95:.1}) = {:.0} fps | egui CPU {cpu:.1} (p95 {cpu95:.1}) | render {ren:.1} (p95 {ren95:.1}) | ask-to-texture {lat:.1} (p95 {lat95:.1}) | new pictures {:.0}/s",
            1e3 / dt.max(1e-3),
            shown as f64 / secs,
        ));
    }

    fn drive(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let now = Instant::now();
        let dt_ms = now.duration_since(self.last_ui).as_secs_f64() * 1e3;
        self.last_ui = now;
        self.frames += 1;
        let new_frame = self.app.shown != self.last_shown;
        self.last_shown = self.app.shown;
        self.samples.push(Sample {
            step: self.step_name(),
            dt_ms,
            cpu_ms: f64::from(frame.info().cpu_usage.unwrap_or(0.0)) * 1e3,
            render_ms: self.app.cost.render_ms,
            latency_ms: self.app.cost.latency_ms,
            new_frame,
            view: self.app.last_view_label(),
        });

        match self.step {
            Step::FirstFrame => {
                if let (1, Some(ppp), Some(native)) = (self.frames, self.ppp, ctx.native_pixels_per_point()) {
                    ctx.set_zoom_factor(ppp / native);
                }
                if self.app.shown > 0 {
                    let startup = self.began.elapsed().as_secs_f64();
                    self.line(format!("startup: first picture {startup:.2} s after the window's first paint"));
                    let wgpu_info = frame.wgpu_render_state().map(|s| s.adapter.get_info());
                    let backend = wgpu_info.map_or("none".to_string(), |i| format!("{} on {:?}", i.name, i.backend));
                    let d3d = self.app.adapter().unwrap_or("?").to_string();
                    self.line(format!("egui's wgpu: {backend}; render3d D3D11: {d3d}"));
                    if self.lose_wgpu {
                        self.go(Step::Destroy);
                    } else if self.soak_seconds.is_some() {
                        self.go(Step::Soak);
                    } else {
                        self.go(Step::Resize(0));
                    }
                }
                if let Some(e) = self.app.error.clone() {
                    self.line(format!("FAILED: {e}"));
                    self.go(Step::Done);
                }
            }
            Step::Resize(i) => {
                if self.frames == 1 {
                    let monitor = ctx.input(|s| s.viewport().monitor_size);
                    self.line(format!("window {:?} on monitor {monitor:?}", self.sizes[i]));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(self.sizes[i].into()));
                }
                // Long enough for the resize and for a frame of the new size to come back.
                if self.frames > 40 && self.began.elapsed().as_secs_f64() > 0.7 {
                    self.go(Step::Screenshot(i));
                }
            }
            Step::Screenshot(i) => {
                if self.frames == 1 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                }
                let shot = ctx.input(|s| {
                    s.events.iter().find_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(Arc::clone(image)),
                        _ => None,
                    })
                });
                if let Some(image) = shot {
                    let [w, h] = image.size;
                    let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                    let path = self.out.join(format!("window_{w}x{h}.png"));
                    image::save_buffer(path, &rgba, w as u32, h as u32, image::ColorType::Rgba8).unwrap();
                    self.go(Step::Orbit(i));
                } else if self.frames > 120 {
                    self.line("no screenshot came back".into());
                    self.go(Step::Orbit(i));
                }
            }
            Step::Orbit(i) => {
                let t = self.frames as f32;
                self.app.yaw = AUTO_YAW + 0.5 * (t * 0.05).sin();
                self.app.pitch = 0.25 + 0.15 * (t * 0.03).sin();
                if self.frames >= ORBIT_FRAMES {
                    self.close_orbit(&format!("orbit {:.0}x{:.0}", self.sizes[i].0, self.sizes[i].1), ctx);
                    self.go(if i + 1 < self.sizes.len() { Step::Resize(i + 1) } else { Step::Idle });
                }
            }
            Step::Idle => {
                if self.began.elapsed().as_secs_f64() >= IDLE_SECONDS {
                    let (ui, asked) = (self.frames, self.app.asked - self.asked_at_step);
                    self.line(format!("idle {IDLE_SECONDS} s: {ui} UI frames, {asked} pictures asked for"));
                    self.go(Step::SwapLoad(0));
                }
            }
            Step::SwapLoad(i) => {
                if self.frames == 1 {
                    // On the UI thread on purpose: a synthetic prelude, kept out of the numbers by this step.
                    let s = SWAP_MAPS[i];
                    let maps = testmaps::maps(s, false);
                    let other: Vec<[u8; 4]> = vec![[128, 128, 128, 255]; (s * s) as usize];
                    self.swap_bytes = Some(Arc::new((maps["p"].clone(), testmaps::dds(s, &other, false))));
                    self.app.send(Cmd::SetContent(preview::Content {
                        form: self.app.form,
                        displacement: self.app.displacement,
                        maps,
                        white: false,
                    }));
                }
                if self.frames > 60 {
                    self.swap_sent = 0;
                    self.swap_waiting_for = self.app.shown;
                    self.go(Step::Swap(i));
                }
            }
            Step::Swap(i) => {
                if self.app.shown >= self.swap_waiting_for {
                    if self.swap_sent == SWAPS {
                        let name = format!("map swap {} (parallax map)", SWAP_MAPS[i]);
                        self.close_orbit(&name, ctx);
                        self.go(if i + 1 < SWAP_MAPS.len() { Step::SwapLoad(i + 1) } else { Step::Recover });
                    } else {
                        let pair = Arc::clone(self.swap_bytes.as_ref().unwrap());
                        let even = self.swap_sent.is_multiple_of(2);
                        self.swap_sent += 1;
                        self.swap_waiting_for = self.app.shown + 1;
                        // Off the UI thread, as generation will be: copying a 4K map here would count as UI time.
                        let tx = self.app.sender();
                        std::thread::spawn(move || {
                            let bytes = if even { pair.1.clone() } else { pair.0.clone() };
                            let _ = tx.send(Cmd::SwapMap("p", bytes));
                        });
                    }
                }
            }
            Step::Recover => {
                if !self.recover_sent {
                    self.app.keep_pixels = true;
                    // The nudge asks for a kept frame. Wait out the rest period, so the frame compared is the full
                    // size one the recovery will redraw, not a reduced moving one.
                    if self.frames == 1 {
                        self.app.yaw += 0.001;
                    }
                    let rested = self.began.elapsed().as_secs_f64() > 0.6;
                    if let Some(p) = rested.then(|| self.app.last_pixels.take()).flatten() {
                        self.pixels_before = Some(p);
                        self.recoveries_before = self.app.recoveries;
                        self.recover_sent = true;
                        self.began = Instant::now();
                        self.frames = 0;
                        self.app.send(Cmd::LoseDevice);
                    }
                } else if self.app.recoveries > self.recoveries_before {
                    if let (Some(a), Some(b)) = (&self.pixels_before, &self.app.last_pixels) {
                        let same = a.size == b.size && a.pixels == b.pixels;
                        let (ms, ui) = (self.app.recovery_ms, self.frames);
                        let total = self.began.elapsed().as_secs_f64() * 1e3;
                        self.line(format!(
                            "device loss (injected): rebuilt in {ms:.0} ms, back on screen in {total:.0} ms, {ui} UI frames drawn meanwhile, same picture: {same}"
                        ));
                        self.go(Step::Done);
                    }
                }
            }
            Step::Soak => {
                let t = self.frames as f32;
                self.app.yaw = AUTO_YAW + 0.5 * (t * 0.05).sin();
                let seconds = self.began.elapsed().as_secs_f64();
                if seconds >= self.soak_seconds.unwrap_or(0.0) {
                    let shown = self.app.shown - self.shown_at_step;
                    self.line(format!("soak: {seconds:.0} s, {} UI frames, {shown} pictures, wgpu device still alive", self.frames));
                    self.samples.clear();
                    self.go(Step::Done);
                }
            }
            Step::Destroy => {
                if self.frames == 1 {
                    if let Some(rs) = frame.wgpu_render_state() {
                        rs.device.destroy();
                    }
                }
                // If the watcher is too late, the paint after this frame panics and the run fails loudly.
                if self.frames > 600 {
                    self.line("wgpu device destroyed but the watcher never saw it".into());
                    self.go(Step::Done);
                }
            }
            Step::Done => {
                let mut csv = std::fs::File::create(self.out.join("frames.csv")).unwrap();
                writeln!(csv, "step,dt_ms,cpu_ms,render_ms,latency_ms,new_frame,view").unwrap();
                for s in &self.samples {
                    writeln!(csv, "{},{:.3},{:.3},{:.3},{:.3},{},{}", s.step, s.dt_ms, s.cpu_ms, s.render_ms, s.latency_ms, s.new_frame, s.view).unwrap();
                }
                std::fs::write(self.out.join("summary.txt"), &self.summary).unwrap();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        // The idle step must not keep the window awake, or it would measure nothing.
        if self.step != Step::Idle {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after_secs(IDLE_SECONDS as f32);
        }
    }
}

impl eframe::App for Driver {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if restart::lost() {
            let text = format!("wgpu device lost in step {:?}: {}", self.step, restart::reason());
            self.line(text);
            std::fs::write(self.out.join("summary.txt"), &self.summary).unwrap();
            if self.lose_wgpu {
                restart::restart_and_exit(&[self.out.clone().into()]);
            }
            std::process::exit(3);
        }
        UI_FRAMES.fetch_add(1, Ordering::Relaxed);
        IN_UI.store(true, Ordering::Relaxed);
        self.app.draw(ui);
        let ctx = ui.ctx().clone();
        self.drive(&ctx, frame);
        let state = ctx.input(|i| {
            let v = i.viewport();
            format!("occluded {:?} focused {:?} minimized {:?}", v.occluded, v.focused, v.minimized)
        });
        if let Ok(mut s) = STATE.lock() {
            *s = format!("{:?} shown {} asked {} {state}", self.step, self.app.shown, self.app.asked);
        }
        IN_UI.store(false, Ordering::Relaxed);
    }
}

fn main() -> eframe::Result {
    restart::mark_started();
    heartbeat();
    let args: Vec<String> = std::env::args().collect();
    let out = PathBuf::from(args.get(1).expect("out dir"));
    let no_vsync = args.iter().any(|a| a == "--no-vsync");
    let lose_wgpu = args.iter().any(|a| a == "--lose-wgpu");
    let no_cap = args.iter().any(|a| a == "--no-cap");
    let soak_seconds = args.iter().position(|a| a == "--soak").map(|i| args[i + 1].parse().expect("seconds"));
    let ppp: Option<f32> = args.iter().position(|a| a == "--ppp").map(|i| args[i + 1].parse().expect("ppp"));
    let sizes = if ppp.is_some() { PIXEL_SIZES.to_vec() } else { NATIVE_SIZES.to_vec() };
    std::fs::create_dir_all(&out).unwrap();
    if restart::is_restart() {
        std::fs::write(out.join("restarted.txt"), "the restarted process ran\n").unwrap();
        return Ok(());
    }
    if lose_wgpu {
        restart::restart_on_lost_panic(vec![out.clone().into()]);
    }
    let cache = out.join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    let mut options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default().with_title("Texalith timing run").with_inner_size(NATIVE_SIZES[0]),
        ..Default::default()
    };
    if no_vsync {
        options.wgpu_options.surface.present_mode = wgpu::PresentMode::AutoNoVsync;
    }
    eframe::run_native(
        "Texalith timing run",
        options,
        Box::new(move |cc| {
            if let Some(rs) = &cc.wgpu_render_state {
                restart::watch(rs);
            }
            let dir = bundle::dir();
            let (tx, rx) = preview::spawn(bundle::shaders(&dir), bundle::settings(&dir), cache, cc.egui_ctx.clone());
            let loader = tx.clone();
            std::thread::spawn(move || {
                let _ = loader.send(Cmd::SetContent(testmaps::content(2048)));
            });
            let now = Instant::now();
            // The From normal column is drawn but idle: its channels go nowhere.
            let (jobs, _) = std::sync::mpsc::channel();
            let (_, replies) = std::sync::mpsc::channel();
            let tab = texalith::tab::Tab::new(jobs, replies);
            let (rnm_jobs, _) = std::sync::mpsc::channel();
            let (_, rnm_outputs) = std::sync::mpsc::channel();
            let rnm = texalith::rnm::tab::RnmTab::new(rnm_jobs, rnm_outputs);
            let (cube_jobs, _) = std::sync::mpsc::channel();
            let (_, cube_outputs) = std::sync::mpsc::channel();
            let cube = texalith::cube::tab::CubeTab::new(cube_jobs, cube_outputs);
            let mut app = App::new(tx, rx, bundle::pin(&dir, "build").unwrap_or_default(), tab, rnm, cube);
            if no_cap {
                app.moving_pixels = f32::INFINITY;
            }
            Ok(Box::new(Driver {
                app,
                out,
                step: Step::FirstFrame,
                frames: 0,
                began: now,
                last_ui: now,
                shown_at_step: 0,
                asked_at_step: 0,
                last_shown: 0,
                samples: vec![],
                summary: format!("vsync: {}, moving cap: {}\n", !no_vsync, !no_cap),
                swap_bytes: None,
                swap_sent: 0,
                swap_waiting_for: 0,
                pixels_before: None,
                recover_sent: false,
                recoveries_before: 0,
                soak_seconds,
                lose_wgpu,
                sizes,
                ppp,
            }))
        }),
    )
}
