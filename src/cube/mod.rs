//! The Cubemap tab: a panorama, cross, strip, mirror ball, angular map, photo or six face files to a `_e.png` strip.
pub mod adjust;
pub mod files;
pub mod import;
pub mod make;
pub mod open;
pub mod tab;
pub mod view;
pub mod worker;

use serde::Serialize;

/// How a source picture wraps around the sphere.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Panorama,
    HCross,
    VCross,
    HStrip,
    VStrip,
    Faces,
    MirrorBall,
    Angular,
    Photo,
}

/// What replaces the ground below the horizon line.
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ground {
    #[default]
    Keep,
    Darken,
    /// sRGB colour code.
    Colour([u8; 3]),
}

/// One face's flip, after its turn.
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Flip {
    #[default]
    None,
    LeftRight,
    TopBottom,
}

/// The tab's settings. Reflection softness is 0..=1 (x 45 degrees of cone), Brightness is in stops; the rest are the
/// Advanced items.
#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct Settings {
    #[serde(rename = "source_type")]
    pub source: Option<SourceType>,
    pub softness: f32,
    pub brightness: f32,
    /// Face edge in pixels; `None` is Auto (`make::face_size`).
    pub side: Option<u32>,
    /// Degrees around the up axis, clockwise seen from above.
    pub turn: f32,
    pub mirror: bool,
    /// White point of the highlight rolloff; `None` is off.
    pub rolloff: Option<f32>,
    /// Colour strength, 1 unchanged, 0 grey.
    pub colour: f32,
    /// sRGB multiply; white changes nothing.
    pub tint: [u8; 3],
    pub ground: Ground,
    /// Degrees above level where the ground starts; read only when `ground` is not `Keep`.
    pub horizon: f32,
    /// Per face +X..-Z: clockwise turn (0, 90, 180, 270), then flip. Read only for types with cells.
    pub face_turn: [u16; 6],
    pub face_flip: [Flip; 6],
    /// Turn a vertical cross's stored -Z cell 180 degrees. Read only for `VCross`.
    pub turn_bottom: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            source: None,
            softness: 0.0,
            brightness: 0.0,
            side: None,
            turn: 0.0,
            mirror: false,
            rolloff: None,
            colour: 1.0,
            tint: [255; 3],
            ground: Ground::Keep,
            horizon: 0.0,
            face_turn: [0; 6],
            face_flip: [Flip::None; 6],
            turn_bottom: true,
        }
    }
}

/// Reflection softness for a newly picked source type: 0.6 for a single photo, which has no real surroundings, else 0.
pub fn default_softness(t: Option<SourceType>) -> f32 {
    if t == Some(SourceType::Photo) {
        0.6
    } else {
        0.0
    }
}

impl SourceType {
    pub const ALL: [SourceType; 9] = [
        SourceType::Panorama,
        SourceType::HCross,
        SourceType::VCross,
        SourceType::HStrip,
        SourceType::VStrip,
        SourceType::Faces,
        SourceType::MirrorBall,
        SourceType::Angular,
        SourceType::Photo,
    ];

    /// Crosses, strips and six face files: the types whose faces are cut out whole, so a face can be turned or flipped.
    pub fn has_cells(self) -> bool {
        matches!(
            self,
            SourceType::HCross | SourceType::VCross | SourceType::HStrip | SourceType::VStrip | SourceType::Faces
        )
    }

    pub fn name(self) -> &'static str {
        match self {
            SourceType::Panorama => "Panorama (2:1)",
            SourceType::HCross => "Horizontal cross (4:3)",
            SourceType::VCross => "Vertical cross (3:4)",
            SourceType::HStrip => "Horizontal strip (6:1)",
            SourceType::VStrip => "Vertical strip (1:6)",
            SourceType::Faces => "Six face files",
            SourceType::MirrorBall => "Mirror ball",
            SourceType::Angular => "Angular map",
            SourceType::Photo => "Single photo",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_change_nothing() {
        let s = Settings::default();
        assert_eq!(s.side, None);
        assert_eq!(s.turn, 0.0);
        assert!(!s.mirror);
        assert_eq!(s.rolloff, None);
        assert_eq!(s.colour, 1.0);
        assert_eq!(s.tint, [255; 3]);
        assert_eq!(s.ground, Ground::Keep);
        assert_eq!(s.horizon, 0.0);
        assert_eq!(s.face_turn, [0; 6]);
        assert_eq!(s.face_flip, [Flip::None; 6]);
        assert!(s.turn_bottom);
        let cells: Vec<_> = SourceType::ALL.into_iter().filter(|t| t.has_cells()).collect();
        assert_eq!(
            cells,
            [SourceType::HCross, SourceType::VCross, SourceType::HStrip, SourceType::VStrip, SourceType::Faces]
        );
    }
}
