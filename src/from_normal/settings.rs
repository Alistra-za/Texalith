//! The From normal tab's settings: the master's options, each output's options and ticks, and the JSON every marker
//! carries.
use super::input::Kind;
use super::maps::{default_crevice, AoSettings, CurvatureOpts, FlatLevel, ParallaxOpts};
use super::master::{Edges, Green, MasterOpts, MasterReport};
use super::sizes;
use crate::marker::MapKind;
use serde_json::{json, Value};

/// The output size a new source starts at, at most: a full 4K AO takes 5 s at 16 threads and 12 s at 4 (spike 6).
const DEFAULT_SIZE_MAX: u32 = 2048;

/// Material presets for Flatten broad shapes (report: "Material presets for Flatten").
pub const PRESETS: [(&str, f32); 4] =
    [("Rough stone and ground", 0.0), ("Brick and tile", 1.0), ("Bark, moss, weave", 1.0), ("Carved parts", 1.0)];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ticks {
    pub height: bool,
    pub parallax: bool,
    pub normal: bool,
    pub ao: bool,
    pub curvature: bool,
    pub cavity: bool,
}

impl Ticks {
    /// The tick of output `k`.
    pub fn get_mut(&mut self, k: MapKind) -> &mut bool {
        match k {
            MapKind::HeightData => &mut self.height,
            MapKind::Parallax => &mut self.parallax,
            MapKind::Normal => &mut self.normal,
            MapKind::Ao => &mut self.ao,
            MapKind::Curvature => &mut self.curvature,
            MapKind::Cavity => &mut self.cavity,
            MapKind::Rnm | MapKind::Cube => unreachable!("RNM and Cube are not From normal outputs"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub master: MasterOpts,
    pub parallax: ParallaxOpts,
    /// The Normal output's green.
    pub normal_green: Green,
    pub ao: AoSettings,
    /// Crevice width in texels; `None` is `default_crevice` of the master's width.
    pub crevice: Option<f32>,
    pub curvature: CurvatureOpts,
    pub ticks: Ticks,
}

impl Settings {
    pub fn for_source(kind: &Kind, src: (u32, u32)) -> Settings {
        let largest = sizes(src).last().copied().unwrap_or(1);
        let height_in = matches!(kind, Kind::Height { .. });
        let normal_depth_pct = match kind {
            Kind::Height { span_texels: Some(t) } => (t / f64::from(src.0.max(1)) * 100.0).clamp(0.05, 5.0) as f32,
            _ => 1.0,
        };
        Settings {
            master: MasterOpts { size: largest.min(DEFAULT_SIZE_MAX), normal_depth_pct, ..MasterOpts::default() },
            parallax: ParallaxOpts::default(),
            normal_green: Green::Directx,
            ao: AoSettings::default(),
            crevice: None,
            curvature: CurvatureOpts::default(),
            // Height data starts unticked: few people save it, and it is made for the other maps either way.
            ticks: Ticks { height: false, parallax: true, normal: height_in, ao: true, curvature: false, cavity: false },
        }
    }

    /// The kind a job builds from: a height's depth always comes from the Normal Depth slider, which starts at the
    /// marker's span.
    pub fn job_kind(kind: &Kind) -> Kind {
        match kind {
            Kind::Height { .. } => Kind::Height { span_texels: None },
            k => k.clone(),
        }
    }

    /// The crevice width in texels for a master `width` wide.
    pub fn crevice_at(&self, width: u32) -> f32 {
        self.crevice.unwrap_or_else(|| default_crevice(width))
    }

    /// The marker settings of output `out`: the master's options, the green used, and that output's own options.
    /// `width` is the master's width, which the default crevice follows.
    pub fn json(&self, out: MapKind, report: &MasterReport, width: u32) -> Value {
        let green = |g: Green| if g == Green::Opengl { "opengl" } else { "directx" };
        let p = &self.parallax;
        let output = match out {
            MapKind::HeightData => json!({}),
            MapKind::Parallax => json!({
                "depth": p.depth,
                "flat": if p.flat == FlatLevel::White { "white" } else { "mid-grey" },
                "smooth_texels": p.smooth_texels,
                "invert": p.invert,
            }),
            MapKind::Normal => json!({ "green": green(self.normal_green) }),
            MapKind::Ao => json!({
                "reach": self.ao.reach, "darkness": self.ao.darkness, "surface_depth": self.ao.surface_depth,
            }),
            MapKind::Curvature => json!({ "line_texels": self.curvature.line_texels, "contrast": self.curvature.contrast }),
            MapKind::Cavity => json!({ "crevice_texels": self.crevice_at(width), "surface_depth": self.ao.surface_depth }),
            MapKind::Rnm | MapKind::Cube => unreachable!("RNM and Cube are not From normal outputs"),
        };
        json!({
            "size": self.master.size,
            "edges": if self.master.edges == Edges::Repeats { "repeats" } else { "does-not-repeat" },
            "green": green(report.green),
            "green_guessed": report.green_guessed,
            "flatten": self.master.flatten,
            "normal_depth_pct": self.master.normal_depth_pct,
            "output": output,
        })
    }
}

/// Flatten's readout: shapes wider than this share of the width fade (`solve` damps content coarser than
/// `8·flatten` cycles across the width). `None` at 0, where every shape is kept.
pub fn fade_pct(flatten: f32) -> Option<f32> {
    (flatten > 0.0).then(|| 12.5 / flatten)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::from_normal::input::Kind;
    use crate::from_normal::master::{Edges, Green, MasterReport};
    use crate::marker::MapKind;

    #[test]
    fn defaults_follow_the_source() {
        let n = Settings::for_source(&Kind::Normal, (4096, 4096));
        assert_eq!(n.master.size, 2048); // the source's size, at most 2048: a full 4K AO takes 5 to 12 s (spike 6)
        assert_eq!((n.master.flatten, n.master.edges), (0.3, Edges::Repeats));
        // Height data starts unticked: few people save it.
        assert!(!n.ticks.height && n.ticks.parallax && n.ticks.ao && !n.ticks.normal && !n.ticks.curvature && !n.ticks.cavity);
        assert_eq!(Settings::for_source(&Kind::Normal, (8, 8)).master.size, 8);
        let h = Settings::for_source(&Kind::Height { span_texels: Some(20.48) }, (2048, 2048));
        assert!(h.ticks.normal && !h.ticks.height); // a height file is already the master
        assert!((h.master.normal_depth_pct - 1.0).abs() < 1e-6); // 20.48 / 2048 = 1 %
        let unmarked = Settings::for_source(&Kind::Height { span_texels: None }, (512, 512));
        assert_eq!(unmarked.master.normal_depth_pct, 1.0);
    }

    #[test]
    fn a_height_job_always_takes_its_depth_from_the_slider() {
        assert_eq!(Settings::job_kind(&Kind::Height { span_texels: Some(9.0) }), Kind::Height { span_texels: None });
        assert_eq!(Settings::job_kind(&Kind::Normal), Kind::Normal);
    }

    #[test]
    fn the_flatten_readout_matches_the_spec() {
        assert_eq!(fade_pct(0.3).map(|p| p.round()), Some(42.0)); // spec line 121; solve.rs:28 damps below 8·flatten cycles
        assert_eq!(fade_pct(0.0), None);
        assert_eq!(fade_pct(1.0), Some(12.5));
    }

    #[test]
    fn the_marker_json_holds_the_master_and_that_output_only() {
        let s = Settings::for_source(&Kind::Normal, (1024, 1024));
        let r = MasterReport { green: Green::Opengl, green_guessed: true, ..Default::default() };
        let j = s.json(MapKind::Parallax, &r, 1024);
        assert_eq!(j["size"], 1024);
        assert_eq!(j["edges"], "repeats");
        assert_eq!(j["green"], "opengl");
        assert_eq!(j["green_guessed"], true);
        assert_eq!(j["output"]["depth"], serde_json::json!(0.9f32));
        assert_eq!(j["output"]["flat"], "mid-grey");
        assert!(s.json(MapKind::Ao, &r, 1024)["output"].get("depth").is_none());
        assert_eq!(s.json(MapKind::Cavity, &r, 1024)["output"]["crevice_texels"], serde_json::json!(8.0f32)); // 1024/128
    }
}
