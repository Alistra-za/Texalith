//! The flat view of one output map, fitted and centred, once or repeated 2x2 to show its seams.
use crate::from_normal::maps::Gray;
use crate::from_normal::worker::Outputs;
use crate::marker::MapKind;
use eframe::egui;
use std::collections::HashMap;

pub struct View2d {
    pub map: MapKind,
    pub repeat: bool,
}

impl Default for View2d {
    fn default() -> View2d {
        View2d { map: MapKind::Parallax, repeat: false }
    }
}

pub fn name(k: MapKind) -> &'static str {
    match k {
        MapKind::HeightData => "Height data",
        MapKind::Parallax => "Parallax",
        MapKind::Normal => "Normal",
        MapKind::Ao => "AO",
        MapKind::Curvature => "Curvature",
        MapKind::Cavity => "Cavity",
        MapKind::Rnm => "RNM normal",
        MapKind::Cube => "Cubemap",
    }
}

fn gray_image(g: &Gray) -> egui::ColorImage {
    let px: Vec<u8> = g.px.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect();
    egui::ColorImage::from_gray([g.width as usize, g.height as usize], &px)
}

/// The gray map of kind `k` in `out`, if it holds one.
fn gray(out: &Outputs, k: MapKind) -> Option<&Gray> {
    match k {
        MapKind::HeightData => Some(&out.height.0),
        MapKind::Parallax => out.parallax.as_ref().map(|p| &p.0),
        MapKind::Normal => None,
        MapKind::Ao => out.ao.as_ref(),
        MapKind::Curvature => out.curvature.as_ref(),
        MapKind::Cavity => out.cavity.as_ref(),
        MapKind::Rnm | MapKind::Cube => unreachable!("RNM and Cube are not From normal outputs"),
    }
}

/// Whether `out` holds the map of kind `k` that Save would write.
pub fn made(out: &Outputs, k: MapKind) -> bool {
    match k {
        MapKind::Normal => out.normal.is_some(),
        k => gray(out, k).is_some(),
    }
}

/// The maps `out` holds, in save order. Normal is the preview's normal, which every source has.
pub fn kinds(out: &Outputs) -> Vec<MapKind> {
    use MapKind::*;
    [HeightData, Parallax, Normal, Ao, Curvature, Cavity]
        .into_iter()
        .filter(|&k| k == Normal || gray(out, k).is_some())
        .collect()
}

pub fn image(out: &Outputs, k: MapKind) -> egui::ColorImage {
    if let Some(g) = gray(out, k) {
        return gray_image(g);
    }
    rgb_image(out.size, &out.preview_normal)
}

/// A small copy of map `k` for its section heading: every n-th texel, so the long side is at most 64. `None` when
/// `out` holds no such map. Normal is the Normal output here, not the preview's normal.
pub fn thumb(out: &Outputs, k: MapKind) -> Option<egui::ColorImage> {
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    // The texel indices kept, and the size they make.
    let pick = |w: usize, h: usize| {
        let n = w.max(h).div_ceil(64).max(1);
        ((0..h).step_by(n).flat_map(move |y| (0..w).step_by(n).map(move |x| y * w + x)), [w.div_ceil(n), h.div_ceil(n)])
    };
    if k == MapKind::Normal {
        let px = out.normal.as_ref()?;
        let (at, size) = pick(out.size.0 as usize, out.size.1 as usize);
        let rgb: Vec<u8> = at.flat_map(|i| [byte(px[i][0]), byte(px[i][1]), byte(px[i][2])]).collect();
        return Some(egui::ColorImage::from_rgb(size, &rgb));
    }
    let g = gray(out, k)?;
    let (at, size) = pick(g.width as usize, g.height as usize);
    let px: Vec<u8> = at.map(|i| byte(g.px[i])).collect();
    Some(egui::ColorImage::from_gray(size, &px))
}

/// An RGB image of stored texels (0..1), alpha dropped.
pub fn rgb_image(size: (u32, u32), px: &[[f32; 4]]) -> egui::ColorImage {
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    let px: Vec<u8> = px.iter().flat_map(|p| [byte(p[0]), byte(p[1]), byte(p[2])]).collect();
    egui::ColorImage::from_rgb([size.0 as usize, size.1 as usize], &px)
}

/// A "Map" combo over `maps` (at least one), a "Repeat 2x2" tick, then the map from `image`. `textures` holds one
/// texture per map and is cleared by the caller when new outputs arrive.
pub fn draw(
    ui: &mut egui::Ui,
    v: &mut View2d,
    size: (u32, u32),
    maps: &[MapKind],
    image: &dyn Fn(MapKind) -> egui::ColorImage,
    textures: &mut HashMap<MapKind, egui::TextureHandle>,
) {
    if !maps.contains(&v.map) {
        v.map = maps[0];
    }
    ui.horizontal(|ui| {
        egui::ComboBox::from_label("Map").selected_text(name(v.map)).show_ui(ui, |ui| {
            for k in maps {
                ui.selectable_value(&mut v.map, *k, name(*k));
            }
        });
        ui.checkbox(&mut v.repeat, "Repeat 2x2");
    });
    let tex = textures
        .entry(v.map)
        .or_insert_with(|| ui.ctx().load_texture(name(v.map), image(v.map), egui::TextureOptions::LINEAR));
    let n = if v.repeat { 2 } else { 1 };
    let area = ui.available_rect_before_wrap();
    let (w, h) = (size.0 as f32 * n as f32, size.1 as f32 * n as f32);
    let scale = (area.width() / w).min(area.height() / h);
    let whole = egui::Rect::from_center_size(area.center(), egui::vec2(w, h) * scale);
    let cell = whole.size() / n as f32;
    for i in 0..n * n {
        let min = whole.min + egui::vec2((i % n) as f32 * cell.x, (i / n) as f32 * cell.y);
        let r = ui.put(egui::Rect::from_min_size(min, cell), egui::Image::new(&*tex).fit_to_exact_size(cell));
        r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, name(v.map)));
    }
}
