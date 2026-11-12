// No console window behind a release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use texalith::{app::App, bundle, from_normal::worker, preview, restart, rnm::tab::RnmTab, tab::Tab};

struct Shell(App);

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if restart::lost() {
            // shortcut: the open file is not carried across a restart; pass `--open <path>` in the restart args when
            // someone loses work to it.
            restart::restart_and_exit(&[]);
        }
        self.0.draw(ui);
    }
}

fn main() -> eframe::Result {
    restart::mark_started();
    restart::restart_on_lost_panic(vec![]);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Texalith")
        .with_inner_size([1280.0, 800.0]);
    // The window and taskbar icon; the exe's own icon comes from build.rs.
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/texalith-256.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Texalith",
        options,
        Box::new(|cc| {
            if let Some(rs) = &cc.wgpu_render_state {
                restart::watch(rs);
            }
            let dir = bundle::dir();
            let label = bundle::pin(&dir, "build").unwrap_or_else(|| "unknown build".into());
            let (tx, rx) = preview::spawn(
                bundle::shaders(&dir),
                bundle::settings(&dir),
                bundle::cache_dir(),
                cc.egui_ctx.clone(),
            );
            // No maps yet, so a flat ball until the first outputs arrive.
            let _ = tx.send(preview::Cmd::SetContent(preview::Content {
                form: render3d::Builtin::MaterialBall,
                displacement: 1.0,
                maps: Default::default(),
                white: false,
            }));
            let ctx = cc.egui_ctx.clone();
            let repaint = ctx.clone();
            let (jobs, replies) = worker::spawn(move || ctx.request_repaint());
            let repaint_cube = repaint.clone();
            let (rnm_jobs, rnm_outputs) = texalith::rnm::worker::spawn(move || repaint.request_repaint());
            let (cube_jobs, cube_outputs) = texalith::cube::worker::spawn(move || repaint_cube.request_repaint());
            let cube = texalith::cube::tab::CubeTab::new(cube_jobs, cube_outputs);
            let mut app = App::new(tx, rx, label, Tab::new(jobs, replies), RnmTab::new(rnm_jobs, rnm_outputs), cube);
            app.restarted = restart::is_restart();
            Ok(Box::new(Shell(app)))
        }),
    )
}
