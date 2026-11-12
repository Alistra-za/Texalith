//! `render3d` on its own thread, the pattern of Tessalith's `gui/src/model/viewport3d.rs` without its NIF handling.
//! The UI sends commands; the thread takes everything queued, keeps the newest view, and answers with one frame.
use eframe::egui;
use nif::material::MaterialState;
use render3d::device::{Gpu, DEVICE_REMOVED};
use render3d::scene::{Camera, Scene};
use render3d::{Builtin, Renderer, ShapeId, ShapeStatus, Variant};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

/// Map name → material slot. The name is also the texture's file stem.
const SLOTS: [(&str, usize); 5] = [("a", 0), ("n", 1), ("r", 5), ("p", 3), ("e", 4)];

/// With True PBR, these two flags make a two-layer material whose parallax map rests on white.
const TWO_LAYER: u32 = 1 << 24;
const INTERLAYER_PARALLAX: u32 = 1 << 25;

/// True PBR, as the parity harness pinned it. `builtin_default` alone has glossiness 80 and displacement 2.
pub fn pbr(displacement: f32) -> MaterialState {
    let mut m = MaterialState::builtin_default();
    m.slsf2 |= nif::material::SLSF2_TRUE_PBR;
    m.params.glossiness = 0.04;
    m.params.specular_strength = 1.0;
    m.params.lighting_effect_2 = displacement;
    m
}

/// A vanilla environment-mapped material, which reads the cube in slot 4. True PBR never reads that cube.
pub fn vanilla_envmap() -> MaterialState {
    let mut m = MaterialState::builtin_default();
    m.shader_type = 1;
    m.slsf1 |= 1 << 7; // Environment_Mapping
    m.params.trailer = nif::props::LightingTrailer::EnvMap { scale: 1.0 };
    m
}

/// What is on the form. Kept on the thread so a lost device can load it again.
#[derive(Clone)]
pub struct Content {
    pub form: Builtin,
    pub displacement: f32,
    /// DDS bytes by slot name (`a`, `n`, `r`, `p`, or `e` for a cube). A missing map gets the shader's default. With
    /// `e` the form is drawn as a vanilla environment-mapped material.
    pub maps: HashMap<&'static str, Vec<u8>>,
    /// The `p` map rests on white (Flat level White). The plain parallax pivots on mid-grey and would draw such a
    /// map shifted, so it is drawn with the two-layer parallax, which rests on white too.
    pub white: bool,
}

impl Content {
    fn material(&self) -> MaterialState {
        let mut m = if self.maps.contains_key("e") { vanilla_envmap() } else { pbr(self.displacement) };
        for (name, slot) in SLOTS {
            if self.maps.contains_key(name) {
                m.set_slot_raw(slot, &format!("textures\\{name}.dds"));
            }
        }
        if self.white && self.maps.contains_key("p") {
            m.slsf2 |= TWO_LAYER | INTERLAYER_PARALLAX;
        }
        m
    }

    fn resolve(&self, path: &str) -> Option<Vec<u8>> {
        let stem = path.rsplit('/').next()?.strip_suffix(".dds")?;
        self.maps.get(stem).cloned()
    }
}

/// Load `c`; `Err` when the shape's shader could not be built.
pub fn load(r: &mut Renderer, c: &Content) -> Result<(), String> {
    r.load_builtin(c.form, c.material(), &|p: &str| c.resolve(p));
    shape_error(r)
}

fn shape_error(r: &Renderer) -> Result<(), String> {
    r.shapes().into_iter().try_for_each(|s| match s.status {
        ShapeStatus::Failed { error, .. } => Err(error),
        _ => Ok(()),
    })
}

/// A camera orbiting the form; `zoom` is in multiples of Tessalith's auto distance, and the size in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub w: u32,
    pub h: u32,
}

/// Same framing as Tessalith's orbit (`gui/src/model/viewport3d.rs:326-328`).
pub fn scene(view: &View, bounds: ([f32; 3], f32)) -> Scene {
    let (target, radius) = bounds;
    let mut s = Scene::neutral();
    s.camera = Camera::Orbit {
        target,
        yaw: view.yaw,
        pitch: view.pitch,
        distance: radius.max(1e-3) * render3d::scene::AUTO_DISTANCE * view.zoom,
    };
    s
}

pub enum Cmd {
    SetContent(Content),
    SetForm(Builtin),
    SetDisplacement(f32),
    /// Whether the `p` map rests on white.
    SetWhite(bool),
    /// New DDS bytes for one map, without touching the mesh or the compiled shader.
    SwapMap(&'static str, Vec<u8>),
    Render(View),
    /// shortcut: fakes the lost-device error for the next frame; dxcap -forcetdr is the real test but resets the
    /// GPU for the whole machine, so run that by hand.
    LoseDevice,
}

pub enum Reply {
    Ready { adapter: String, warp: bool, fingerprint: String },
    Frame(Frame),
    /// A lost device was replaced and the content loaded again.
    Recovered { adapter: String, warp: bool, ms: f64 },
    /// The preview cannot draw this content, or at all.
    Failed(String),
}

pub struct Frame {
    pub image: egui::ColorImage,
    pub view: View,
    /// When the work behind this frame was asked for, for the input-to-screen measure.
    pub asked: Instant,
    pub render_ms: f64,
}

pub fn spawn(shaders: PathBuf, settings: render3d::cs_settings::CsSettings, cache: PathBuf, ctx: egui::Context) -> (Sender<Cmd>, Receiver<Reply>) {
    let (cmd_tx, cmd_rx) = channel();
    let (reply_tx, reply_rx) = channel();
    std::thread::Builder::new()
        .name("preview".into())
        .spawn(move || {
            let send = |reply: Reply| {
                let _ = reply_tx.send(reply);
                ctx.request_repaint();
            };
            if let Err(e) = run(&cmd_rx, &send, shaders, settings, cache) {
                send(Reply::Failed(e));
            }
        })
        .expect("spawn the preview thread");
    (cmd_tx, reply_rx)
}

fn run(
    rx: &Receiver<Cmd>,
    send: &dyn Fn(Reply),
    shaders: PathBuf,
    settings: render3d::cs_settings::CsSettings,
    cache: PathBuf,
) -> Result<(), String> {
    let fp = render3d::locate::fingerprint(&shaders, None).map_err(|e| e.to_string())?;
    let fingerprint = fp.hash.clone();
    let mut r = Renderer::new(fp, cache)?;
    r.set_settings(settings);
    send(Reply::Ready {
        adapter: r.gpu().adapter.clone(),
        warp: r.gpu().warp,
        fingerprint,
    });

    let mut content: Option<Content> = None;
    let mut last_view: Option<View> = None;
    let mut lose = false;
    while let Ok(first) = rx.recv() {
        let asked = Instant::now();
        let (mut reload, mut material, mut view) = (false, false, None);
        let mut swaps: Vec<&'static str> = vec![];
        for cmd in std::iter::once(first).chain(rx.try_iter()) {
            match cmd {
                Cmd::SetContent(c) => {
                    content = Some(c);
                    reload = true;
                }
                Cmd::SetForm(form) => {
                    if let Some(c) = &mut content {
                        c.form = form;
                        reload = true;
                    }
                }
                Cmd::SetDisplacement(d) => {
                    if let Some(c) = &mut content {
                        c.displacement = d;
                        material = true;
                    }
                }
                Cmd::SetWhite(white) => {
                    if let Some(c) = &mut content {
                        // The flags pick another shader, so the form is loaded again.
                        c.white = white;
                        reload = true;
                    }
                }
                Cmd::SwapMap(name, bytes) => {
                    if let Some(c) = &mut content {
                        c.maps.insert(name, bytes);
                        if !swaps.contains(&name) {
                            swaps.push(name);
                        }
                    }
                }
                Cmd::Render(v) => view = Some(v),
                Cmd::LoseDevice => lose = true,
            }
        }
        // Kept even with nothing to draw yet: the UI's first request can beat the content here.
        last_view = view.or(last_view);
        let Some(c) = &content else { continue };
        let mut failed = None;
        if reload {
            failed = load(&mut r, c).err();
        } else {
            if material {
                let edit = r.set_material(ShapeId(0), c.material(), &|p: &str| c.resolve(p));
                failed = edit.map(|_| ()).and_then(|()| shape_error(&r)).err();
            }
            for name in &swaps {
                let path = format!("textures/{name}.dds");
                if let Err(e) = r.replace_texture(&path, &c.maps[name], Variant::Source) {
                    // A lost device is handled at the render below, which loads the new bytes anyway.
                    if !e.starts_with(DEVICE_REMOVED) {
                        failed = Some(e);
                    }
                }
            }
        }
        // No frame after a failure: it would clear the message and show the old picture as if the edit had worked.
        if let Some(e) = failed {
            send(Reply::Failed(e));
            continue;
        }
        // A change without a new camera still needs a frame.
        let changed = reload || material || !swaps.is_empty() || lose;
        let Some(v) = last_view.filter(|_| view.is_some() || changed) else { continue };
        let Some(bounds) = r.bounds() else {
            send(Reply::Failed("Nothing is loaded to draw.".into()));
            continue;
        };

        let start = Instant::now();
        let s = scene(&v, bounds);
        let mut px = r.render(&s, v.w, v.h);
        if std::mem::take(&mut lose) {
            px = Err(format!("{DEVICE_REMOVED}: simulated"));
        }
        // Lost during a load or at the frame: a new device (maybe software), the content again, one retry.
        if r.gpu().is_removed() || matches!(&px, Err(e) if e.starts_with(DEVICE_REMOVED)) {
            r.rebuild_device_on(Gpu::new()?)?;
            load(&mut r, c)?;
            let bounds = r.bounds().ok_or("no bounds after reloading")?;
            px = r.render(&scene(&v, bounds), v.w, v.h);
            send(Reply::Recovered {
                adapter: r.gpu().adapter.clone(),
                warp: r.gpu().warp,
                ms: start.elapsed().as_secs_f64() * 1e3,
            });
        }
        match px {
            Ok(rgba) => {
                let render_ms = start.elapsed().as_secs_f64() * 1e3;
                // Here, not on the UI thread: it costs up to 4 ms at 2560x1440 (spike 1).
                let image = egui::ColorImage::from_rgba_unmultiplied([v.w as usize, v.h as usize], &rgba);
                send(Reply::Frame(Frame { image, view: v, asked, render_ms }));
            }
            Err(e) => send(Reply::Failed(e)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_white_parallax_map_draws_as_two_layers() {
        let flags = |white, p: bool| {
            let maps = if p { HashMap::from([("p", vec![])]) } else { HashMap::new() };
            Content { form: Builtin::Sphere, displacement: 1.0, maps, white }.material().slsf2 & (3 << 24)
        };
        assert_eq!(flags(true, true), (1 << 24) | (1 << 25));
        assert_eq!((flags(false, true), flags(true, false)), (0, 0));
    }

    #[test]
    fn a_cube_map_makes_a_vanilla_envmap_material() {
        let with = |name| Content { form: Builtin::Sphere, displacement: 1.0, maps: HashMap::from([(name, vec![])]), white: false }.material();
        let e = with("e");
        assert_eq!(e.shader_type, 1);
        assert!(!e.is_pbr() && e.slsf1 & (1 << 7) != 0);
        assert_eq!(e.slots[4].norm, "textures/e.dds");
        assert!(matches!(e.params.trailer, nif::props::LightingTrailer::EnvMap { scale } if scale == 1.0));
        assert!(with("n").is_pbr());
    }
}
