//! The cube thread: import, the Advanced steps, blur and tone. The newest job wins.
use super::adjust::{ground, mirror, tone, turn};
use super::import::{cell, faces, fix, import};
use super::make::{blur, face_size};
use super::open::Opened;
use super::{Flip, Ground, Settings, SourceType};
use engine::cube::Cube;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

pub struct Job {
    pub id: u64,
    pub opened: Arc<Opened>,
    pub source: SourceType,
    pub settings: Settings,
}

pub struct Output {
    pub id: u64,
    pub n: u32,
    /// Linear light, after every step.
    pub cube: Cube,
    /// The 3D preview's map, packed on the cube thread; `None` from `compute` alone.
    pub preview: Option<Arc<crate::preview_maps::Maps>>,
}

/// What an import reads: the face fix only for types with cells, the bottom turn only for a vertical cross.
#[derive(Clone, PartialEq)]
struct ImportKey {
    source: SourceType,
    n: u32,
    fix: Option<([u16; 6], [Flip; 6])>,
    turn_bottom: bool,
}

/// What the shape stage (Turn, Mirror, Ground, blur) reads; the horizon only when a ground is picked.
#[derive(Clone, PartialEq)]
struct ShapeKey {
    turn: f32,
    mirror: bool,
    ground: Ground,
    horizon: f32,
    softness: f32,
}

/// The last import and the last shaped cube, so a tone change neither imports nor blurs again. Holding the `Arc`
/// keeps its address from being reused by another source.
// shortcut: at 2048 the worker holds the imported, shaped and output cubes (about 1.2 GB); drop the import cache
// first if memory runs short.
#[derive(Default)]
pub(crate) struct Cache {
    imported: Option<(Arc<Opened>, ImportKey, Cube)>,
    /// Valid for the import above; dropped with it.
    shaped: Option<(ShapeKey, Cube)>,
    /// How many times the shape stage ran.
    pub shaped_runs: u32,
}

/// Run order: import (with the bottom turn and the face fix) -> Turn -> Mirror -> Ground -> blur -> tone.
pub(crate) fn compute(job: &Job, cache: &mut Cache) -> Output {
    let s = &job.settings;
    let first = &job.opened.imgs[0];
    let n = s.side.unwrap_or_else(|| face_size(s.softness, cell(job.source, first.width, first.height)));
    let key = ImportKey {
        source: job.source,
        n,
        fix: job.source.has_cells().then_some((s.face_turn, s.face_flip)),
        turn_bottom: job.source == SourceType::VCross && s.turn_bottom,
    };
    let hit = cache.imported.as_ref().is_some_and(|(o, k, _)| Arc::ptr_eq(o, &job.opened) && *k == key);
    if !hit {
        // Freed first, so an old source's images and cubes are not held while the new one imports.
        (cache.imported, cache.shaped) = (None, None);
        let mut c = if job.source == SourceType::Faces {
            faces(std::array::from_fn(|k| job.opened.imgs[k].as_ref()), n).expect("open() refuses unequal face files")
        } else {
            import(first, job.source, n, key.turn_bottom)
        };
        if let Some((t, f)) = &key.fix {
            fix(&mut c, t, f);
        }
        cache.imported = Some((job.opened.clone(), key, c));
    }
    let shape = ShapeKey {
        turn: s.turn,
        mirror: s.mirror,
        ground: s.ground,
        horizon: if s.ground == Ground::Keep { 0.0 } else { s.horizon },
        softness: s.softness,
    };
    if cache.shaped.as_ref().map(|(k, _)| k) != Some(&shape) {
        cache.shaped = None;
        let imported = &cache.imported.as_ref().expect("filled above").2;
        let mut c = turn(imported, s.turn);
        if s.mirror {
            c = mirror(&c);
        }
        ground(&mut c, s.ground, s.horizon);
        cache.shaped = Some((shape, blur(&c, f64::from(s.softness))));
        cache.shaped_runs += 1;
    }
    let mut cube = cache.shaped.as_ref().expect("filled above").1.clone();
    tone(&mut cube, s);
    Output { id: job.id, n, cube, preview: None }
}

/// `compute` with the preview's map packed: what the cube thread sends, so the UI thread packs nothing.
pub(crate) fn finish(job: &Job, cache: &mut Cache) -> Output {
    let mut o = compute(job, cache);
    o.preview = Some(Arc::new(crate::preview_maps::cube_only(&o.cube)));
    o
}

/// The cube thread. Before each job it drops every older queued job; `repaint` runs after each output.
pub fn spawn(repaint: impl Fn() + Send + 'static) -> (Sender<Job>, Receiver<Arc<Output>>) {
    let (job_tx, job_rx) = channel::<Job>();
    let (out_tx, out_rx) = channel();
    std::thread::Builder::new()
        .name("cube".into())
        .spawn(move || {
            let mut cache = Cache::default();
            while let Ok(first) = job_rx.recv() {
                let job = job_rx.try_iter().last().unwrap_or(first);
                if out_tx.send(Arc::new(finish(&job, &mut cache))).is_err() {
                    break;
                }
                repaint();
            }
        })
        .expect("spawn the cube thread");
    (job_tx, out_rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::open::Opened;
    use crate::cube::{Settings, SourceType};
    use crate::from_normal::files::Source;
    use engine::image::Rgba32F;
    use std::sync::Arc;
    use std::time::Duration;

    fn panorama() -> Arc<Opened> {
        let mut img = Rgba32F::new(512, 256, [0.0; 4]);
        for (i, p) in img.px.iter_mut().enumerate() {
            *p = [(i % 512) as f32 / 512.0, 0.3, 0.6, 1.0];
        }
        Arc::new(Opened {
            sources: vec![Source { path: "C:/in/sky.png".into(), sha256: "00".into() }],
            imgs: vec![Arc::new(img)],
            detected: Some(SourceType::Panorama),
        })
    }

    fn job(id: u64, opened: &Arc<Opened>, softness: f32) -> Job {
        let settings = Settings { source: Some(SourceType::Panorama), softness, ..Settings::default() };
        Job { id, opened: opened.clone(), source: SourceType::Panorama, settings }
    }

    #[test]
    fn the_newest_job_wins() {
        // The first repaint blocks the thread until jobs 2 and 3 are both queued.
        let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
        let gate = std::sync::Mutex::new(gate_rx);
        let (tx, rx) = spawn(move || {
            let _ = gate.lock().unwrap().recv_timeout(Duration::from_secs(30));
        });
        let o = panorama();
        tx.send(job(1, &o, 0.0)).unwrap();
        let first = rx.recv_timeout(Duration::from_secs(30)).expect("output 1");
        assert_eq!(first.id, 1);
        assert!(first.preview.is_some(), "the thread packs the preview's maps, so the window's thread does not");
        tx.send(job(2, &o, 0.0)).unwrap();
        tx.send(job(3, &o, 0.0)).unwrap();
        drop(gate_tx);
        assert_eq!(rx.recv_timeout(Duration::from_secs(30)).expect("output 3").id, 3);
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err(), "job 2 never arrives");
    }

    fn with(opened: &Arc<Opened>, s: Settings) -> Job {
        Job { id: 1, opened: opened.clone(), source: SourceType::Panorama, settings: s }
    }

    fn base() -> Settings {
        Settings { source: Some(SourceType::Panorama), ..Settings::default() }
    }

    #[test]
    fn the_thread_packs_the_preview_cube() {
        let o = finish(&with(&panorama(), Settings { side: Some(16), ..base() }), &mut Cache::default());
        let maps = o.preview.as_ref().expect("a packed map");
        assert_eq!(**maps, crate::preview_maps::cube_only(&o.cube));
    }

    #[test]
    fn an_explicit_side_size_wins_over_the_source_cell() {
        let o = compute(&with(&panorama(), Settings { side: Some(256), ..base() }), &mut Cache::default());
        assert_eq!((o.n, o.cube.size), (256, 256));
    }

    #[test]
    fn a_face_fix_is_ignored_for_a_panorama() {
        let p = panorama();
        let plain = compute(&with(&p, base()), &mut Cache::default());
        let fixed = compute(&with(&p, Settings { face_turn: [90; 6], ..base() }), &mut Cache::default());
        assert!(plain.cube == fixed.cube);
    }

    #[test]
    fn a_tone_change_reuses_the_shaped_cube() {
        let p = panorama();
        let mut cache = Cache::default();
        let soft = Settings { softness: 0.5, ..base() };
        compute(&with(&p, soft.clone()), &mut cache);
        compute(&with(&p, Settings { brightness: 1.0, ..soft.clone() }), &mut cache);
        compute(&with(&p, Settings { colour: 0.5, ..soft.clone() }), &mut cache);
        assert_eq!(cache.shaped_runs, 1);
        compute(&with(&p, Settings { turn: 30.0, ..soft }), &mut cache);
        assert_eq!(cache.shaped_runs, 2);
    }

    #[test]
    fn the_output_follows_the_face_size_rule() {
        let (tx, rx) = spawn(|| {});
        let o = panorama();
        tx.send(job(1, &o, 0.0)).unwrap();
        let a = rx.recv_timeout(Duration::from_secs(30)).expect("output 1");
        assert_eq!((a.id, a.n, a.cube.size), (1, 128, 128));
        tx.send(job(2, &o, 0.5)).unwrap();
        let b = rx.recv_timeout(Duration::from_secs(60)).expect("output 2");
        assert_eq!((b.id, b.n, b.cube.size), (2, 64, 64));
    }
}
