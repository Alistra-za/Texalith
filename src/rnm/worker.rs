//! The RNM blend thread: the newest job wins.
use super::blend::{apply, encode};
use super::Input;
use crate::preview_maps::{self, Maps};
use engine::image::Rgba32F;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

pub struct Job {
    pub id: u64,
    /// Already `decode_base`d.
    pub base: Arc<Rgba32F>,
    /// The layers that have a file, in blend order.
    pub layers: Vec<Input>,
}

pub struct Output {
    pub id: u64,
    pub size: (u32, u32),
    /// Stored texels 0..1 with the base's alpha.
    pub px: Vec<[f32; 4]>,
    /// The 3D preview's maps, packed on the blend thread; `None` from `compute` alone.
    pub preview: Option<Arc<Maps>>,
}

// shortcut: every job resamples each layer again; cache resampled layers by slot and size if drags stutter at 4096.
pub fn compute(job: &Job) -> Output {
    let mut work = (*job.base).clone();
    apply(&mut work, &job.layers);
    Output { id: job.id, size: (work.width, work.height), px: encode(work), preview: None }
}

/// `compute` with the preview's maps packed: what the blend thread sends, so the UI thread packs nothing.
pub fn finish(job: &Job) -> Output {
    let mut o = compute(job);
    o.preview = Some(Arc::new(preview_maps::normal_only(o.size, &o.px)));
    o
}

/// The blend thread. Before each job it drops every older queued job; `repaint` runs after each output.
pub fn spawn(repaint: impl Fn() + Send + 'static) -> (Sender<Job>, Receiver<Arc<Output>>) {
    let (job_tx, job_rx) = channel::<Job>();
    let (out_tx, out_rx) = channel();
    std::thread::Builder::new()
        .name("rnm".into())
        .spawn(move || {
            while let Ok(first) = job_rx.recv() {
                let job = job_rx.try_iter().last().unwrap_or(first);
                if out_tx.send(Arc::new(finish(&job))).is_err() {
                    break;
                }
                repaint();
            }
        })
        .expect("spawn the rnm thread");
    (job_tx, out_rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rnm::blend::{decode_base, encode};
    use crate::rnm::{Input, Kind, Layer};
    use engine::image::Rgba32F;
    use std::sync::Arc;
    use std::time::Duration;

    fn base() -> Arc<Rgba32F> {
        let mut img = Rgba32F::new(8, 4, [0.0; 4]);
        for (i, p) in img.px.iter_mut().enumerate() {
            *p = [0.5 + 0.02 * (i % 5) as f32, 0.5 - 0.01 * (i % 3) as f32, 1.0, i as f32 / 31.0];
        }
        Arc::new(decode_base(&img))
    }

    fn job(id: u64, layers: Vec<Input>) -> Job {
        Job { id, base: base(), layers }
    }

    fn detail() -> Vec<Input> {
        let d = Rgba32F { width: 3, height: 3, px: (0..9).map(|k| [0.3 + 0.05 * k as f32, 0.6, 0.9, 1.0]).collect() };
        vec![Input { set: Layer::new(Kind::Detail), img: Arc::new(d), mask: None, ao: None }]
    }

    #[test]
    fn compute_keeps_size_and_alpha() {
        let j = job(7, detail());
        let o = compute(&j);
        assert_eq!((o.id, o.size), (7, (8, 4)));
        assert!(o.px.iter().zip(&j.base.px).all(|(a, b)| a[3] == b[3]));
        assert!(o.px.iter().zip(encode((*j.base).clone())).any(|(a, b)| a[..3] != b[..3]), "the detail must change something");
    }

    #[test]
    fn the_thread_packs_the_preview_normal() {
        let o = finish(&job(1, vec![]));
        let n = &o.preview.as_ref().expect("packed maps")["n"];
        let u = |at: usize| u32::from_le_bytes(n[at..at + 4].try_into().unwrap());
        assert_eq!((u(16), u(12)), (8, 4)); // the DDS header's width and height
        assert_eq!(**o.preview.as_ref().unwrap(), preview_maps::normal_only(o.size, &o.px));
    }

    #[test]
    fn no_layers_gives_the_base_back() {
        let j = job(1, vec![]);
        let o = compute(&j);
        for (a, b) in o.px.iter().zip(encode((*j.base).clone())) {
            for c in 0..4 {
                assert!((a[c] - b[c]).abs() < 1e-6, "{a:?} vs {b:?}");
            }
        }
    }

    #[test]
    fn the_newest_job_wins() {
        // The first repaint blocks the thread until jobs 2 and 3 are both queued.
        let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
        let gate = std::sync::Mutex::new(gate_rx);
        let (tx, rx) = spawn(move || {
            let _ = gate.lock().unwrap().recv_timeout(Duration::from_secs(10));
        });
        tx.send(job(1, detail())).unwrap();
        let first = rx.recv_timeout(Duration::from_secs(10)).expect("output 1");
        assert_eq!(first.id, 1);
        assert!(first.preview.is_some(), "the thread packs the preview's maps, so the window's thread does not");
        tx.send(job(2, detail())).unwrap();
        tx.send(job(3, detail())).unwrap();
        drop(gate_tx);
        assert_eq!(rx.recv_timeout(Duration::from_secs(10)).expect("output 3").id, 3);
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err(), "job 2 never arrives");
    }
}
