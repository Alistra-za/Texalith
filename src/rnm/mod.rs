//! The RNM tab: blend bent, bump and detail layers into a base tangent normal.
use crate::from_normal::maps::Gray;
use engine::image::Rgba32F;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub mod blend;
pub mod files;
pub mod tab;
pub mod worker;

/// What a layer's image is and how it blends.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A bent normal the base leans toward.
    Bent,
    /// A height image, turned into normals by `blend::bump_normals`; its strength is the bump constant `k`.
    Bump,
    /// A tangent normal reoriented onto the base.
    Detail,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Bent, Kind::Bump, Kind::Detail];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Bent => "Bent",
            Kind::Bump => "Bump",
            Kind::Detail => "Detail",
        }
    }

    pub fn default_strength(self) -> f32 {
        match self {
            Kind::Bent => 0.35,
            Kind::Bump => 0.4,
            Kind::Detail => 0.6,
        }
    }
}

/// One layer's settings. `use_ao` and `ao_power` are read for a Bent layer only.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Layer {
    pub kind: Kind,
    /// For a Bump layer this is `k`, shown as `2k` texels.
    pub strength: f32,
    pub flip: bool,
    pub use_ao: bool,
    pub ao_power: f32,
}

impl Layer {
    pub fn new(kind: Kind) -> Layer {
        Layer { kind, strength: kind.default_strength(), flip: false, use_ao: true, ao_power: 1.5 }
    }
}

/// One layer of a blend: its settings and images. A mask and an AO map are one channel each (`blend::red`).
#[derive(Clone)]
pub struct Input {
    pub set: Layer,
    pub img: Arc<Rgba32F>,
    pub mask: Option<Arc<Gray>>,
    pub ao: Option<Arc<Gray>>,
}
