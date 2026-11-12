//! Texalith's provenance marker: one iTXt chunk, keyword `Texalith`, JSON. It proves a file is Texalith's own when
//! the pixel hash it carries matches the pixels (report: "Overwrite and provenance").
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Cursor;

pub const KEYWORD: &str = "Texalith";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MapKind {
    HeightData,
    Parallax,
    Normal,
    Ao,
    Curvature,
    Cavity,
    Rnm,
    Cube,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Marker {
    pub tool: String,
    pub version: String,
    pub kind: MapKind,
    /// SHA-256, lower-case hex, of the PNG's decoded samples (16-bit big-endian, as stored).
    pub pixels_sha256: String,
    pub source_path: String,
    pub source_sha256: String,
    pub settings: serde_json::Value,
    /// Height data only: max − min of the height in texels at the written size.
    pub span_texels: Option<f64>,
    /// Parallax only: the `pixels_sha256` of the Height data it was made from.
    pub master_sha256: Option<String>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn code(v: f32) -> [u8; 2] {
    ((v.clamp(0.0, 1.0) * 65535.0).round() as u16).to_be_bytes()
}

fn encode(width: u32, height: u32, color: png::ColorType, samples: Vec<u8>, m: &Marker) -> Vec<u8> {
    let mut m = m.clone();
    m.tool = KEYWORD.into();
    m.pixels_sha256 = sha256_hex(&samples);
    let mut out = Vec::new();
    let mut e = png::Encoder::new(&mut out, width, height);
    e.set_color(color);
    e.set_depth(png::BitDepth::Sixteen);
    // Writing to a Vec cannot fail, and the JSON of plain fields always serialises.
    e.add_itxt_chunk(KEYWORD.into(), serde_json::to_string(&m).expect("marker JSON")).expect("iTXt chunk");
    e.write_header()
        .and_then(|mut w| w.write_image_data(&samples))
        .expect("PNG into memory");
    out
}

/// A 16-bit grey PNG with `m` as its marker; `m.pixels_sha256` is filled in here. Codes `round(clamp(v, 0, 1)·65535)`.
pub fn encode_gray16(width: u32, height: u32, px: &[f32], m: &Marker) -> Vec<u8> {
    encode(width, height, png::ColorType::Grayscale, px.iter().flat_map(|&v| code(v)).collect(), m)
}

/// A 16-bit RGB PNG (alpha dropped) with `m` as its marker.
pub fn encode_rgb16(width: u32, height: u32, px: &[[f32; 4]], m: &Marker) -> Vec<u8> {
    let samples = px.iter().flat_map(|p| [code(p[0]), code(p[1]), code(p[2])]).flatten().collect();
    encode(width, height, png::ColorType::Rgb, samples, m)
}

/// A 16-bit RGBA PNG (alpha kept) with `m` as its marker.
pub fn encode_rgba16(width: u32, height: u32, px: &[[f32; 4]], m: &Marker) -> Vec<u8> {
    let samples = px.iter().flat_map(|p| p.map(code)).flatten().collect();
    encode(width, height, png::ColorType::Rgba, samples, m)
}

/// The marker and the decoded samples, or `None` when the bytes are not a PNG.
fn decode(png: &[u8]) -> Option<(Option<Marker>, Vec<u8>)> {
    let mut r = png::Decoder::new(Cursor::new(png)).read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let marker = r
        .info()
        .utf8_text
        .iter()
        .find(|c| c.keyword == KEYWORD)
        .and_then(|c| c.get_text().ok())
        .and_then(|t| serde_json::from_str(&t).ok());
    Some((marker, buf))
}

/// The file's Texalith marker; `None` when there is none or it cannot be read.
pub fn read(png: &[u8]) -> Option<Marker> {
    decode(png)?.0
}

/// The PNG's header says no side exceeds the largest output, `from_normal::MAX_SIZE`. Reads the header only, so a
/// huge file is never decoded.
pub fn fits(png: &[u8]) -> bool {
    let max = crate::from_normal::MAX_SIZE;
    png::Decoder::new(Cursor::new(png)).read_info().is_ok_and(|r| r.info().width <= max && r.info().height <= max)
}

/// A marker exists and its pixel hash matches the decoded pixels. A PNG larger than any output is never ours and is
/// not decoded.
pub fn is_ours(png: &[u8]) -> bool {
    fits(png) && matches!(decode(png), Some((Some(m), px)) if m.tool == KEYWORD && m.pixels_sha256 == sha256_hex(&px))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn sample(kind: MapKind) -> Marker {
        Marker {
            tool: "Texalith".into(),
            version: "test".into(),
            kind,
            pixels_sha256: String::new(),
            source_path: "C:/src/rock_n.png".into(),
            source_sha256: "00".into(),
            settings: serde_json::json!({ "flatten": 0.3 }),
            span_texels: Some(12.5),
            master_sha256: None,
        }
    }

    /// The same pixels re-written with another file's marker.
    fn splice_marker(png: &[u8], m: &Marker) -> Vec<u8> {
        let (w, h, px) = decode_gray(png);
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Grayscale);
        e.set_depth(png::BitDepth::Sixteen);
        e.add_itxt_chunk(KEYWORD.into(), serde_json::to_string(m).unwrap()).unwrap();
        e.write_header().unwrap().write_image_data(&px).unwrap();
        out
    }

    /// The same pixels with no text chunks.
    fn strip_text_chunks(png: &[u8]) -> Vec<u8> {
        let (w, h, px) = decode_gray(png);
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Grayscale);
        e.set_depth(png::BitDepth::Sixteen);
        e.write_header().unwrap().write_image_data(&px).unwrap();
        out
    }

    fn decode_gray(png: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut r = png::Decoder::new(std::io::Cursor::new(png)).read_info().unwrap();
        let mut buf = vec![0; r.output_buffer_size().unwrap()];
        let info = r.next_frame(&mut buf).unwrap();
        buf.truncate(info.buffer_size());
        (info.width, info.height, buf)
    }

    #[test]
    fn a_marker_round_trips_and_proves_the_pixels() {
        let png = encode_gray16(2, 2, &[0.0, 0.25, 0.5, 1.0], &sample(MapKind::HeightData));
        let m = read(&png).unwrap();
        assert_eq!(m.kind, MapKind::HeightData);
        assert_eq!(m.span_texels, Some(12.5));
        assert_eq!(m.tool, "Texalith");
        assert!(is_ours(&png));
        let rgb = encode_rgb16(1, 1, &[[0.5, 0.5, 1.0, 1.0]], &sample(MapKind::Normal));
        assert!(is_ours(&rgb));
    }

    #[test]
    fn rgba16_keeps_alpha_and_is_ours() {
        let alphas = [0.0, 0.25, 1.0, 0.5, 0.75, 0.1];
        let px: Vec<[f32; 4]> = alphas.iter().map(|&a| [0.5, 0.5, 1.0, a]).collect();
        let png = encode_rgba16(3, 2, &px, &sample(MapKind::Rnm));
        let mut r = png::Decoder::new(std::io::Cursor::new(&png)).read_info().unwrap();
        assert_eq!(r.info().color_type, png::ColorType::Rgba);
        assert_eq!(r.info().bit_depth, png::BitDepth::Sixteen);
        let mut buf = vec![0; r.output_buffer_size().unwrap()];
        r.next_frame(&mut buf).unwrap();
        for (i, a) in alphas.iter().enumerate() {
            let s = u16::from_be_bytes([buf[i * 8 + 6], buf[i * 8 + 7]]);
            assert_eq!(s, (a * 65535.0f32).round() as u16, "texel {i}");
        }
        assert!(is_ours(&png));
        assert_eq!(read(&png).unwrap().kind, MapKind::Rnm);
    }

    #[test]
    fn a_png_larger_than_any_output_is_never_ours() {
        let big = encode_gray16(4097, 1, &vec![0.5; 4097], &sample(MapKind::HeightData));
        assert!(!is_ours(&big));
        assert!(!fits(&big));
        assert!(fits(&encode_gray16(2, 2, &[0.5; 4], &sample(MapKind::HeightData))));
    }

    #[test]
    fn an_edited_or_stripped_file_is_not_ours() {
        let png = encode_gray16(2, 2, &[0.0, 0.25, 0.5, 1.0], &sample(MapKind::HeightData));
        let edited = encode_gray16(2, 2, &[0.0, 0.25, 0.5, 0.9], &sample(MapKind::HeightData));
        let forged = splice_marker(&edited, &read(&png).unwrap());
        assert!(read(&forged).is_some());
        assert!(!is_ours(&forged));
        let stripped = strip_text_chunks(&png);
        assert!(read(&stripped).is_none());
        assert!(!is_ours(&stripped));
        assert!(!is_ours(b"not a png"));
    }
}
