//! The cube drawn as a labelled horizontal cross. `FACE_LABELS` and the cross layout are copied from Tessalith's
//! `crates/gui/src/model/cube_cross.rs`.
use super::import::HCROSS;
use eframe::egui;
use engine::cube::Cube;
use engine::ops::linear_to_game_gamma;

/// The face names, in face order (+X -X +Y -Y +Z -Z), as drawn on the cross.
pub const FACE_LABELS: [&str; 6] = ["+X east", "\u{2212}X west", "+Y north", "\u{2212}Y south", "+Z up", "\u{2212}Z down"];

/// The largest face edge the cross draws.
pub const PREVIEW_MAX: u32 = 512;

/// The cross of `c`: 4m x 3m, `m` the face edge cut to at most `PREVIEW_MAX` by taking every `k`-th texel, face `k`
/// in cell `HCROSS[k]`, colours in game gamma as the strip stores them, the empty cells transparent.
pub fn cross_image(c: &Cube) -> egui::ColorImage {
    let size = c.size as usize;
    let k = (c.size / PREVIEW_MAX).max(1) as usize;
    let (n, w) = (size / k, 4 * (size / k));
    let mut px = vec![egui::Color32::TRANSPARENT; w * 3 * n];
    let code = |v: f32| (linear_to_game_gamma(v) * 255.0).round() as u8;
    for (face, &(col, row)) in c.faces.iter().zip(&HCROSS) {
        for y in 0..n {
            for x in 0..n {
                let p = face.px[y * k * size + x * k];
                px[(row as usize * n + y) * w + col as usize * n + x] = egui::Color32::from_rgb(code(p[0]), code(p[1]), code(p[2]));
            }
        }
    }
    egui::ColorImage::new([w, 3 * n], px)
}

/// The cross scaled to fit the space left, each face's label at its cell's top-left. `texture` is made on first use;
/// the caller drops it when a new cube arrives.
pub fn draw(ui: &mut egui::Ui, c: &Cube, texture: &mut Option<egui::TextureHandle>) {
    let t = texture.get_or_insert_with(|| ui.ctx().load_texture("cube-cross", cross_image(c), egui::TextureOptions::NEAREST));
    let avail = ui.available_size() - egui::vec2(16.0, 16.0);
    let cell = (avail.x / 4.0).min(avail.y / 3.0).max(8.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(4.0 * cell, 3.0 * cell) + egui::vec2(16.0, 16.0), egui::Sense::hover());
    let rect = rect.shrink(8.0);
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    ui.painter_at(rect).image(t.id(), rect, uv, egui::Color32::WHITE);
    for (label, &(col, row)) in FACE_LABELS.iter().zip(&HCROSS) {
        let at = rect.min + egui::vec2(col as f32 * cell + 4.0, row as f32 * cell + 4.0);
        let text = egui::RichText::new(*label).color(egui::Color32::WHITE).background_color(egui::Color32::from_black_alpha(170));
        ui.put(egui::Rect::from_min_size(at, egui::vec2(cell - 8.0, 20.0)), egui::Label::new(text).truncate());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::import::HCROSS;
    use engine::cube::Cube;
    use engine::image::Rgba32F;
    use engine::ops::linear_to_game_gamma;

    fn code(v: f32) -> u8 {
        (linear_to_game_gamma(v) * 255.0).round() as u8
    }

    #[test]
    fn cross_puts_each_face_in_its_cell() {
        let size = 4u32;
        let colours: [[f32; 4]; 6] = std::array::from_fn(|i| [i as f32 / 8.0 + 0.1, 0.5, 0.25, 1.0]);
        let c = Cube { size, faces: colours.map(|v| Rgba32F::new(size, size, v)) };
        let img = cross_image(&c);
        assert_eq!(img.size, [16, 12]);
        let centre = |col: u32, row: u32| img.pixels[((row * size + 2) * 16 + col * size + 2) as usize];
        for (i, &(col, row)) in HCROSS.iter().enumerate() {
            let v = colours[i];
            assert_eq!(centre(col, row), egui::Color32::from_rgb(code(v[0]), code(v[1]), code(v[2])), "face {i}");
        }
        for (col, row) in [(0, 0), (2, 0), (3, 0), (0, 2), (2, 2), (3, 2)] {
            assert_eq!(centre(col, row), egui::Color32::TRANSPARENT, "cell ({col}, {row}) is empty");
        }
    }

    #[test]
    fn a_big_cube_previews_at_512() {
        let n = 1024u32;
        let mut stripes = Rgba32F::new(n, n, [0.0, 0.0, 0.0, 1.0]);
        for (i, p) in stripes.px.iter_mut().enumerate() {
            if i % 2 == 0 {
                *p = [1.0; 4];
            }
        }
        let blank = Rgba32F::new(n, n, [0.0, 0.0, 0.0, 1.0]);
        let c = Cube { size: n, faces: std::array::from_fn(|i| if i == 0 { stripes.clone() } else { blank.clone() }) };
        let img = cross_image(&c);
        assert_eq!(img.size, [2048, 1536]);
        let (col, row) = HCROSS[0];
        for y in 0..512 {
            for x in 0..512 {
                let p = img.pixels[((row * 512 + y) * 2048 + col * 512 + x) as usize];
                assert_eq!(p, egui::Color32::WHITE, "({x},{y})");
            }
        }
    }

    #[test]
    fn a_face_keeps_its_own_orientation() {
        // One face whose red rises along each row and then down the rows: copied row by row, not transposed.
        let size = 3u32;
        let mut ramp = Rgba32F::new(size, size, [0.0; 4]);
        for (i, p) in ramp.px.iter_mut().enumerate() {
            *p = [i as f32 / 8.0, 0.0, 0.0, 1.0];
        }
        let blank = Rgba32F::new(size, size, [0.0, 0.0, 0.0, 1.0]);
        let c = Cube { size, faces: std::array::from_fn(|i| if i == 0 { ramp.clone() } else { blank.clone() }) };
        let img = cross_image(&c);
        let (col, row) = HCROSS[0];
        for y in 0..size {
            for x in 0..size {
                let p = img.pixels[((row * size + y) * 4 * size + col * size + x) as usize];
                assert_eq!(p.r(), code((y * 3 + x) as f32 / 8.0), "({x},{y})");
            }
        }
    }
}
