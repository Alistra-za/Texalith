//! Generation off the UI thread, in the pattern of `crate::preview`: the tab sends a `Job`, the thread keeps only the
//! newest one queued, and answers with its outputs. The master and every output are cached by their own options, so
//! an output's own slider never re-solves.
use super::input::Kind;
use super::maps::{
    ao, cavity, curvature, height_data, match_depth, normal_from_height, parallax, AoSettings, CurvatureOpts, Gray,
    ParallaxOpts, ParallaxReport,
};
use super::master::{build, BuildError, Green, Master, MasterOpts, MasterReport};
use super::settings::{Settings, Ticks};
use super::sizes;
use crate::resample::resample;
use engine::image::Rgba32F;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// The longest side a draft builds at, while a slider that rebuilds the master is dragged.
pub const DRAFT_SIZE: u32 = 1024;
/// The longest side of the Compare strip's heights.
pub const COMPARE_SIZE: u32 = 128;

pub struct Job {
    pub id: u64,
    pub img: Arc<Rgba32F>,
    pub kind: Kind,
    pub settings: Settings,
    pub draft: bool,
    pub compare: bool,
    /// This job's own flags: the tab sets `cancel` when a newer job replaces it, and reads `progress` (0 to 1000).
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<AtomicU32>,
}

#[derive(Debug, Clone)]
pub struct Outputs {
    pub id: u64,
    pub draft: bool,
    /// The master's width and height.
    pub size: (u32, u32),
    pub report: MasterReport,
    pub match_depth: f32,
    /// Height data and its span in texels; always made, since Parallax and the markers need it.
    pub height: (Gray, f64),
    pub parallax: Option<(Gray, ParallaxReport)>,
    pub normal: Option<Vec<[f32; 4]>>,
    /// The normal the 3D preview binds, DirectX green: the source's, or one from the height.
    pub preview_normal: Vec<[f32; 4]>,
    pub ao: Option<Gray>,
    pub cavity: Option<Gray>,
    pub curvature: Option<Gray>,
    /// Height data at Flatten 0, the job's value and 1, at most `COMPARE_SIZE` on the longer side.
    pub compare: Option<[Gray; 3]>,
    /// The 3D preview's maps, packed on the generation thread; `None` from `compute` alone.
    pub preview: Option<Arc<crate::preview_maps::Maps>>,
}

pub enum Reply {
    Done(Box<Outputs>),
    Failed { id: u64, message: String },
}

type Slot<K, V> = Option<(K, V)>;

/// What the worker keeps between jobs. `gen` names the cached master; every output key starts with it.
#[derive(Default)]
pub struct Cache {
    pub master_builds: u32,
    pub ao_runs: u32,
    img: Option<Arc<Rgba32F>>,
    img_gen: u64,
    master: Slot<(Kind, MasterOpts), Arc<Master>>,
    gen: u64,
    height: Slot<u64, (Gray, f64)>,
    match_depth: Slot<u64, f32>,
    preview_normal: Slot<u64, Vec<[f32; 4]>>,
    parallax: Slot<(u64, ParallaxOpts), (Gray, ParallaxReport)>,
    normal: Slot<(u64, Green), Vec<[f32; 4]>>,
    ao: Slot<(u64, AoSettings), Gray>,
    cavity: Slot<(u64, u32, u32), Gray>,
    curvature: Slot<(u64, CurvatureOpts), Gray>,
    compare: Slot<(u64, Kind, MasterOpts), [Gray; 3]>,
}

/// The cached value under `key`, or `f`'s, stored. `None` from `f` (cancelled) is passed on and not stored.
fn cached<K: PartialEq, V: Clone>(slot: &mut Slot<K, V>, key: K, f: impl FnOnce() -> Option<V>) -> Option<V> {
    if let Some((k, v)) = slot {
        if *k == key {
            return Some(v.clone());
        }
    }
    let v = f()?;
    *slot = Some((key, v.clone()));
    Some(v)
}

/// The outputs of `job`; `Ok(None)` when it was cancelled.
pub fn compute(cache: &mut Cache, job: &Job) -> Result<Option<Outputs>, String> {
    let s = &job.settings;
    let cancel = &*job.cancel;
    let cancelled = || cancel.load(Ordering::Relaxed);
    if !cache.img.as_ref().is_some_and(|i| Arc::ptr_eq(i, &job.img)) {
        cache.img = Some(job.img.clone());
        cache.img_gen += 1;
        cache.master = None;
    }
    let mut mo = s.master.clone();
    if job.draft {
        mo.size = mo.size.min(DRAFT_SIZE);
    }
    let kind = Settings::job_kind(&job.kind);
    let key = (kind.clone(), mo.clone());
    if !matches!(&cache.master, Some((k, _)) if *k == key) {
        let m = match build(&job.img, &kind, &mo, cancel) {
            Ok(m) => m,
            Err(BuildError::Cancelled) => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        cache.master_builds += 1;
        cache.gen += 1;
        cache.master = Some((key, Arc::new(m)));
    }
    let m = cache.master.as_ref().expect("built above").1.clone();
    let gen = cache.gen;

    let height = cached(&mut cache.height, gen, || Some(height_data(&m))).expect("never cancelled");
    let match_depth = cached(&mut cache.match_depth, gen, || Some(match_depth(&m))).expect("never cancelled");
    let preview_normal = cached(&mut cache.preview_normal, gen, || Some(preview_normal(&job.img, &m)))
        .expect("never cancelled");
    let t: Ticks = s.ticks;
    let parallax = t.parallax.then(|| cached(&mut cache.parallax, (gen, s.parallax.clone()), || Some(parallax(&m, &s.parallax))));
    let normal = t.normal.then(|| cached(&mut cache.normal, (gen, s.normal_green), || Some(normal_from_height(&m, s.normal_green))));
    let curvature = t.curvature.then(|| cached(&mut cache.curvature, (gen, s.curvature), || Some(curvature(&m, &s.curvature))));
    if cancelled() {
        return Ok(None);
    }
    let long = m.width.max(m.height);
    let ao_runs = &mut cache.ao_runs;
    let ao = match t.ao {
        true => match cached(&mut cache.ao, (gen, s.ao), || {
            *ao_runs += 1;
            ao(&m, &s.ao, long, &job.progress, cancel)
        }) {
            None => return Ok(None),
            v => Some(v),
        },
        false => None,
    };
    let crevice = s.crevice_at(m.width);
    let depth = s.ao.surface_depth;
    let cavity = match t.cavity {
        true => match cached(&mut cache.cavity, (gen, crevice.to_bits(), depth.to_bits()), || {
            cavity(&m, crevice, depth, &job.progress, cancel)
        }) {
            None => return Ok(None),
            v => Some(v),
        },
        false => None,
    };
    let compare = match job.compare {
        true => match cached(&mut cache.compare, (cache.img_gen, kind.clone(), mo.clone()), || {
            compare(&job.img, &kind, &mo, cancel)
        }) {
            None => return Ok(None),
            v => Some(v),
        },
        false => None,
    };
    job.progress.store(1000, Ordering::Relaxed);
    Ok(Some(Outputs {
        id: job.id,
        draft: job.draft,
        size: (m.width, m.height),
        report: m.report.clone(),
        match_depth,
        height,
        parallax: parallax.flatten(),
        normal: normal.flatten(),
        preview_normal,
        ao: ao.flatten(),
        cavity: cavity.flatten(),
        curvature: curvature.flatten(),
        compare: compare.flatten(),
        preview: None,
    }))
}

/// The preview's DirectX normal: the source at the master's size, green flipped back when it is OpenGL, or one from
/// the height.
fn preview_normal(img: &Rgba32F, m: &Master) -> Vec<[f32; 4]> {
    if !m.from_normal {
        return normal_from_height(m, Green::Directx);
    }
    let flip = m.report.green == Green::Opengl;
    resample(img.clone(), (m.width, m.height))
        .px
        .into_iter()
        .map(|p| if flip { [p[0], 1.0 - p[1], p[2], p[3]] } else { p })
        .collect()
}

/// Height data at Flatten 0, `mo.flatten` and 1, at `COMPARE_SIZE` or the source's largest size if smaller.
fn compare(img: &Rgba32F, kind: &Kind, mo: &MasterOpts, cancel: &AtomicBool) -> Option<[Gray; 3]> {
    let largest = sizes((img.width, img.height)).last().copied().unwrap_or(1);
    let one = |flatten: f32| {
        let o = MasterOpts { size: COMPARE_SIZE.min(largest), flatten, ..mo.clone() };
        build(img, kind, &o, cancel).ok().map(|m| height_data(&m).0)
    };
    Some([one(0.0)?, one(mo.flatten)?, one(1.0)?])
}

/// The generation thread. A cancelled job sends nothing; `repaint` runs after each reply.
pub fn spawn(repaint: impl Fn() + Send + 'static) -> (Sender<Job>, Receiver<Reply>) {
    let (job_tx, job_rx) = channel::<Job>();
    let (reply_tx, reply_rx) = channel();
    std::thread::Builder::new()
        .name("from-normal".into())
        .spawn(move || {
            let mut cache = Cache::default();
            while let Ok(first) = job_rx.recv() {
                let job = job_rx.try_iter().last().unwrap_or(first);
                let reply = match compute(&mut cache, &job) {
                    Ok(Some(mut o)) => {
                        // Packed here, so the UI thread does none of it when the reply lands.
                        o.preview = Some(Arc::new(crate::preview_maps::maps_for(&o)));
                        if job.cancel.load(Ordering::Relaxed) {
                            continue;
                        }
                        Reply::Done(Box::new(o))
                    }
                    Ok(None) => continue,
                    Err(message) => Reply::Failed { id: job.id, message },
                };
                if reply_tx.send(reply).is_err() {
                    break;
                }
                repaint();
            }
        })
        .expect("spawn the from-normal thread");
    (job_tx, reply_rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::from_normal::maps::{height_data, normal_from_height};
    use crate::from_normal::master::tests::{bump_normal, no};
    use crate::from_normal::master::{build, Green};
    use std::time::Duration;

    fn job(img: &Arc<Rgba32F>, kind: Kind, settings: Settings, id: u64) -> Job {
        Job {
            id,
            img: img.clone(),
            kind,
            settings,
            draft: false,
            compare: false,
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(AtomicU32::new(0)),
        }
    }

    fn normal_job(img: &Arc<Rgba32F>, settings: Settings) -> Job {
        job(img, Kind::Normal, settings, 1)
    }

    fn run(c: &mut Cache, j: &Job) -> Outputs {
        compute(c, j).unwrap().expect("not cancelled")
    }

    fn base(n: u32) -> Settings {
        Settings::for_source(&Kind::Normal, (n, n))
    }

    #[test]
    fn an_output_slider_never_rebuilds_the_master_or_reruns_ao() {
        let img = Arc::new(bump_normal(64));
        let mut c = Cache::default();
        let a = run(&mut c, &normal_job(&img, base(64)));
        let mut s = base(64);
        s.parallax.depth = 0.5;
        let b = run(&mut c, &normal_job(&img, s));
        assert_eq!((c.master_builds, c.ao_runs), (1, 1));
        assert_ne!(a.parallax.unwrap().0.px, b.parallax.unwrap().0.px);
    }

    #[test]
    fn a_flatten_change_rebuilds_the_master_and_reruns_ao() {
        let img = Arc::new(bump_normal(64));
        let mut c = Cache::default();
        run(&mut c, &normal_job(&img, base(64)));
        let mut s = base(64);
        s.master.flatten = 0.8;
        run(&mut c, &normal_job(&img, s));
        assert_eq!((c.master_builds, c.ao_runs), (2, 2));
    }

    #[test]
    fn a_draft_runs_at_1024_and_never_above_the_chosen_size() {
        let img = Arc::new(bump_normal(2048));
        let mut s = base(2048);
        s.ticks = Ticks { height: true, parallax: false, normal: false, ao: false, curvature: false, cavity: false };
        s.master.size = 2048;
        let mut c = Cache::default();
        let mut j = normal_job(&img, s.clone());
        j.draft = true;
        let d = run(&mut c, &j);
        assert_eq!((d.size, d.draft), ((1024, 1024), true));
        j.settings.master.size = 512;
        assert_eq!(run(&mut c, &j).size, (512, 512));
        let full = run(&mut c, &normal_job(&img, s));
        assert_eq!((full.size, full.draft), ((2048, 2048), false));
    }

    #[test]
    fn unticked_outputs_are_not_made_but_height_data_always_is() {
        let img = Arc::new(bump_normal(64));
        let mut s = base(64);
        s.ticks = Ticks { height: false, parallax: false, normal: false, ao: false, curvature: false, cavity: false };
        let o = run(&mut Cache::default(), &normal_job(&img, s));
        assert_eq!(o.height.0.px.len(), 64 * 64);
        assert!(o.parallax.is_none() && o.normal.is_none() && o.ao.is_none());
        assert!(o.cavity.is_none() && o.curvature.is_none() && o.compare.is_none());
    }

    #[test]
    fn a_cancelled_job_returns_none_and_each_job_counts_its_own_progress() {
        let img = Arc::new(bump_normal(64));
        let mut c = Cache::default();
        let a = normal_job(&img, base(64));
        a.cancel.store(true, Ordering::Relaxed);
        assert!(compute(&mut c, &a).unwrap().is_none());
        let b = normal_job(&img, base(64));
        run(&mut c, &b);
        assert_eq!(b.progress.load(Ordering::Relaxed), 1000);
        assert_eq!(a.progress.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn the_preview_normal_is_always_directx() {
        let img = Arc::new(bump_normal(64));
        let mut s = base(64);
        s.master.green = Some(Green::Opengl);
        let o = run(&mut Cache::default(), &normal_job(&img, s));
        for (p, src) in o.preview_normal.iter().zip(&img.px) {
            assert!((p[1] - (1.0 - src[1])).abs() < 1e-3, "{p:?} {src:?}");
        }
        let ramp = Rgba32F {
            width: 64,
            height: 64,
            px: (0..64 * 64).map(|i| { let v = (i % 64) as f32 / 63.0; [v, v, v, 1.0] }).collect(),
        };
        let kind = Kind::Height { span_texels: None };
        let s = Settings::for_source(&kind, (64, 64));
        let img = Arc::new(ramp);
        let o = run(&mut Cache::default(), &job(&img, kind.clone(), s.clone(), 1));
        let m = build(&img, &kind, &s.master, &no()).unwrap();
        assert_eq!(o.preview_normal, normal_from_height(&m, Green::Directx));
    }

    #[test]
    fn compare_builds_three_heights_at_128() {
        let img = Arc::new(bump_normal(512));
        let mut s = base(512);
        s.ticks.ao = false;
        let mut j = normal_job(&img, s.clone());
        j.compare = true;
        let three = run(&mut Cache::default(), &j).compare.expect("compare made");
        assert!(three.iter().all(|g| (g.width, g.height) == (128, 128)));
        let mid = build(&img, &Kind::Normal, &MasterOpts { size: 128, ..s.master.clone() }, &no()).unwrap();
        assert_eq!(three[1], height_data(&mid).0);
    }

    #[test]
    fn the_thread_answers_the_newest_job() {
        let (tx, rx) = spawn(|| {});
        let img = Arc::new(bump_normal(64));
        let mut last: Option<Arc<AtomicBool>> = None;
        for id in 1..=5 {
            if let Some(c) = &last {
                c.store(true, Ordering::Relaxed);
            }
            let j = job(&img, Kind::Normal, base(64), id);
            last = Some(j.cancel.clone());
            tx.send(j).unwrap();
        }
        loop {
            match rx.recv_timeout(Duration::from_secs(30)).expect("a reply") {
                Reply::Done(o) if o.id == 5 => {
                    assert!(o.preview.as_ref().is_some_and(|m| m.contains_key("n")), "the thread packs the preview's maps");
                    break;
                }
                Reply::Done(_) => {}
                Reply::Failed { message, .. } => panic!("{message}"),
            }
        }
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err(), "nothing after the newest job");
    }
}
